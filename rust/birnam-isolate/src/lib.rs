//! Wire and worker primitives for the Isolate wire-v1 protocol.
//!
//! The Rust rung owns copied-value transport, worker dispatch, and the
//! process-local host registry. Every floor object crossing this boundary is
//! reconstructed through the frozen `IdApi`; only opaque process handles and
//! owned wire buffers survive between host calls.

use birnam_abi::{IdApi, MarkFn, Oop};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(unix)]
mod asynchronous;
#[cfg(unix)]
mod service;
use std::time::Duration;

pub const MAGIC: &[u8; 4] = b"BISO";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: usize = 20;
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_DEPTH: usize = 64;
pub const MAX_CONTAINER: usize = 65_536;
pub const MAX_NODES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Ready = 1,
    BootError = 2,
    Request = 3,
    ReplyOk = 4,
    ReplyError = 5,
    Stop = 6,
    Stopped = 7,
    ProtocolError = 8,
}

impl FrameKind {
    fn parse(raw: u8) -> Result<Self, CodecError> {
        match raw {
            1 => Ok(Self::Ready),
            2 => Ok(Self::BootError),
            3 => Ok(Self::Request),
            4 => Ok(Self::ReplyOk),
            5 => Ok(Self::ReplyError),
            6 => Ok(Self::Stop),
            7 => Ok(Self::Stopped),
            8 => Ok(Self::ProtocolError),
            _ => Err(CodecError::UnknownKind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub kind: FrameKind,
    pub request_id: u64,
    pub payload_len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    BadMagic,
    BadVersion,
    BadFlags,
    UnknownKind,
    InvalidRequestId,
    FrameTooLarge,
    Truncated,
    TrailingBytes,
    UnknownTag,
    TooDeep,
    TooManyNodes,
    ContainerTooLarge,
    InvalidUtf8,
    InvalidMapKey,
    DuplicateMapKey,
    UnknownRecordSchema,
    InvalidRecord,
    CyclicValue,
    NonTransferable,
    OutOfMemory,
}

pub fn encode_header(header: Header) -> Result<[u8; HEADER_LEN], CodecError> {
    if header.payload_len as usize > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    validate_request_id(header.kind, header.request_id)?;
    let mut out = [0u8; HEADER_LEN];
    out[..4].copy_from_slice(MAGIC);
    out[4..6].copy_from_slice(&VERSION.to_be_bytes());
    out[6] = header.kind as u8;
    out[7] = 0;
    out[8..16].copy_from_slice(&header.request_id.to_be_bytes());
    out[16..20].copy_from_slice(&header.payload_len.to_be_bytes());
    Ok(out)
}

pub fn decode_header(bytes: &[u8]) -> Result<Header, CodecError> {
    if bytes.len() < HEADER_LEN {
        return Err(CodecError::Truncated);
    }
    if &bytes[..4] != MAGIC {
        return Err(CodecError::BadMagic);
    }
    if u16::from_be_bytes([bytes[4], bytes[5]]) != VERSION {
        return Err(CodecError::BadVersion);
    }
    if bytes[7] != 0 {
        return Err(CodecError::BadFlags);
    }
    let kind = FrameKind::parse(bytes[6])?;
    let request_id = u64::from_be_bytes(bytes[8..16].try_into().expect("header slice"));
    let payload_len = u32::from_be_bytes(bytes[16..20].try_into().expect("header slice"));
    if payload_len as usize > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    validate_request_id(kind, request_id)?;
    Ok(Header {
        kind,
        request_id,
        payload_len,
    })
}

/// An owned BISO frame. The payload is copied so callers never retain a
/// borrowed transport buffer across a worker or callback boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub header: Header,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        validate_empty_payload(self.header.kind, self.payload.len())?;
        let payload_len =
            u32::try_from(self.payload.len()).map_err(|_| CodecError::FrameTooLarge)?;
        let header = encode_header(Header {
            payload_len,
            ..self.header
        })?;
        let mut bytes = Vec::with_capacity(HEADER_LEN + self.payload.len());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let header = decode_header(bytes)?;
        let payload_len = header.payload_len as usize;
        validate_empty_payload(header.kind, payload_len)?;
        let frame_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(CodecError::FrameTooLarge)?;
        if bytes.len() < frame_len {
            return Err(CodecError::Truncated);
        }
        if bytes.len() > frame_len {
            return Err(CodecError::TrailingBytes);
        }
        Ok(Self {
            header,
            payload: bytes[HEADER_LEN..frame_len].to_vec(),
        })
    }
}

fn validate_empty_payload(kind: FrameKind, payload_len: usize) -> Result<(), CodecError> {
    if matches!(
        kind,
        FrameKind::Ready | FrameKind::Stop | FrameKind::Stopped
    ) && payload_len != 0
    {
        return Err(CodecError::InvalidRecord);
    }
    Ok(())
}

fn validate_request_id(kind: FrameKind, request_id: u64) -> Result<(), CodecError> {
    if matches!(kind, FrameKind::Ready | FrameKind::BootError) && request_id != 0 {
        return Err(CodecError::InvalidRequestId);
    }
    if !matches!(
        kind,
        FrameKind::Ready | FrameKind::BootError | FrameKind::ProtocolError
    ) && request_id == 0
    {
        return Err(CodecError::InvalidRequestId);
    }
    if request_id > i64::MAX as u64 {
        return Err(CodecError::InvalidRequestId);
    }
    Ok(())
}

/// The owned, pointer-free representation used by the portable value codec.
/// Strings and symbols retain bytes rather than borrowing input memory.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    False,
    True,
    Int(i64),
    Float(u64),
    String(Vec<u8>),
    Symbol(Vec<u8>),
    Vector(Vec<Value>),
    Map(Vec<(Value, Value)>),
}

impl Value {
    pub fn float(value: f64) -> Self {
        Self::Float(value.to_bits())
    }

    pub fn as_float(self) -> Option<f64> {
        match self {
            Self::Float(bits) => Some(f64::from_bits(bits)),
            _ => None,
        }
    }
}

const TAG_NIL: u8 = 0;
const TAG_FALSE: u8 = 1;
const TAG_TRUE: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_FLOAT: u8 = 4;
const TAG_STRING: u8 = 5;
const TAG_SYMBOL: u8 = 6;
const TAG_VECTOR: u8 = 7;
const TAG_MAP: u8 = 8;
const TAG_RECORD: u8 = 9;

struct Encoder {
    bytes: Vec<u8>,
    nodes: usize,
}

impl Encoder {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            nodes: 0,
        }
    }

    fn reserve(&self, additional: usize) -> Result<(), CodecError> {
        let end = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or(CodecError::FrameTooLarge)?;
        if end > MAX_FRAME {
            return Err(CodecError::FrameTooLarge);
        }
        Ok(())
    }

    fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.reserve(1)?;
        self.bytes.push(value);
        Ok(())
    }

    fn u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.reserve(4)?;
        self.bytes.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.reserve(8)?;
        self.bytes.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn sized(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        if bytes.len() > MAX_FRAME || bytes.len() > u32::MAX as usize {
            return Err(CodecError::FrameTooLarge);
        }
        self.u32(bytes.len() as u32)?;
        self.reserve(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn value(&mut self, value: &Value, depth: usize) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::TooDeep);
        }
        if self.nodes == MAX_NODES {
            return Err(CodecError::TooManyNodes);
        }
        self.nodes += 1;
        match value {
            Value::Nil => self.byte(TAG_NIL),
            Value::False => self.byte(TAG_FALSE),
            Value::True => self.byte(TAG_TRUE),
            Value::Int(value) => {
                self.byte(TAG_INT)?;
                self.u64(*value as u64)
            }
            Value::Float(bits) => {
                self.byte(TAG_FLOAT)?;
                self.u64(*bits)
            }
            Value::String(bytes) => {
                self.byte(TAG_STRING)?;
                self.sized(bytes)
            }
            Value::Symbol(bytes) => {
                if std::str::from_utf8(bytes).is_err() || bytes.contains(&0) {
                    return Err(CodecError::InvalidUtf8);
                }
                self.byte(TAG_SYMBOL)?;
                self.sized(bytes)
            }
            Value::Vector(values) => {
                if values.len() > MAX_CONTAINER {
                    return Err(CodecError::ContainerTooLarge);
                }
                self.byte(TAG_VECTOR)?;
                self.u32(values.len() as u32)?;
                for value in values {
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
            Value::Map(entries) => {
                if entries.len() > MAX_CONTAINER {
                    return Err(CodecError::ContainerTooLarge);
                }
                self.byte(TAG_MAP)?;
                self.u32(entries.len() as u32)?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if !valid_map_key(key) {
                        return Err(CodecError::InvalidMapKey);
                    }
                    if entries[..index].iter().any(|(previous, _)| previous == key) {
                        return Err(CodecError::DuplicateMapKey);
                    }
                    self.value(key, depth + 1)?;
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
        }
    }
}

pub fn encode_value(value: &Value) -> Result<Vec<u8>, CodecError> {
    let mut encoder = Encoder::new();
    encoder.value(value, 0)?;
    Ok(encoder.bytes)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
    nodes: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            nodes: 0,
        }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(CodecError::Truncated)?;
        if end > self.bytes.len() {
            return Err(CodecError::Truncated);
        }
        let out = &self.bytes[self.position..end];
        self.position = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("u32 slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("u64 slice"),
        ))
    }

    fn sized(&mut self) -> Result<&'a [u8], CodecError> {
        let length = self.u32()? as usize;
        if length > MAX_FRAME {
            return Err(CodecError::FrameTooLarge);
        }
        self.take(length)
    }

    fn value(&mut self, depth: usize) -> Result<Value, CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::TooDeep);
        }
        if self.nodes == MAX_NODES {
            return Err(CodecError::TooManyNodes);
        }
        self.nodes += 1;
        match self.byte()? {
            TAG_NIL => Ok(Value::Nil),
            TAG_FALSE => Ok(Value::False),
            TAG_TRUE => Ok(Value::True),
            TAG_INT => Ok(Value::Int(self.u64()? as i64)),
            TAG_FLOAT => Ok(Value::Float(self.u64()?)),
            TAG_STRING => Ok(Value::String(self.sized()?.to_vec())),
            TAG_SYMBOL => {
                let bytes = self.sized()?;
                if std::str::from_utf8(bytes).is_err() || bytes.contains(&0) {
                    return Err(CodecError::InvalidUtf8);
                }
                Ok(Value::Symbol(bytes.to_vec()))
            }
            TAG_VECTOR => {
                let count = self.u32()? as usize;
                if count > MAX_CONTAINER {
                    return Err(CodecError::ContainerTooLarge);
                }
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(self.value(depth + 1)?);
                }
                Ok(Value::Vector(values))
            }
            TAG_MAP => {
                let count = self.u32()? as usize;
                if count > MAX_CONTAINER {
                    return Err(CodecError::ContainerTooLarge);
                }
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    let key = self.value(depth + 1)?;
                    if !valid_map_key(&key) {
                        return Err(CodecError::InvalidMapKey);
                    }
                    if entries.iter().any(|(previous, _)| previous == &key) {
                        return Err(CodecError::DuplicateMapKey);
                    }
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                }
                Ok(Value::Map(entries))
            }
            TAG_RECORD => Err(CodecError::UnknownRecordSchema),
            _ => Err(CodecError::UnknownTag),
        }
    }
}

pub fn decode_value(bytes: &[u8]) -> Result<Value, CodecError> {
    if bytes.len() > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    let mut decoder = Decoder::new(bytes);
    let value = decoder.value(0)?;
    if decoder.position != bytes.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok(value)
}

fn error_code(error: CodecError) -> i32 {
    match error {
        CodecError::BadMagic => 1,
        CodecError::BadVersion => 2,
        CodecError::BadFlags => 3,
        CodecError::UnknownKind => 4,
        CodecError::InvalidRequestId => 5,
        CodecError::FrameTooLarge => 6,
        CodecError::Truncated => 7,
        CodecError::TrailingBytes => 8,
        CodecError::UnknownTag => 9,
        CodecError::TooDeep => 10,
        CodecError::TooManyNodes => 11,
        CodecError::ContainerTooLarge => 12,
        CodecError::InvalidUtf8 => 13,
        CodecError::InvalidMapKey => 14,
        CodecError::DuplicateMapKey => 15,
        CodecError::UnknownRecordSchema => 16,
        CodecError::InvalidRecord => 17,
        CodecError::CyclicValue => 18,
        CodecError::NonTransferable => 19,
        CodecError::OutOfMemory => 20,
    }
}

const BOOT_ERROR_SCHEMA: &[u8] = b"birnam.boot-error.v1";
const APPLICATION_ERROR_SCHEMA: &[u8] = b"birnam.application-error.v1";
const PROTOCOL_ERROR_SCHEMA: &[u8] = b"birnam.protocol-error.v1";

fn error_record(schema: &[u8], kind: &[u8], message: &[u8]) -> Result<Vec<u8>, CodecError> {
    if schema.len() > u16::MAX as usize || kind.len() > MAX_FRAME || message.len() > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    if std::str::from_utf8(kind).is_err() || kind.contains(&0) {
        return Err(CodecError::InvalidUtf8);
    }
    let mut payload = Vec::new();
    payload.push(TAG_RECORD);
    payload.extend_from_slice(&(schema.len() as u16).to_be_bytes());
    payload.extend_from_slice(schema);
    payload.extend_from_slice(&2u16.to_be_bytes());
    payload.push(TAG_SYMBOL);
    payload.extend_from_slice(&(kind.len() as u32).to_be_bytes());
    payload.extend_from_slice(kind);
    payload.push(TAG_STRING);
    payload.extend_from_slice(&(message.len() as u32).to_be_bytes());
    payload.extend_from_slice(message);
    if payload.len() > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    Ok(payload)
}

fn codec_error_name(error: CodecError) -> &'static [u8] {
    match error {
        CodecError::BadMagic => b"BadMagic",
        CodecError::BadVersion => b"BadVersion",
        CodecError::BadFlags => b"BadFlags",
        CodecError::UnknownKind => b"UnknownKind",
        CodecError::InvalidRequestId => b"InvalidRequestId",
        CodecError::FrameTooLarge => b"FrameTooLarge",
        CodecError::Truncated => b"Truncated",
        CodecError::TrailingBytes => b"TrailingBytes",
        CodecError::UnknownTag => b"UnknownTag",
        CodecError::TooDeep => b"TooDeep",
        CodecError::TooManyNodes => b"TooManyNodes",
        CodecError::ContainerTooLarge => b"ContainerTooLarge",
        CodecError::InvalidUtf8 => b"InvalidUtf8",
        CodecError::InvalidMapKey => b"InvalidMapKey",
        CodecError::DuplicateMapKey => b"DuplicateMapKey",
        CodecError::UnknownRecordSchema => b"UnknownRecordSchema",
        CodecError::InvalidRecord => b"InvalidRecord",
        CodecError::CyclicValue => b"CyclicValue",
        CodecError::NonTransferable => b"NonTransferable",
        CodecError::OutOfMemory => b"OutOfMemory",
    }
}

struct OopEncoder<'a> {
    api: &'a IdApi,
    bytes: Vec<u8>,
    nodes: usize,
    active: Vec<usize>,
}

impl<'a> OopEncoder<'a> {
    fn new(api: &'a IdApi) -> Self {
        Self {
            api,
            bytes: Vec::new(),
            nodes: 0,
            active: Vec::new(),
        }
    }

    fn reserve(&self, additional: usize) -> Result<(), CodecError> {
        let end = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or(CodecError::FrameTooLarge)?;
        if end > MAX_FRAME {
            return Err(CodecError::FrameTooLarge);
        }
        Ok(())
    }

    fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.reserve(1)?;
        self.bytes.push(value);
        Ok(())
    }

    fn sized(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        if bytes.len() > u32::MAX as usize {
            return Err(CodecError::FrameTooLarge);
        }
        self.reserve(4 + bytes.len())?;
        self.bytes
            .extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn count_node(&mut self, depth: usize) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::TooDeep);
        }
        if self.nodes == MAX_NODES {
            return Err(CodecError::TooManyNodes);
        }
        self.nodes += 1;
        Ok(())
    }

    fn value(&mut self, value: Oop, depth: usize) -> Result<(), CodecError> {
        self.count_node(depth)?;
        if value == self.api.nilObj {
            return self.byte(TAG_NIL);
        }
        if value == self.api.falseObj {
            return self.byte(TAG_FALSE);
        }
        if value == self.api.trueObj {
            return self.byte(TAG_TRUE);
        }
        if value.is_null() {
            return Err(CodecError::NonTransferable);
        }
        if unsafe { (birnam_abi::integer::is_integer)(value) } != 0 {
            self.byte(TAG_INT)?;
            let integer = unsafe { (birnam_abi::integer::value)(value) };
            self.reserve(8)?;
            self.bytes
                .extend_from_slice(&(integer as u64).to_be_bytes());
            return Ok(());
        }
        if unsafe { (birnam_abi::float::is_float)(value) } != 0 {
            self.byte(TAG_FLOAT)?;
            let float = unsafe { (birnam_abi::float::value)(value) };
            self.reserve(8)?;
            self.bytes.extend_from_slice(&float.to_bits().to_be_bytes());
            return Ok(());
        }
        let vtable = unsafe { (self.api.vtableOf)(value) };
        if vtable == unsafe { birnam_abi::string::behavior() } {
            let mut length = 0usize;
            let pointer = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
            if length != 0 && pointer.is_null() {
                return Err(CodecError::InvalidRecord);
            }
            self.byte(TAG_STRING)?;
            let bytes = if length == 0 {
                &[]
            } else {
                // SAFETY: the floor promises `stringBytes`'s returned extent
                // remains readable for this synchronous call.
                unsafe { std::slice::from_raw_parts(pointer, length) }
            };
            return self.sized(bytes);
        }
        if vtable == self.api.symbol_vt {
            let pointer = unsafe { (self.api.symbolName)(value) };
            if pointer.is_null() {
                return Err(CodecError::InvalidRecord);
            }
            // SAFETY: value remains an active encoder root while this borrowed
            // name is synchronously copied into the Rust-owned output buffer.
            let bytes = unsafe { CStr::from_ptr(pointer) }.to_bytes();
            if std::str::from_utf8(bytes).is_err() {
                return Err(CodecError::InvalidUtf8);
            }
            self.byte(TAG_SYMBOL)?;
            return self.sized(bytes);
        }

        let vector_vt = unsafe { (self.api.import)(c"VectorVt".as_ptr()) };
        let map_vt = unsafe { (self.api.import)(c"MapVt".as_ptr()) };
        if vtable != vector_vt && vtable != map_vt {
            return Err(CodecError::NonTransferable);
        }
        let address = value as usize;
        if self.active.contains(&address) {
            return Err(CodecError::CyclicValue);
        }
        self.active.push(address);
        let result = if vtable == vector_vt {
            self.vector(value, depth)
        } else {
            self.map(value, depth)
        };
        self.active.pop();
        result
    }

    fn size(&self, object: Oop) -> Result<usize, CodecError> {
        let selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let size = unsafe { (self.api.send)(object, selector, ptr::null(), 0) };
        if unsafe { (birnam_abi::integer::is_integer)(size) } == 0 {
            return Err(CodecError::NonTransferable);
        }
        let size = unsafe { (birnam_abi::integer::value)(size) };
        if size < 0 {
            return Err(CodecError::ContainerTooLarge);
        }
        let size = usize::try_from(size).map_err(|_| CodecError::ContainerTooLarge)?;
        if size > MAX_CONTAINER {
            return Err(CodecError::ContainerTooLarge);
        }
        Ok(size)
    }

    fn at(&self, object: Oop, index: Oop) -> Oop {
        let selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        unsafe { (self.api.send)(object, selector, &index, 1) }
    }

    fn vector(&mut self, object: Oop, depth: usize) -> Result<(), CodecError> {
        let size = self.size(object)?;
        self.byte(TAG_VECTOR)?;
        self.reserve(4)?;
        self.bytes.extend_from_slice(&(size as u32).to_be_bytes());
        for index in 0..size {
            let item = self.at(object, unsafe { (birnam_abi::integer::make)(index as i64) });
            self.value(item, depth + 1)?;
        }
        Ok(())
    }

    fn valid_map_key(&self, key: Oop) -> bool {
        if key == self.api.nilObj || key == self.api.falseObj || key == self.api.trueObj {
            return true;
        }
        if unsafe { (birnam_abi::integer::is_integer)(key) } != 0 {
            return true;
        }
        let vtable = unsafe { (self.api.vtableOf)(key) };
        vtable == unsafe { birnam_abi::string::behavior() } || vtable == self.api.symbol_vt
    }

    fn map(&mut self, object: Oop, depth: usize) -> Result<(), CodecError> {
        let size = self.size(object)?;
        let keys_selector = unsafe { (self.api.intern)(c"keys".as_ptr()) };
        let keys = unsafe { (self.api.send)(object, keys_selector, ptr::null(), 0) };
        if keys.is_null() {
            return Err(CodecError::NonTransferable);
        }
        if self.size(keys)? != size {
            return Err(CodecError::InvalidRecord);
        }
        let at_put_size = size;
        self.byte(TAG_MAP)?;
        self.reserve(4)?;
        self.bytes
            .extend_from_slice(&(at_put_size as u32).to_be_bytes());
        for index in 0..size {
            let key = self.at(keys, unsafe { (birnam_abi::integer::make)(index as i64) });
            if !self.valid_map_key(key) {
                return Err(CodecError::InvalidMapKey);
            }
            self.value(key, depth + 1)?;
            let value = self.at(object, key);
            self.value(value, depth + 1)?;
        }
        Ok(())
    }
}

struct OopDecoder<'a> {
    api: &'a IdApi,
    bytes: &'a [u8],
    position: usize,
    nodes: usize,
}

impl<'a> OopDecoder<'a> {
    fn new(api: &'a IdApi, bytes: &'a [u8]) -> Self {
        Self {
            api,
            bytes,
            position: 0,
            nodes: 0,
        }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(CodecError::Truncated)?;
        if end > self.bytes.len() {
            return Err(CodecError::Truncated);
        }
        let out = &self.bytes[self.position..end];
        self.position = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("u32 slice"),
        ))
    }

    fn u16(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("u16 slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("u64 slice"),
        ))
    }

    fn sized(&mut self) -> Result<&'a [u8], CodecError> {
        let length = self.u32()? as usize;
        if length > MAX_FRAME {
            return Err(CodecError::FrameTooLarge);
        }
        self.take(length)
    }

    fn count_node(&mut self, depth: usize) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::TooDeep);
        }
        if self.nodes == MAX_NODES {
            return Err(CodecError::TooManyNodes);
        }
        self.nodes += 1;
        Ok(())
    }

    fn value(&mut self, depth: usize) -> Result<Oop, CodecError> {
        self.count_node(depth)?;
        match self.byte()? {
            TAG_NIL => Ok(self.api.nilObj),
            TAG_FALSE => Ok(self.api.falseObj),
            TAG_TRUE => Ok(self.api.trueObj),
            TAG_INT => Ok(unsafe { (birnam_abi::integer::make)(self.u64()? as i64) }),
            TAG_FLOAT => Ok(unsafe { (birnam_abi::float::make)(f64::from_bits(self.u64()?)) }),
            TAG_STRING => {
                let bytes = self.sized()?;
                Ok(unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) })
            }
            TAG_SYMBOL => {
                let bytes = self.sized()?;
                if std::str::from_utf8(bytes).is_err() || bytes.contains(&0) {
                    return Err(CodecError::InvalidUtf8);
                }
                let mut name = Vec::with_capacity(bytes.len() + 1);
                name.extend_from_slice(bytes);
                name.push(0);
                Ok(unsafe { (self.api.intern)(name.as_ptr().cast()) })
            }
            TAG_VECTOR => self.vector(depth),
            TAG_MAP => self.map(depth),
            TAG_RECORD => Err(CodecError::UnknownRecordSchema),
            _ => Err(CodecError::UnknownTag),
        }
    }

    fn vector(&mut self, depth: usize) -> Result<Oop, CodecError> {
        let count = self.u32()? as usize;
        if count > MAX_CONTAINER {
            return Err(CodecError::ContainerTooLarge);
        }
        let vector = unsafe {
            let global = (self.api.import)(c"Vector".as_ptr());
            let selector = (self.api.intern)(c"new".as_ptr());
            (self.api.send)(global, selector, ptr::null(), 0)
        };
        if vector.is_null() {
            return Err(CodecError::NonTransferable);
        }
        let add = unsafe { (self.api.intern)(c"add:".as_ptr()) };
        let mut out = vector;
        for _ in 0..count {
            let item = self.value(depth + 1)?;
            out = unsafe { (self.api.send)(out, add, &item, 1) };
            if out.is_null() {
                return Err(CodecError::NonTransferable);
            }
        }
        Ok(out)
    }

    fn valid_map_key(&self, key: Oop) -> bool {
        if key == self.api.nilObj || key == self.api.falseObj || key == self.api.trueObj {
            return true;
        }
        if unsafe { (birnam_abi::integer::is_integer)(key) } != 0 {
            return true;
        }
        let vtable = unsafe { (self.api.vtableOf)(key) };
        vtable == unsafe { birnam_abi::string::behavior() } || vtable == self.api.symbol_vt
    }

    fn map(&mut self, depth: usize) -> Result<Oop, CodecError> {
        let count = self.u32()? as usize;
        if count > MAX_CONTAINER {
            return Err(CodecError::ContainerTooLarge);
        }
        let map = unsafe {
            let global = (self.api.import)(c"Map".as_ptr());
            let selector = (self.api.intern)(c"new".as_ptr());
            (self.api.send)(global, selector, ptr::null(), 0)
        };
        if map.is_null() {
            return Err(CodecError::NonTransferable);
        }
        let includes = unsafe { (self.api.intern)(c"includesKey:".as_ptr()) };
        let put = unsafe { (self.api.intern)(c"at:put:".as_ptr()) };
        let mut out = map;
        for _ in 0..count {
            let key = self.value(depth + 1)?;
            if !self.valid_map_key(key) {
                return Err(CodecError::InvalidMapKey);
            }
            if unsafe { (self.api.send)(out, includes, &key, 1) } == self.api.trueObj {
                return Err(CodecError::DuplicateMapKey);
            }
            let value = self.value(depth + 1)?;
            let arguments = [key, value];
            out = unsafe { (self.api.send)(out, put, arguments.as_ptr(), 2) };
            if out.is_null() {
                return Err(CodecError::NonTransferable);
            }
        }
        Ok(out)
    }
}

fn decode_oop_value(api: &IdApi, bytes: &[u8]) -> Result<Oop, CodecError> {
    if bytes.len() > MAX_FRAME {
        return Err(CodecError::FrameTooLarge);
    }
    let mut decoder = OopDecoder::new(api, bytes);
    let value = decoder.value(0)?;
    if decoder.position != bytes.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok(value)
}

fn encode_oop_value(api: &IdApi, value: Oop) -> Result<Vec<u8>, CodecError> {
    let mut encoder = OopEncoder::new(api);
    encoder.value(value, 0)?;
    Ok(encoder.bytes)
}

fn decode_oop_error_record(
    api: &IdApi,
    schema: &[u8],
    bytes: &[u8],
) -> Result<(Oop, Oop), CodecError> {
    let mut decoder = OopDecoder::new(api, bytes);
    if decoder.byte()? != TAG_RECORD {
        return Err(CodecError::InvalidRecord);
    }
    let schema_len = decoder.u16()? as usize;
    if decoder.take(schema_len)? != schema {
        return Err(CodecError::UnknownRecordSchema);
    }
    if decoder.u16()? != 2 {
        return Err(CodecError::InvalidRecord);
    }
    let kind = decoder.value(1)?;
    let message = decoder.value(1)?;
    if unsafe { (api.vtableOf)(kind) } != api.symbol_vt
        || unsafe { (api.vtableOf)(message) } != unsafe { birnam_abi::string::behavior() }
    {
        return Err(CodecError::InvalidRecord);
    }
    if decoder.position != bytes.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok((kind, message))
}

fn oop_error_record(
    api: &IdApi,
    schema: &[u8],
    kind: Oop,
    message: Oop,
) -> Result<Vec<u8>, CodecError> {
    if kind.is_null() || message.is_null() {
        return Err(CodecError::InvalidRecord);
    }
    if unsafe { (api.vtableOf)(kind) } != api.symbol_vt
        || (message != api.nilObj && unsafe { (api.vtableOf)(message) } != unsafe { birnam_abi::string::behavior() })
    {
        return Err(CodecError::InvalidRecord);
    }
    let kind_pointer = unsafe { (api.symbolName)(kind) };
    if kind_pointer.is_null() {
        return Err(CodecError::InvalidRecord);
    }
    // SAFETY: `symbolName` returns a live NUL-terminated symbol name for the
    // synchronous call, as required by the frozen IdApi contract.
    let kind_bytes = unsafe { CStr::from_ptr(kind_pointer) }.to_bytes();
    // Language Error permits an absent message; wire-v1 carries a String.
    // Preserve it as an empty message rather than losing the worker process.
    if message == api.nilObj {
        return error_record(schema, kind_bytes, b"");
    }
    let mut message_len = 0usize;
    let message_pointer = unsafe { (birnam_abi::string::bytes)(message, &mut message_len) };
    if message_len != 0 && message_pointer.is_null() {
        return Err(CodecError::InvalidRecord);
    }
    let message_bytes = if message_len == 0 {
        &[]
    } else {
        // SAFETY: `stringBytes` returns a readable extent for the live String
        // object throughout this synchronous encoding call.
        unsafe { std::slice::from_raw_parts(message_pointer, message_len) }
    };
    error_record(schema, kind_bytes, message_bytes)
}

fn write_frame<W: Write>(writer: &mut W, frame: &Frame) -> Result<(), CodecError> {
    let bytes = frame.encode()?;
    writer
        .write_all(&bytes)
        .map_err(|_| CodecError::InvalidRecord)
}

fn read_frame<R: Read>(reader: &mut R) -> Result<Frame, CodecError> {
    let mut header = [0u8; HEADER_LEN];
    reader
        .read_exact(&mut header)
        .map_err(|_| CodecError::Truncated)?;
    let decoded = decode_header(&header)?;
    let mut bytes = header.to_vec();
    let payload_len = decoded.payload_len as usize;
    let start = bytes.len();
    bytes.resize(start + payload_len, 0);
    reader
        .read_exact(&mut bytes[start..])
        .map_err(|_| CodecError::Truncated)?;
    Frame::decode(&bytes)
}

fn borrowed_file(file: &mut ManuallyDrop<File>) -> &mut File {
    // SAFETY: `ManuallyDrop<T>` has the same representation as `T`; the
    // wrapper only suppresses `File`'s destructor and does not move it.
    unsafe { &mut *(file as *mut ManuallyDrop<File> as *mut File) }
}

#[repr(C)]
struct Failure {
    class: Oop,
    kind: Oop,
    message: Oop,
    exit_status: Oop,
    request_id: Oop,
}

struct Runtime {
    api: usize,
    isolate_vt: usize,
    isolate_obj: usize,
    handle_vt: usize,
    failure_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();

unsafe extern "C" fn mark_runtime_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.isolate_vt,
            runtime.isolate_obj,
            runtime.handle_vt,
            runtime.failure_vt,
        ] {
            unsafe { mark(value as Oop) };
        }
    }
}

#[repr(C)]
struct Handle {
    token: u64,
}

#[cfg(unix)]
const WORKER_FD: i32 = 198;

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Starting,
    Ready,
    Busy,
    Stopping,
    ReapPending,
    Exited,
}

#[cfg(unix)]
struct Entry {
    child: Child,
    channel: Option<UnixStream>,
    state: Lifecycle,
    next_request: u64,
    exit_status: Option<u32>,
    orphaned: bool,
    pending: Option<asynchronous::Pending>,
    task_service: bool,
}

#[cfg(unix)]
static REGISTRY: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Entry>>>>> = OnceLock::new();
#[cfg(unix)]
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[cfg(unix)]
fn registry() -> &'static Mutex<HashMap<u64, Arc<Mutex<Entry>>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(unix)]
fn registry_lock() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Entry>>>> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn lookup_entry(token: u64) -> Option<Arc<Mutex<Entry>>> {
    registry_lock().get(&token).cloned()
}

#[cfg(unix)]
fn lock_entry(entry: &Mutex<Entry>) -> std::sync::MutexGuard<'_, Entry> {
    entry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn status_value(status: std::process::ExitStatus) -> u32 {
    if let Some(code) = status.code() {
        return code.max(0) as u32;
    }
    status
        .signal()
        .map(|signal| 128u32.saturating_add(signal as u32))
        .unwrap_or(1)
}

#[cfg(unix)]
fn status_of(entry: &mut Entry) -> Option<u32> {
    if entry.state == Lifecycle::Exited {
        return entry.exit_status;
    }
    match entry.child.try_wait() {
        Ok(Some(status)) => {
            let value = status_value(status);
            entry.exit_status = Some(value);
            entry.state = Lifecycle::Exited;
            if entry.pending.is_none() {
                entry.channel.take();
            }
            Some(value)
        }
        Ok(None) | Err(_) => None,
    }
}

#[cfg(unix)]
fn terminate_entry(entry: &mut Entry) {
    if status_of(entry).is_some() {
        entry.channel.take();
        return;
    }
    entry.channel.take();
    let _ = entry.child.kill();
    match entry.child.wait() {
        Ok(status) => {
            entry.exit_status = Some(status_value(status));
            entry.state = Lifecycle::Exited;
        }
        Err(_) => entry.state = Lifecycle::ReapPending,
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostTransportError {
    Timeout,
    Io,
    Protocol,
}

#[cfg(unix)]
fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[cfg(unix)]
fn host_timeout(stream: &UnixStream, timeout_ms: u32) -> Result<(), HostTransportError> {
    let timeout = Duration::from_millis(timeout_ms as u64);
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| HostTransportError::Io)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| HostTransportError::Io)
}

#[cfg(unix)]
fn host_read_frame(stream: &mut UnixStream, timeout_ms: u32) -> Result<Frame, HostTransportError> {
    host_timeout(stream, timeout_ms)?;
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).map_err(|error| {
        if is_timeout(&error) {
            HostTransportError::Timeout
        } else {
            HostTransportError::Io
        }
    })?;
    let decoded = decode_header(&header).map_err(|_| HostTransportError::Protocol)?;
    let mut bytes = header.to_vec();
    let payload_len = decoded.payload_len as usize;
    let start = bytes.len();
    bytes.resize(start + payload_len, 0);
    stream.read_exact(&mut bytes[start..]).map_err(|error| {
        if is_timeout(&error) {
            HostTransportError::Timeout
        } else {
            HostTransportError::Io
        }
    })?;
    Frame::decode(&bytes).map_err(|_| HostTransportError::Protocol)
}

#[cfg(unix)]
fn host_write_frame(
    stream: &mut UnixStream,
    frame: &Frame,
    timeout_ms: u32,
) -> Result<(), HostTransportError> {
    host_timeout(stream, timeout_ms)?;
    let bytes = frame.encode().map_err(|_| HostTransportError::Protocol)?;
    stream.write_all(&bytes).map_err(|error| {
        if is_timeout(&error) {
            HostTransportError::Timeout
        } else {
            HostTransportError::Io
        }
    })
}

#[cfg(unix)]
fn spawn_host_worker(
    image: &[u8],
    receiver: &[u8],
    selector: &[u8],
    task_service: bool,
) -> Result<Entry, &'static str> {
    let image = std::str::from_utf8(image).map_err(|_| "invalidImageEncoding")?;
    let receiver = std::str::from_utf8(receiver).map_err(|_| "invalidReceiverEncoding")?;
    let selector = std::str::from_utf8(selector).map_err(|_| "invalidSelectorEncoding")?;
    let image = std::fs::canonicalize(image).map_err(|_| "imageUnavailable")?;
    let executable = std::env::var_os("BIRNAM_WORKER_EXECUTABLE")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_exe().map_err(|_| "executableUnavailable")?);
    let (host, worker) = UnixStream::pair().map_err(|_| "socketPairFailed")?;
    let host_fd = host.into_raw_fd();
    let worker_fd = worker.into_raw_fd();
    let mut command = Command::new(executable);
    command.env("BIRNAM_TASK_SERVICE", if task_service { "1" } else { "0" });
    command.args([
        "--worker",
        image.to_str().ok_or("invalidImageEncoding")?,
        receiver,
        selector,
        "--channel-fd",
        &WORKER_FD.to_string(),
    ]);
    unsafe {
        command.pre_exec(move || {
            unsafe extern "C" {
                fn close(fd: c_int) -> c_int;
                fn dup2(oldfd: c_int, newfd: c_int) -> c_int;
            }
            if dup2(worker_fd, WORKER_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            if host_fd != WORKER_FD {
                close(host_fd);
            }
            if worker_fd != WORKER_FD {
                close(worker_fd);
            }
            Ok(())
        });
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            unsafe {
                let _ = close_fd(host_fd);
                let _ = close_fd(worker_fd);
            }
            return Err("spawnFailed");
        }
    };
    unsafe { close_fd(worker_fd) };
    let channel = unsafe { UnixStream::from_raw_fd(host_fd) };
    Ok(Entry {
        child,
        channel: Some(channel),
        state: Lifecycle::Starting,
        next_request: 1,
        exit_status: None,
        orphaned: false,
        pending: None,
        task_service,
    })
}

#[cfg(unix)]
unsafe fn close_fd(fd: c_int) -> c_int {
    unsafe extern "C" {
        fn close(fd: c_int) -> c_int;
    }
    unsafe { close(fd) }
}

fn runtime_api() -> &'static IdApi {
    let runtime = RUNTIME.get().expect("Rust Isolate module_init missing");
    // SAFETY: module_init records the process-owned IdApi pointer for the
    // process lifetime; all dynamic methods are synchronous borrows.
    unsafe { &*(runtime.api as *const IdApi) }
}

fn failure_object(value: Oop) -> &'static mut Failure {
    // SAFETY: callers pass only instances allocated from the registered
    // IsolateFailureVt family with the exact five-OOP payload layout.
    unsafe { &mut *(value as *mut Failure) }
}

fn make_failure_objects(
    api: &IdApi,
    class: Oop,
    kind: Oop,
    message: Oop,
    exit_status: Oop,
    request_id: Oop,
) -> Oop {
    let runtime = RUNTIME.get().expect("Rust Isolate module_init missing");
    let value = unsafe { (api.alloc)(runtime.failure_vt as Oop, std::mem::size_of::<Failure>()) };
    if value.is_null() {
        return api.nilObj;
    }
    failure_object(value).class = class;
    failure_object(value).kind = kind;
    failure_object(value).message = message;
    failure_object(value).exit_status = exit_status;
    failure_object(value).request_id = request_id;
    value
}

fn make_failure(api: &IdApi, class: &[u8], kind: &[u8], message: &[u8]) -> Oop {
    let class_z = class
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let kind_z = kind
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let class_o = unsafe { (api.intern)(class_z.as_ptr().cast()) };
    let kind_o = unsafe { (api.intern)(kind_z.as_ptr().cast()) };
    let message_o = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    make_failure_objects(api, class_o, kind_o, message_o, api.nilObj, api.nilObj)
}

fn make_failure_from_record(
    api: &IdApi,
    class: &[u8],
    schema: &[u8],
    payload: &[u8],
    fallback_kind: &[u8],
    fallback_message: &[u8],
) -> Oop {
    let class_z = class
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let class_o = unsafe { (api.intern)(class_z.as_ptr().cast()) };
    match decode_oop_error_record(api, schema, payload) {
        Ok((kind, message)) => {
            make_failure_objects(api, class_o, kind, message, api.nilObj, api.nilObj)
        }
        Err(_) => make_failure(api, class, fallback_kind, fallback_message),
    }
}

fn oop_text(api: &IdApi, value: Oop) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let vtable = unsafe { (api.vtableOf)(value) };
    if vtable == api.symbol_vt {
        let pointer = unsafe { (api.symbolName)(value) };
        if pointer.is_null() {
            return None;
        }
        // SAFETY: `symbolName` returns a live NUL-terminated symbol name.
        return Some(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec());
    }
    if vtable == unsafe { birnam_abi::string::behavior() } {
        let mut length = 0usize;
        let pointer = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
        if length != 0 && pointer.is_null() {
            return None;
        }
        // SAFETY: `stringBytes` returns a readable extent for this call.
        return Some(if length == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec()
        });
    }
    None
}

fn selector_arity_one(selector: &[u8]) -> bool {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons == 1;
    }
    selector
        .first()
        .is_some_and(|byte| !byte.is_ascii_alphabetic())
}

#[cfg(unix)]
unsafe extern "C" fn finalise_handle(value: Oop) {
    if value.is_null() {
        return;
    }
    // SAFETY: the finalizer is installed only on `Handle` payloads.
    let token = unsafe { (&*(value as *const Handle)).token };
    if let Some(entry) = lookup_entry(token) {
        lock_entry(&entry).orphaned = true;
    }
}

#[cfg(unix)]
fn handle_object(value: Oop) -> &'static mut Handle {
    // SAFETY: callers pass only instances allocated from `handle_vt`.
    unsafe { &mut *(value as *mut Handle) }
}

#[cfg(unix)]
fn new_handle(api: &IdApi, mut entry: Entry) -> Oop {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    if token == 0 {
        terminate_entry(&mut entry);
        return make_failure(
            api,
            b"exit",
            b"tokenExhausted",
            b"isolate handle token space exhausted",
        );
    }
    registry_lock().insert(token, Arc::new(Mutex::new(entry)));
    let runtime = RUNTIME.get().expect("Rust Isolate module_init missing");
    let value = unsafe { (api.alloc)(runtime.handle_vt as Oop, std::mem::size_of::<Handle>()) };
    if value.is_null() {
        let entry = registry_lock().remove(&token);
        if let Some(entry) = entry {
            terminate_entry(&mut lock_entry(&entry));
        }
        return api.nilObj;
    }
    handle_object(value).token = token;
    unsafe { (api.setFinalizer)(value, finalise_handle) };
    value
}

#[cfg(unix)]
unsafe extern "C" fn isolate_start(_: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { start_worker(args, count, false) }
}

#[cfg(unix)]
unsafe fn start_worker(args: *const Oop, count: usize, task_service: bool) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"start expects three arguments",
        );
    };
    if args.len() != 3 {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"start expects three arguments",
        );
    }
    let Some(receiver) = oop_text(api, args[0]) else {
        return make_failure(
            api,
            b"encode",
            b"invalidReceiver",
            b"receiver must be a Symbol or String",
        );
    };
    let Some(selector) = oop_text(api, args[1]) else {
        return make_failure(
            api,
            b"encode",
            b"invalidSelector",
            b"selector must be a Symbol or String",
        );
    };
    let Some(image) = oop_text(api, args[2]) else {
        return make_failure(api, b"encode", b"invalidImage", b"image must be a String");
    };
    if receiver.contains(&0) || selector.contains(&0) || image.contains(&0) {
        return make_failure(
            api,
            b"encode",
            b"embeddedNul",
            b"worker names and image path may not contain NUL",
        );
    }
    if !selector_arity_one(&selector) {
        return make_failure(
            api,
            b"boot",
            b"wrongArity",
            b"worker selector must have arity one",
        );
    }
    let mut entry = match spawn_host_worker(&image, &receiver, &selector, task_service) {
        Ok(entry) => entry,
        Err(kind) => return make_failure(api, b"boot", kind.as_bytes(), kind.as_bytes()),
    };
    let ready = match entry
        .channel
        .as_mut()
        .ok_or(HostTransportError::Io)
        .and_then(|channel| host_read_frame(channel, 10_000))
    {
        Ok(frame) => frame,
        Err(error) => {
            terminate_entry(&mut entry);
            let kind = match error {
                HostTransportError::Timeout => b"handshakeTimeout".as_slice(),
                HostTransportError::Protocol => b"invalidReady".as_slice(),
                HostTransportError::Io => b"handshakeFailed".as_slice(),
            };
            return make_failure(
                api,
                b"boot",
                kind,
                b"worker did not complete the ready handshake",
            );
        }
    };
    if ready.header.kind == FrameKind::BootError {
        terminate_entry(&mut entry);
        return make_failure_from_record(
            api,
            b"boot",
            BOOT_ERROR_SCHEMA,
            &ready.payload,
            b"invalidBootError",
            b"worker sent an invalid boot error record",
        );
    }
    if ready.header.kind != FrameKind::Ready {
        terminate_entry(&mut entry);
        return make_failure(api, b"boot", b"expectedReady", b"worker did not send ready");
    }
    entry.state = Lifecycle::Ready;
    new_handle(api, entry)
}

unsafe extern "C" fn isolate_sleep(_: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if args.is_empty() {
        return api.nilObj;
    }
    let value = args[0];
    if unsafe { (birnam_abi::integer::is_integer)(value) } != 0 {
        let milliseconds = unsafe { (birnam_abi::integer::value)(value) };
        if milliseconds > 0 {
            std::thread::sleep(Duration::from_millis(milliseconds as u64));
        }
    }
    value
}

unsafe extern "C" fn isolate_crash(_: Oop, _: *const Oop, _: usize) -> Oop {
    std::process::abort()
}

#[cfg(unix)]
unsafe extern "C" fn handle_request(self_oop: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"request expects value and timeout",
        );
    };
    if args.len() != 2 {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"request expects value and timeout",
        );
    }
    if unsafe { (birnam_abi::integer::is_integer)(args[1]) } == 0 || unsafe { (birnam_abi::integer::value)(args[1]) } < 0 {
        return make_failure(
            api,
            b"encode",
            b"invalidTimeout",
            b"timeout must be a nonnegative Integer",
        );
    }
    let timeout = (unsafe { (birnam_abi::integer::value)(args[1]) } as u64).min(u32::MAX as u64) as u32;
    let argument = match encode_oop_value(api, args[0]) {
        Ok(argument) => argument,
        Err(error) => {
            return make_failure(api, b"encode", b"nonTransferable", codec_error_name(error));
        }
    };
    let token = handle_object(self_oop).token;
    let Some(shared) = lookup_entry(token) else {
        return make_failure(api, b"stopped", b"unknownHandle", b"unknown isolate handle");
    };
    let mut guard = lock_entry(&shared);
    let entry = &mut *guard;
    if entry.task_service {
        return make_failure(
            api,
            b"encode",
            b"asyncRequired",
            b"task services use submit/poll or Worker request:",
        );
    }
    if entry.state != Lifecycle::Ready {
        return make_failure(api, b"stopped", b"notReady", b"isolate is not ready");
    }
    let request_id = entry.next_request;
    if request_id == 0 || request_id > i64::MAX as u64 {
        entry.state = Lifecycle::Stopping;
        terminate_entry(entry);
        return make_failure(
            api,
            b"protocol",
            b"requestIdExhausted",
            b"request id space exhausted",
        );
    }
    entry.next_request += 1;
    entry.state = Lifecycle::Busy;
    let request = Frame {
        header: Header {
            kind: FrameKind::Request,
            request_id,
            payload_len: 0,
        },
        payload: argument,
    };
    let result = (|| {
        let channel = entry.channel.as_mut().ok_or(HostTransportError::Io)?;
        host_write_frame(channel, &request, timeout)?;
        host_read_frame(channel, timeout)
    })();
    let reply = match result {
        Ok(reply) => reply,
        Err(error) => {
            entry.state = Lifecycle::Stopping;
            terminate_entry(entry);
            let (class, kind, message): (&[u8], &[u8], &[u8]) = match error {
                HostTransportError::Timeout => (
                    b"timeout",
                    b"deadline",
                    b"request deadline expired before a reply arrived",
                ),
                HostTransportError::Protocol => (
                    b"protocol",
                    b"invalidReply",
                    b"worker returned an invalid reply frame",
                ),
                HostTransportError::Io => (
                    if entry.exit_status.is_some() {
                        b"exit"
                    } else {
                        b"transport"
                    },
                    b"transport",
                    b"request transport failed",
                ),
            };
            let class_z = class
                .iter()
                .copied()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let kind_z = kind
                .iter()
                .copied()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let class_o = unsafe { (api.intern)(class_z.as_ptr().cast()) };
            let kind_o = unsafe { (api.intern)(kind_z.as_ptr().cast()) };
            let message_o = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
            let status_o = entry
                .exit_status
                .map(|status| unsafe { (birnam_abi::integer::make)(status as i64) })
                .unwrap_or(api.nilObj);
            let request_o = unsafe { (birnam_abi::integer::make)(request_id as i64) };
            return make_failure_objects(api, class_o, kind_o, message_o, status_o, request_o);
        }
    };
    if reply.header.request_id != request_id {
        entry.state = Lifecycle::Stopping;
        terminate_entry(entry);
        return make_failure(
            api,
            b"protocol",
            b"mismatchedRequestId",
            b"reply request id did not match",
        );
    }
    match reply.header.kind {
        FrameKind::ReplyOk => match decode_oop_value(api, &reply.payload) {
            Ok(value) => {
                entry.state = Lifecycle::Ready;
                value
            }
            Err(error) => {
                entry.state = Lifecycle::Stopping;
                terminate_entry(entry);
                make_failure(api, b"protocol", b"invalidReply", codec_error_name(error))
            }
        },
        FrameKind::ReplyError => {
            match decode_oop_error_record(api, APPLICATION_ERROR_SCHEMA, &reply.payload) {
                Ok((kind, message)) => {
                    entry.state = Lifecycle::Ready;
                    let request = unsafe { (birnam_abi::integer::make)(request_id as i64) };
                    let class = unsafe { (api.intern)(c"application".as_ptr()) };
                    make_failure_objects(api, class, kind, message, api.nilObj, request)
                }
                Err(error) => {
                    entry.state = Lifecycle::Stopping;
                    terminate_entry(entry);
                    make_failure(
                        api,
                        b"protocol",
                        b"invalidApplicationError",
                        codec_error_name(error),
                    )
                }
            }
        }
        _ => {
            entry.state = Lifecycle::Stopping;
            terminate_entry(entry);
            make_failure(
                api,
                b"protocol",
                b"unexpectedReply",
                b"unexpected reply frame",
            )
        }
    }
}

#[cfg(unix)]
unsafe extern "C" fn handle_request_default(self_oop: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"request expects one argument",
        );
    };
    if args.len() != 1 {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"request expects one argument",
        );
    }
    let timeout = unsafe { (birnam_abi::integer::make)(5_000) };
    let packed = [args[0], timeout];
    unsafe { handle_request(self_oop, packed.as_ptr(), 2) }
}

#[cfg(unix)]
unsafe extern "C" fn handle_stop(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    let api = runtime_api();
    let token = handle_object(self_oop).token;
    let Some(shared) = lookup_entry(token) else {
        return self_oop;
    };
    let mut guard = lock_entry(&shared);
    let entry = &mut *guard;
    if entry.state == Lifecycle::Exited {
        return self_oop;
    }
    if entry.state != Lifecycle::Ready {
        return make_failure(api, b"busy", b"notReady", b"isolate is not ready");
    }
    let request_id = entry.next_request;
    entry.next_request += 1;
    entry.state = Lifecycle::Stopping;
    let stop = Frame {
        header: Header {
            kind: FrameKind::Stop,
            request_id,
            payload_len: 0,
        },
        payload: Vec::new(),
    };
    if let Some(channel) = entry.channel.as_mut() {
        let _ = host_write_frame(channel, &stop, 2_000);
        if let Ok(reply) = host_read_frame(channel, 2_000)
            && (reply.header.kind != FrameKind::Stopped || reply.header.request_id != request_id)
        {
            terminate_entry(entry);
            return make_failure(
                api,
                b"protocol",
                b"invalidStopReply",
                b"invalid stopped reply",
            );
        }
    }
    terminate_entry(entry);
    self_oop
}

#[cfg(unix)]
unsafe extern "C" fn handle_state(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    let api = runtime_api();
    let token = handle_object(self_oop).token;
    let state = lookup_entry(token)
        .map(|shared| {
            let mut guard = lock_entry(&shared);
            let entry = &mut *guard;
            let _ = status_of(entry);
            match entry.state {
                Lifecycle::Starting => "starting",
                Lifecycle::Ready => "ready",
                Lifecycle::Busy => "busy",
                Lifecycle::Stopping => "stopping",
                Lifecycle::ReapPending => "reapPending",
                Lifecycle::Exited => "exited",
            }
        })
        .unwrap_or("stopped");
    let name = CString::new(state).expect("state symbol");
    unsafe { (api.intern)(name.as_ptr()) }
}

unsafe extern "C" fn failure_is_error(_: Oop, _: *const Oop, _: usize) -> Oop {
    runtime_api().trueObj
}

unsafe extern "C" fn failure_class(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    failure_object(self_oop).class
}

unsafe extern "C" fn failure_kind(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    failure_object(self_oop).kind
}

unsafe extern "C" fn failure_message(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    failure_object(self_oop).message
}

unsafe extern "C" fn failure_status(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    failure_object(self_oop).exit_status
}

unsafe extern "C" fn failure_request_id(self_oop: Oop, _: *const Oop, _: usize) -> Oop {
    failure_object(self_oop).request_id
}

/// Register the Rust Isolate family shape without claiming host lifecycle
/// parity. The worker ABI can be exercised independently; this bridge keeps
/// family names/layouts and structured pending failures explicit until process
/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the process-global isolate service until owner-thread and
/// registry/timeout ownership is ported.
///
/// # Safety
///
/// `api` must point to a live process-owned `IdApi` table for the duration of
/// initialization. The returned module state is null and no caller memory is
/// retained beyond the process-owned API table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1) -> *mut c_void {
    if api.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the caller owns a live process IdApi table for module_init.
    let api_ref = unsafe { &*api };
    let runtime = RUNTIME.get_or_init(|| {
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, c"Isolate".as_ptr()) } != 0;
        let (isolate_vt, isolate_obj, handle_vt, failure_vt) = if reloading {
            (
                unsafe { (api_ref.import)(c"IsolateVt".as_ptr()) },
                unsafe { (api_ref.import)(c"Isolate".as_ptr()) },
                unsafe { (api_ref.import)(c"IsolateHandleVt".as_ptr()) },
                unsafe { (api_ref.import)(c"IsolateFailureVt".as_ptr()) },
            )
        } else {
            let isolate_vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
            let isolate_obj = unsafe { (api_ref.alloc)(isolate_vt, 0) };
            let handle_vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
            let error_vt = unsafe { (api_ref.import)(c"ErrorVt".as_ptr()) };
            let failure_vt = if error_vt.is_null() {
                unsafe { (api_ref.proto)(api_ref.object_vt) }
            } else {
                unsafe { (api_ref.proto)(error_vt) }
            };
            (isolate_vt, isolate_obj, handle_vt, failure_vt)
        };
        Runtime {
            api: api as usize,
            isolate_vt: isolate_vt as usize,
            isolate_obj: isolate_obj as usize,
            handle_vt: handle_vt as usize,
            failure_vt: failure_vt as usize,
        }
    });
    let isolate_vt = runtime.isolate_vt as Oop;
    let handle_vt = runtime.handle_vt as Oop;
    let failure_vt = runtime.failure_vt as Oop;
    ROOT_REGISTERED.get_or_init(|| unsafe {
        (api_ref.gcRegisterRootProvider)(mark_runtime_roots);
    });
    unsafe {
        (api_ref.setLayout)(isolate_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api_ref.setLayout)(handle_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api_ref.setLayout)(failure_vt, ptr::null(), 0, birnam_abi::SLOT_OOP);
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"start:selector:image:".as_ptr()),
            isolate_start,
        );
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"idleFor:".as_ptr()),
            isolate_sleep,
        );
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"_isolateSleep:".as_ptr()),
            isolate_sleep,
        );
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"_isolateCrash:".as_ptr()),
            isolate_crash,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"request:timeout:".as_ptr()),
            handle_request,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"request:".as_ptr()),
            handle_request_default,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"submit:timeout:".as_ptr()),
            asynchronous::submit,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"pollRequest:".as_ptr()),
            asynchronous::poll,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"terminate".as_ptr()),
            asynchronous::terminate,
        );
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"startService:image:".as_ptr()),
            service::start,
        );
        (api_ref.method)(
            isolate_vt,
            (api_ref.intern)(c"pollServiceControl".as_ptr()),
            service::poll_control,
        );
        (api_ref.method)(
            handle_vt,
            (api_ref.intern)(c"cancelRequest:".as_ptr()),
            asynchronous::cancel,
        );
        (api_ref.method)(handle_vt, (api_ref.intern)(c"stop".as_ptr()), handle_stop);
        (api_ref.method)(handle_vt, (api_ref.intern)(c"state".as_ptr()), handle_state);
        (api_ref.method)(
            failure_vt,
            (api_ref.intern)(c"isError".as_ptr()),
            failure_is_error,
        );
        (api_ref.method)(
            failure_vt,
            (api_ref.intern)(c"class".as_ptr()),
            failure_class,
        );
        (api_ref.method)(failure_vt, (api_ref.intern)(c"kind".as_ptr()), failure_kind);
        (api_ref.method)(
            failure_vt,
            (api_ref.intern)(c"message".as_ptr()),
            failure_message,
        );
        (api_ref.method)(
            failure_vt,
            (api_ref.intern)(c"exitStatus".as_ptr()),
            failure_status,
        );
        (api_ref.method)(
            failure_vt,
            (api_ref.intern)(c"requestId".as_ptr()),
            failure_request_id,
        );
        if birnam_abi::namespace_presence::contains(presence, c"Isolate".as_ptr()) == 0 {
            let _ = (api_ref.exportObj)(c"IsolateVt".as_ptr(), isolate_vt);
            let _ = (api_ref.exportObj)(c"Isolate".as_ptr(), runtime.isolate_obj as Oop);
            let _ = (api_ref.exportObj)(c"IsolateHandleVt".as_ptr(), handle_vt);
            let _ = (api_ref.exportObj)(c"IsolateFailureVt".as_ptr(), failure_vt);
        }
    }
    ptr::null_mut()
}

/// Run the Rust worker loop over an inherited channel.
///
/// Requests contain one copied BISO value. The worker decodes it through the
/// process-local `IdApi`, sends it to the borrowed receiver/selector, then
/// encodes either the result or the exact application/protocol error record.
/// The receiver and selector never cross the channel and are not retained.
///
/// # Safety
///
/// `api`, `receiver`, and `selector` follow the frozen host ABI and are only
/// borrowed for this call. `fd` must be a live inherited channel descriptor;
/// ownership remains with the caller and is not closed here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_worker_driver(
    api: *mut IdApi,
    fd: c_int,
    receiver: Oop,
    selector: Oop,
) -> c_int {
    // SAFETY: the caller supplies a live descriptor and retains ownership;
    // ManuallyDrop prevents File from closing it on return.
    let mut channel = ManuallyDrop::new(unsafe { File::from_raw_fd(fd as RawFd) });
    let task_service = std::env::var("BIRNAM_TASK_SERVICE").is_ok_and(|s| s == "1");
    let _service_guard = service::DriverGuard;
    let ready = Frame {
        header: Header {
            kind: FrameKind::Ready,
            request_id: 0,
            payload_len: 0,
        },
        payload: Vec::new(),
    };
    if write_frame(borrowed_file(&mut channel), &ready).is_err() {
        return 1;
    }
    loop {
        let frame = match if task_service {
            service::read_next(borrowed_file(&mut channel)).map_err(|_| ())
        } else {
            read_frame(borrowed_file(&mut channel)).map_err(|_| ())
        } {
            Ok(frame) => frame,
            Err(_) => return 1,
        };
        match frame.header.kind {
            FrameKind::Stop => {
                let stopped = Frame {
                    header: Header {
                        kind: FrameKind::Stopped,
                        request_id: frame.header.request_id,
                        payload_len: 0,
                    },
                    payload: Vec::new(),
                };
                return if write_frame(borrowed_file(&mut channel), &stopped).is_ok() {
                    0
                } else {
                    1
                };
            }
            FrameKind::Request => {
                let Some(api) = (!api.is_null()).then(|| unsafe { &*api }) else {
                    return 1;
                };
                let payload = if task_service {
                    match service::request_payload(&frame.payload) {
                        Ok(Some(value)) => value,
                        Ok(None) => continue, // A cancellation arriving after publication is stale.
                        Err(_) => return 1,
                    }
                } else {
                    frame.payload.clone()
                };
                let argument = match decode_oop_value(api, &payload) {
                    Ok(value) => value,
                    Err(error) => {
                        let payload = match error_record(
                            PROTOCOL_ERROR_SCHEMA,
                            b"invalidPayload",
                            codec_error_name(error),
                        ) {
                            Ok(payload) => payload,
                            Err(_) => return 1,
                        };
                        let protocol_error = Frame {
                            header: Header {
                                kind: FrameKind::ProtocolError,
                                request_id: frame.header.request_id,
                                payload_len: 0,
                            },
                            payload,
                        };
                        let _ = write_frame(borrowed_file(&mut channel), &protocol_error);
                        return 1;
                    }
                };
                let result = if task_service {
                    if service::begin(fd, frame.header.request_id).is_err() {
                        return 1;
                    }
                    let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
                    let name = unsafe { (api.intern)(c"TaskService".as_ptr()) };
                    let policy =
                        unsafe { (api.send)(globals, (api.intern)(c"at:".as_ptr()), &name, 1) };
                    let result = unsafe {
                        (api.send)(
                            policy,
                            (api.intern)(c"serve:request:".as_ptr()),
                            [receiver, argument].as_ptr(),
                            2,
                        )
                    };
                    result
                } else {
                    unsafe { (api.send)(receiver, selector, &argument, 1) }
                };
                if !result.is_null()
                    && unsafe {
                        (api.send)(result, (api.intern)(c"isError".as_ptr()), ptr::null(), 0)
                    } == api.trueObj
                {
                    let kind = unsafe {
                        (api.send)(result, (api.intern)(c"kind".as_ptr()), ptr::null(), 0)
                    };
                    let message = unsafe {
                        (api.send)(result, (api.intern)(c"message".as_ptr()), ptr::null(), 0)
                    };
                    let payload =
                        match oop_error_record(api, APPLICATION_ERROR_SCHEMA, kind, message) {
                            Ok(payload) => payload,
                            Err(_) => return 1,
                        };
                    let reply = Frame {
                        header: Header {
                            kind: FrameKind::ReplyError,
                            request_id: frame.header.request_id,
                            payload_len: 0,
                        },
                        payload,
                    };
                    if write_frame(borrowed_file(&mut channel), &reply).is_err() {
                        return 1;
                    }
                } else {
                    let mut encoder = OopEncoder::new(api);
                    let (kind, payload) = match encoder.value(result, 0) {
                        Ok(()) => (FrameKind::ReplyOk, encoder.bytes),
                        Err(error) => {
                            let payload = match error_record(
                                APPLICATION_ERROR_SCHEMA,
                                b"encodeFailed",
                                codec_error_name(error),
                            ) {
                                Ok(payload) => payload,
                                Err(_) => return 1,
                            };
                            (FrameKind::ReplyError, payload)
                        }
                    };
                    let reply = Frame {
                        header: Header {
                            kind,
                            request_id: frame.header.request_id,
                            payload_len: 0,
                        },
                        payload,
                    };
                    if write_frame(borrowed_file(&mut channel), &reply).is_err() {
                        return 1;
                    }
                }
            }
            _ => return 1,
        }
    }
}

/// Write a frozen `birnam.boot-error.v1` frame to an inherited channel.
///
/// # Safety
///
/// `api` is borrowed for ABI compatibility but is not retained. `kind_z` must
/// be a valid NUL-terminated string, and `message_ptr` must reference
/// `message_len` readable bytes. `fd` remains owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_worker_boot_error(
    _api: *mut IdApi,
    fd: c_int,
    kind_z: *const c_char,
    message_ptr: *const u8,
    message_len: usize,
) {
    if kind_z.is_null() || (message_ptr.is_null() && message_len != 0) {
        return;
    }
    // SAFETY: the caller promises a valid NUL-terminated kind string.
    let kind = unsafe { CStr::from_ptr(kind_z) }.to_bytes();
    // SAFETY: the null/count check above establishes the readable message
    // range; an empty message is represented without dereferencing null.
    let message = if message_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(message_ptr, message_len) }
    };
    let Ok(payload) = error_record(BOOT_ERROR_SCHEMA, kind, message) else {
        return;
    };
    let frame = Frame {
        header: Header {
            kind: FrameKind::BootError,
            request_id: 0,
            payload_len: 0,
        },
        payload,
    };
    // SAFETY: the caller supplies a live descriptor and retains ownership;
    // ManuallyDrop prevents File from closing it on return.
    let mut channel = ManuallyDrop::new(unsafe { File::from_raw_fd(fd as RawFd) });
    let _ = write_frame(borrowed_file(&mut channel), &frame);
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_drain() {
    #[cfg(unix)]
    {
        let entries: Vec<_> = registry_lock().values().cloned().collect();
        for shared in entries {
            let mut guard = lock_entry(&shared);
            let entry = &mut *guard;
            if entry.orphaned && entry.state != Lifecycle::Exited {
                terminate_entry(entry);
            } else {
                let _ = status_of(entry);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_shutdown() {
    #[cfg(unix)]
    {
        let entries: Vec<_> = registry_lock().values().cloned().collect();
        for shared in entries {
            let mut guard = lock_entry(&shared);
            let entry = &mut *guard;
            if entry.state != Lifecycle::Exited {
                terminate_entry(entry);
            }
        }
    }
}

/// Stable identity for the first Rust Isolate dynamic artifact.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_rust_identity() -> *const c_char {
    c"rust-isolate-r3-codec".as_ptr()
}

/// Encode a BISO header into caller-owned storage. This probe is deliberately
/// smaller than the eventual worker ABI: it lets the reference corpus compare the
/// exact wire bytes before process orchestration is ported.
///
/// # Safety
///
/// `out` must be non-null and point to at least `HEADER_LEN` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_rust_encode_header(
    kind: u8,
    request_id: u64,
    payload_len: u32,
    out: *mut u8,
) -> i32 {
    if out.is_null() {
        return -1;
    }
    let Some(kind) = FrameKind::parse(kind).ok() else {
        return error_code(CodecError::UnknownKind);
    };
    let header = match encode_header(Header {
        kind,
        request_id,
        payload_len,
    }) {
        Ok(header) => header,
        Err(error) => return error_code(error),
    };
    // SAFETY: the caller promises a writable HEADER_LEN-byte destination.
    unsafe { ptr::copy_nonoverlapping(header.as_ptr(), out, HEADER_LEN) };
    0
}

/// Decode a BISO header from caller-owned bytes and return its fields through
/// caller-owned output pointers. This keeps frame validation observable at the
/// dynamic boundary before worker orchestration is introduced.
///
/// # Safety
///
/// `bytes` must be non-null and point to `len` readable bytes. Each output
/// pointer must be non-null and point to writable storage of the corresponding
/// type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_rust_decode_header(
    bytes: *const u8,
    len: usize,
    kind_out: *mut u8,
    request_id_out: *mut u64,
    payload_len_out: *mut u32,
) -> i32 {
    if bytes.is_null()
        || kind_out.is_null()
        || request_id_out.is_null()
        || payload_len_out.is_null()
    {
        return -1;
    }
    // SAFETY: the caller promises that `bytes` points to `len` readable bytes;
    // the null check above establishes a non-null base pointer.
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
    let header = match decode_header(bytes) {
        Ok(header) => header,
        Err(error) => return error_code(error),
    };
    // SAFETY: the caller promises writable output storage for each field;
    // null checks above establish all three destinations.
    unsafe {
        *kind_out = header.kind as u8;
        *request_id_out = header.request_id;
        *payload_len_out = header.payload_len;
    }
    0
}

/// Encode one floor OOP through the BISO value codec. Scalars and the live
/// Vector/Map selector contracts are supported; unsupported objects return
/// `NonTransferable` (19).
///
/// # Safety
///
/// `api` must point to the live process-owned IdApi table. `value` must be a
/// live floor OOP for the duration of the call. `out` must point to at least
/// `out_cap` writable bytes and `out_len` must point to writable `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_rust_encode_oop(
    api: *mut IdApi,
    value: Oop,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if api.is_null() || out.is_null() || out_len.is_null() {
        return -1;
    }
    // SAFETY: the caller promises a live IdApi table for this synchronous
    // conversion; the null check above establishes a valid reference.
    let api = unsafe { &*api };
    let mut encoder = OopEncoder::new(api);
    let bytes = match encoder.value(value, 0) {
        Ok(()) => encoder.bytes,
        Err(error) => return error_code(error),
    };
    // SAFETY: the caller promises writable `usize` storage.
    unsafe { *out_len = bytes.len() };
    if out_cap < bytes.len() {
        return -2;
    }
    // SAFETY: the caller promises `out_cap` writable bytes and the capacity
    // check above proves the encoded slice fits.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    0
}

/// Decode one BISO value into a live floor OOP through the supplied IdApi.
/// The decoded object is owned by the floor's normal object model; this
/// bridge does not retain the input buffer after returning.
///
/// # Safety
///
/// `api` must point to the live process-owned IdApi table. `bytes` must be
/// non-null and point to `len` readable bytes. `out` must point to writable
/// storage for one OOP. All referenced floor objects and function pointers
/// must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_rust_decode_oop(
    api: *mut IdApi,
    bytes: *const u8,
    len: usize,
    out: *mut Oop,
) -> i32 {
    if api.is_null() || bytes.is_null() || out.is_null() {
        return -1;
    }
    // SAFETY: the caller promises a live IdApi table for this synchronous
    // conversion; the null check above establishes a valid reference.
    let api = unsafe { &*api };
    // SAFETY: the caller promises that `bytes` points to `len` readable
    // bytes; the null check above establishes a non-null base pointer.
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
    // SAFETY: the caller promises writable OOP storage; initialize it before
    // decoding so failures cannot expose stale caller data.
    unsafe { *out = ptr::null_mut() };
    let value = match decode_oop_value(api, bytes) {
        Ok(value) => value,
        Err(error) => return error_code(error),
    };
    // SAFETY: the caller promises writable OOP storage; null checks above
    // establish a valid destination and the decoded value is a live OOP.
    unsafe { *out = value };
    0
}

fn valid_map_key(value: &Value) -> bool {
    matches!(
        value,
        Value::Nil
            | Value::False
            | Value::True
            | Value::Int(_)
            | Value::String(_)
            | Value::Symbol(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_exact_network_order_wire_v1() {
        let bytes = encode_header(Header {
            kind: FrameKind::Request,
            request_id: 0x0102_0304_0506_0708,
            payload_len: 0x0012_3344,
        })
        .expect("header");
        assert_eq!(
            bytes,
            [
                b'B', b'I', b'S', b'O', 0, 1, 3, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0x12, 0x33, 0x44,
            ]
        );
        assert_eq!(
            decode_header(&bytes).expect("decode"),
            Header {
                kind: FrameKind::Request,
                request_id: 0x0102_0304_0506_0708,
                payload_len: 0x0012_3344,
            }
        );
    }

    #[test]
    fn header_rejects_invalid_request_ids() {
        assert_eq!(
            encode_header(Header {
                kind: FrameKind::Request,
                request_id: 0,
                payload_len: 0,
            }),
            Err(CodecError::InvalidRequestId)
        );
        assert_eq!(
            encode_header(Header {
                kind: FrameKind::Ready,
                request_id: 1,
                payload_len: 0,
            }),
            Err(CodecError::InvalidRequestId)
        );
        assert!(
            encode_header(Header {
                kind: FrameKind::ProtocolError,
                request_id: 0,
                payload_len: 1,
            })
            .is_ok()
        );
    }

    #[test]
    fn frames_round_trip_and_reject_transport_shape_errors() {
        let frame = Frame {
            header: Header {
                kind: FrameKind::Request,
                request_id: 7,
                payload_len: 1,
            },
            payload: vec![TAG_NIL],
        };
        let encoded = frame.encode().expect("frame encode");
        assert_eq!(Frame::decode(&encoded).expect("frame decode"), frame);

        let truncated = &encoded[..encoded.len() - 1];
        assert_eq!(Frame::decode(truncated), Err(CodecError::Truncated));
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(Frame::decode(&trailing), Err(CodecError::TrailingBytes));

        let ready = Frame {
            header: Header {
                kind: FrameKind::Ready,
                request_id: 0,
                payload_len: 0,
            },
            payload: vec![TAG_NIL],
        };
        assert_eq!(ready.encode(), Err(CodecError::InvalidRecord));
    }

    #[cfg(unix)]
    #[test]
    fn worker_control_plane_handshakes_and_stops() {
        use std::io::Write;
        use std::os::fd::IntoRawFd;
        use std::os::unix::net::UnixStream;
        use std::thread;

        let (mut host, worker) = UnixStream::pair().expect("socketpair");
        let worker_fd = worker.into_raw_fd();
        let join = thread::spawn(move || unsafe {
            birnam_isolate_worker_driver(
                ptr::null_mut(),
                worker_fd as c_int,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        });
        let ready = read_frame(&mut host).expect("ready");
        assert_eq!(ready.header.kind, FrameKind::Ready);
        assert_eq!(ready.header.request_id, 0);
        let stop = Frame {
            header: Header {
                kind: FrameKind::Stop,
                request_id: 1,
                payload_len: 0,
            },
            payload: Vec::new(),
        };
        host.write_all(&stop.encode().expect("stop encode"))
            .expect("stop write");
        let stopped = read_frame(&mut host).expect("stopped");
        assert_eq!(stopped.header.kind, FrameKind::Stopped);
        assert_eq!(stopped.header.request_id, 1);
        assert_eq!(join.join().expect("worker join"), 0);
    }

    #[cfg(unix)]
    #[test]
    fn worker_boot_error_uses_frozen_record_schema() {
        use std::ffi::CString;
        use std::os::fd::IntoRawFd;
        use std::os::unix::net::UnixStream;
        use std::thread;

        let (mut host, worker) = UnixStream::pair().expect("socketpair");
        let worker_fd = worker.into_raw_fd();
        let kind = CString::new("missingImage").expect("kind");
        let message = b"image unavailable";
        let join = thread::spawn(move || unsafe {
            birnam_isolate_worker_boot_error(
                ptr::null_mut(),
                worker_fd as c_int,
                kind.as_ptr(),
                message.as_ptr(),
                message.len(),
            );
        });
        let frame = read_frame(&mut host).expect("boot error");
        assert_eq!(frame.header.kind, FrameKind::BootError);
        assert_eq!(
            frame.payload,
            error_record(BOOT_ERROR_SCHEMA, b"missingImage", message).expect("record")
        );
        join.join().expect("boot-error join");
    }

    #[test]
    fn values_round_trip_with_float_bits_and_nested_containers() {
        let value = Value::Map(vec![
            (
                Value::Symbol(b"items".to_vec()),
                Value::Vector(vec![
                    Value::Nil,
                    Value::Int(-42),
                    Value::Float((-0.0f64).to_bits()),
                    Value::String(b"a\0b".to_vec()),
                ]),
            ),
            (Value::String(b"ok".to_vec()), Value::True),
        ]);
        let encoded = encode_value(&value).expect("encode");
        assert_eq!(decode_value(&encoded).expect("decode"), value);
    }

    #[test]
    fn map_keys_are_restricted_and_unique() {
        let duplicate = Value::Map(vec![
            (Value::Int(1), Value::Nil),
            (Value::Int(1), Value::True),
        ]);
        assert_eq!(encode_value(&duplicate), Err(CodecError::DuplicateMapKey));
        let float_key = Value::Map(vec![(Value::float(1.0), Value::Nil)]);
        assert_eq!(encode_value(&float_key), Err(CodecError::InvalidMapKey));
        assert_eq!(
            decode_value(&[
                TAG_MAP, 0, 0, 0, 1, TAG_FLOAT, 0, 0, 0, 0, 0, 0, 0, 0, 0, TAG_NIL
            ]),
            Err(CodecError::InvalidMapKey)
        );
    }

    #[test]
    fn malformed_values_fail_closed() {
        assert_eq!(decode_value(&[]), Err(CodecError::Truncated));
        assert_eq!(
            decode_value(&[TAG_NIL, TAG_NIL]),
            Err(CodecError::TrailingBytes)
        );
        assert_eq!(decode_value(&[0xff]), Err(CodecError::UnknownTag));
        assert_eq!(
            decode_value(&[TAG_SYMBOL, 0, 0, 0, 1, 0xff]),
            Err(CodecError::InvalidUtf8)
        );
        assert_eq!(
            decode_value(&[TAG_RECORD]),
            Err(CodecError::UnknownRecordSchema)
        );
    }

    #[test]
    fn symbols_reject_nul_and_depth_is_bounded() {
        assert_eq!(
            encode_value(&Value::Symbol(b"bad\0symbol".to_vec())),
            Err(CodecError::InvalidUtf8)
        );
        let mut nested = Value::Nil;
        for _ in 0..=MAX_DEPTH {
            nested = Value::Vector(vec![nested]);
        }
        assert_eq!(encode_value(&nested), Err(CodecError::TooDeep));
    }
}

birnam_abi::integer_dependency!();

birnam_abi::float_dependency!();

birnam_abi::string_dependency!();
