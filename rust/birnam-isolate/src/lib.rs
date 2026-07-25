//! Pointer-free primitives for the Isolate wire-v1 protocol.
//!
//! This first R3 slice deliberately owns no `IdApi` values, worker handles, or
//! process state. It mirrors the byte-level contract in `src/id/isolate.zig` so
//! the later dynamic rung can put orchestration around a stable, independently
//! tested codec.

use std::ffi::c_char;
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
