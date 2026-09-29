//! Isolate wire-v1 codec and process transport. Language ownership is in native.
use std::collections::HashMap;
use std::ffi::c_int;
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, IntoRawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

mod asynchronous;
mod native;
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
