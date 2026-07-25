//! Pointer-free primitives for the Isolate wire-v1 protocol.
//!
//! This first R3 slice deliberately owns no `IdApi` values, worker handles, or
//! process state. It mirrors the byte-level contract in `src/id/isolate.zig` so
//! the later dynamic rung can put orchestration around a stable, independently
//! tested codec.

use birnam_abi::{IdApi, Oop};
use std::ffi::{CStr, c_char, c_int};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, RawFd};
use std::ptr;

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

/// Run the Rust control-plane worker skeleton over an inherited channel.
///
/// The skeleton owns no `IdApi` object conversion yet: it sends `ready`,
/// accepts `stop` and sends `stopped`, and rejects request frames. This keeps
/// the frozen ABI and shutdown handshake testable without pretending that
/// Rust can dispatch Birnam OOPs before the value bridge is ported.
///
/// # Safety
///
/// `api`, `receiver`, and `selector` follow the frozen host ABI and are only
/// borrowed for this call. `fd` must be a live inherited channel descriptor;
/// ownership remains with the caller and is not closed here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_isolate_worker_driver(
    _api: *mut IdApi,
    fd: c_int,
    _receiver: Oop,
    _selector: Oop,
) -> c_int {
    // SAFETY: the caller supplies a live descriptor and retains ownership;
    // ManuallyDrop prevents File from closing it on return.
    let mut channel = ManuallyDrop::new(unsafe { File::from_raw_fd(fd as RawFd) });
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
    let frame = match read_frame(borrowed_file(&mut channel)) {
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
            if write_frame(borrowed_file(&mut channel), &stopped).is_ok() {
                0
            } else {
                1
            }
        }
        FrameKind::Request => 1,
        _ => 1,
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

/// The pre-lifecycle Rust fixture has no host-side registry to drain.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_drain() {}

/// The pre-lifecycle Rust fixture has no host-side registry to shut down.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_shutdown() {}

/// Stable identity for the first Rust Isolate dynamic artifact.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_isolate_rust_identity() -> *const c_char {
    c"rust-isolate-r3-codec".as_ptr()
}

/// Encode a BISO header into caller-owned storage. This probe is deliberately
/// smaller than the eventual worker ABI: it lets the Zig corpus compare the
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
