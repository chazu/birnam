//! Borrowed framing primitives and a checked writer for the portable Birnam v4 image.
//!
//! This format-only crate owns no `IdApi` values and performs no allocation. It
//! validates the v4 image header, authority/code relationship, borrowed field
//! envelopes, facts, and layout-driven heap records without materializing any
//! runtime state. The writer emits only framing and caller-owned payloads;
//! higher layers remain responsible for name resolution, swizzling, loading,
//! heap walking, and Store mutation.

#![allow(clippy::missing_safety_doc)]

pub const MAGIC: &[u8; 8] = b"BIRNAMIM";
pub const FORMAT_VERSION: u32 = 4;
pub const ENDIAN_TAG: u32 = 0x0102_0304;
pub const HEADER_LEN: usize = 28;
pub const RECORD_HEADER_LEN: usize = 12;
pub const AUTHORITY_DIGEST_LEN: usize = 64;
pub const CODE_PREFIX_LEN: usize = 8 + AUTHORITY_DIGEST_LEN;
pub const FIELD_LENGTH_PREFIX: usize = 4;

use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Truncated,
    MagicMismatch,
    UnsupportedVersion,
    EndianMismatch,
    ArchitectureMismatch,
    AbiMismatch,
    InvalidRecordKind,
    InvalidPayloadLength,
    InvalidFieldTag,
    LayoutMismatch,
    MissingAuthority,
    DuplicateAuthority,
    CodeBeforeAuthority,
    DuplicateCode,
    AuthorityMismatch,
    CodeChecksumMismatch,
    RecordTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub architecture: u32,
    pub abi_hash: u64,
}

impl Header {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Truncated);
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(Error::MagicMismatch);
        }
        if read_u32(&bytes[8..12]) != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if read_u32(&bytes[12..16]) != ENDIAN_TAG {
            return Err(Error::EndianMismatch);
        }
        Ok(Self {
            architecture: read_u32(&bytes[16..20]),
            abi_hash: read_u64(&bytes[20..28]),
        })
    }

    pub fn require(self, architecture: u32, abi_hash: u64) -> Result<Self, Error> {
        if self.architecture != architecture {
            return Err(Error::ArchitectureMismatch);
        }
        if self.abi_hash != abi_hash {
            return Err(Error::AbiMismatch);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RecordKind {
    Facts = 2,
    Heap = 3,
    Code = 4,
    Authority = 5,
}

impl RecordKind {
    fn parse(raw: u32) -> Result<Self, Error> {
        match raw {
            2 => Ok(Self::Facts),
            3 => Ok(Self::Heap),
            4 => Ok(Self::Code),
            5 => Ok(Self::Authority),
            _ => Err(Error::InvalidRecordKind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    pub kind: RecordKind,
    pub payload: &'a [u8],
}

/// A semantically preflighted v4 image. The records remain borrowed and can
/// be iterated again after the authority/code relationship has been checked.
#[derive(Debug, Clone, Copy)]
pub struct Image<'a> {
    pub header: Header,
    pub authority: Authority<'a>,
    code: Option<Code<'a>>,
    records: Records<'a>,
}

impl<'a> Image<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let header = Header::parse(bytes)?;
        let record_bytes = bytes.get(HEADER_LEN..).ok_or(Error::Truncated)?;
        let mut records = Records::new(record_bytes);
        let mut authority = None;
        let mut code = None;
        while let Some(record) = records.next_record()? {
            match record.kind {
                RecordKind::Authority => {
                    if authority.is_some() {
                        return Err(Error::DuplicateAuthority);
                    }
                    if code.is_some() {
                        return Err(Error::CodeBeforeAuthority);
                    }
                    authority = Some(Authority::parse(record.payload)?);
                }
                RecordKind::Code => {
                    let Some(authority_value) = authority else {
                        return Err(Error::CodeBeforeAuthority);
                    };
                    if code.is_some() {
                        return Err(Error::DuplicateCode);
                    }
                    let code_value = Code::parse(record.payload)?;
                    if code_value.generation_digest != authority_value.digest {
                        return Err(Error::AuthorityMismatch);
                    }
                    if !code_value.checksum_matches() {
                        return Err(Error::CodeChecksumMismatch);
                    }
                    code = Some(code_value);
                }
                RecordKind::Facts | RecordKind::Heap => {}
            }
        }
        let authority = authority.ok_or(Error::MissingAuthority)?;
        Ok(Self {
            header,
            authority,
            code,
            records: Records::new(record_bytes),
        })
    }

    pub fn code(&self) -> Option<Code<'a>> {
        self.code
    }

    pub fn records(&self) -> Records<'a> {
        self.records
    }

    /// Validate every facts record's count and tagged-field framing without
    /// resolving any symbol, export, or heap reference.
    pub fn validate_facts(&self) -> Result<(), Error> {
        let mut records = self.records();
        while let Some(record) = records.next_record()? {
            if record.kind != RecordKind::Facts {
                continue;
            }
            let mut facts = Facts::parse(record.payload)?;
            while facts.next_fact()?.is_some() {}
        }
        Ok(())
    }
}

/// The v4 authority record is exactly one borrowed 64-byte generation digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authority<'a> {
    pub digest: &'a [u8; AUTHORITY_DIGEST_LEN],
}

impl<'a> Authority<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        Ok(Self {
            digest: payload
                .try_into()
                .map_err(|_| Error::InvalidPayloadLength)?,
        })
    }
}

/// The fixed prefix of a v4 code record, borrowed from the image bytes.
///
/// The checksum is the existing image integrity field; the generation digest
/// binds the unit to the preceding authority record. Hash verification and
/// dynamic loading remain higher-layer responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code<'a> {
    pub checksum: u64,
    pub generation_digest: &'a [u8; AUTHORITY_DIGEST_LEN],
    pub unit: &'a [u8],
}

impl<'a> Code<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < CODE_PREFIX_LEN {
            return Err(Error::InvalidPayloadLength);
        }
        Ok(Self {
            checksum: read_u64(&payload[..8]),
            generation_digest: payload[8..CODE_PREFIX_LEN]
                .try_into()
                .map_err(|_| Error::InvalidPayloadLength)?,
            unit: &payload[CODE_PREFIX_LEN..],
        })
    }

    pub fn checksum_matches(self) -> bool {
        wyhash(0, self.unit) == self.checksum
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldTag {
    Nil = 0,
    True = 1,
    False = 2,
    Integer = 3,
    FloatBits = 4,
    Symbol = 5,
    String = 6,
    HeapRef = 7,
    NullOop = 8,
    ExportRef = 9,
}

impl FieldTag {
    fn parse(raw: u8) -> Result<Self, Error> {
        match raw {
            0 => Ok(Self::Nil),
            1 => Ok(Self::True),
            2 => Ok(Self::False),
            3 => Ok(Self::Integer),
            4 => Ok(Self::FloatBits),
            5 => Ok(Self::Symbol),
            6 => Ok(Self::String),
            7 => Ok(Self::HeapRef),
            8 => Ok(Self::NullOop),
            9 => Ok(Self::ExportRef),
            _ => Err(Error::InvalidFieldTag),
        }
    }
}

/// One borrowed v4 heap/fact field. Float values retain their serialized bits
/// so NaN payloads and signed zero remain lossless until a runtime materializer
/// deliberately chooses a representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field<'a> {
    Nil,
    True,
    False,
    Integer(i64),
    FloatBits(u64),
    Symbol(&'a [u8]),
    String(&'a [u8]),
    HeapRef(u32),
    NullOop,
    ExportRef(&'a [u8]),
}

#[derive(Debug, Clone, Copy)]
pub struct Fields<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Fields<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn next_field(&mut self) -> Result<Option<Field<'a>>, Error> {
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let tag = FieldTag::parse(self.bytes[self.offset])?;
        self.offset += 1;
        let field = match tag {
            FieldTag::Nil => Field::Nil,
            FieldTag::True => Field::True,
            FieldTag::False => Field::False,
            FieldTag::NullOop => Field::NullOop,
            FieldTag::Integer => Field::Integer(i64::from_le_bytes(self.take_fixed()?)),
            FieldTag::FloatBits => Field::FloatBits(u64::from_le_bytes(self.take_fixed()?)),
            FieldTag::HeapRef => Field::HeapRef(u32::from_le_bytes(self.take_fixed()?)),
            FieldTag::Symbol => Field::Symbol(self.take_bytes()?),
            FieldTag::String => Field::String(self.take_bytes()?),
            FieldTag::ExportRef => Field::ExportRef(self.take_bytes()?),
        };
        Ok(Some(field))
    }

    fn take_byte(&mut self) -> Result<u8, Error> {
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or(Error::InvalidPayloadLength)?;
        self.offset += 1;
        Ok(byte)
    }

    fn is_exhausted(&self) -> bool {
        self.offset == self.bytes.len()
    }

    pub fn consumed(&self) -> usize {
        self.offset
    }

    fn take_fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let end = self.offset.checked_add(N).ok_or(Error::RecordTooLarge)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::InvalidPayloadLength)?;
        self.offset = end;
        Ok(bytes.try_into().expect("fixed-width field slice"))
    }

    fn take_bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = u32::from_le_bytes(self.take_fixed()?) as usize;
        let end = self.offset.checked_add(len).ok_or(Error::RecordTooLarge)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::InvalidPayloadLength)?;
        self.offset = end;
        Ok(bytes)
    }
}

/// One borrowed fact from a v4 facts record. The field values are intentionally
/// left tagged; a loader decides how to resolve symbols, exports, and heap refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact<'a> {
    pub op: u8,
    pub tx: u64,
    pub entity: Field<'a>,
    pub attribute: Field<'a>,
    pub value: Field<'a>,
}

/// Borrowed iterator over the v4 facts payload (`basis`, `count`, facts...).
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub basis: u64,
    remaining: u32,
    fields: Fields<'a>,
}

impl<'a> Facts<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < 12 {
            return Err(Error::InvalidPayloadLength);
        }
        Ok(Self {
            basis: read_u64(&payload[..8]),
            remaining: read_u32(&payload[8..12]),
            fields: Fields::new(&payload[12..]),
        })
    }

    pub fn next_fact(&mut self) -> Result<Option<Fact<'a>>, Error> {
        if self.remaining == 0 {
            if self.fields.is_exhausted() {
                return Ok(None);
            }
            return Err(Error::InvalidPayloadLength);
        }
        let op = self.fields.take_byte()?;
        let tx = u64::from_le_bytes(self.fields.take_fixed()?);
        let entity = self
            .fields
            .next_field()?
            .ok_or(Error::InvalidPayloadLength)?;
        let attribute = self
            .fields
            .next_field()?
            .ok_or(Error::InvalidPayloadLength)?;
        let value = self
            .fields
            .next_field()?
            .ok_or(Error::InvalidPayloadLength)?;
        self.remaining -= 1;
        Ok(Some(Fact {
            op,
            tx,
            entity,
            attribute,
            value,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Oop,
    RawWord,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapHeader<'a> {
    pub vtable_name: &'a [u8],
    pub word_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapObject<'a> {
    pub header: HeapHeader<'a>,
    /// The encoded bytes for the object's non-code slots. Code slots are
    /// deliberately absent from v4 payloads and are reconstructed by a later
    /// mapped-unit/replay layer.
    pub encoded_slots: &'a [u8],
}

/// Borrowed iterator over a v4 heap payload. The caller supplies the live
/// vtable layout after peeking at each object's name; the codec only validates
/// framing and slot widths and never resolves names or heap references.
#[derive(Debug, Clone, Copy)]
pub struct Heap<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: u32,
}

impl<'a> Heap<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < 4 {
            return Err(Error::InvalidPayloadLength);
        }
        Ok(Self {
            bytes: payload,
            offset: 4,
            remaining: read_u32(&payload[..4]),
        })
    }

    pub fn peek_header(&self) -> Result<Option<HeapHeader<'a>>, Error> {
        self.header_at()
            .map(|entry| entry.map(|(header, _)| header))
    }

    pub fn next_object(&mut self, layout: &[SlotKind]) -> Result<Option<HeapObject<'a>>, Error> {
        let Some((header, body_start)) = self.header_at()? else {
            return Ok(None);
        };
        if layout.len() != usize::try_from(header.word_count).unwrap_or(usize::MAX) {
            return Err(Error::LayoutMismatch);
        }
        let mut cursor = body_start;
        for slot in layout {
            match slot {
                SlotKind::Oop => {
                    let mut fields = Fields::new(
                        self.bytes
                            .get(cursor..)
                            .ok_or(Error::InvalidPayloadLength)?,
                    );
                    fields.next_field()?.ok_or(Error::InvalidPayloadLength)?;
                    cursor = cursor
                        .checked_add(fields.consumed())
                        .ok_or(Error::RecordTooLarge)?;
                }
                SlotKind::RawWord => {
                    cursor = cursor.checked_add(8).ok_or(Error::RecordTooLarge)?;
                    if cursor > self.bytes.len() {
                        return Err(Error::InvalidPayloadLength);
                    }
                }
                SlotKind::Code => {}
            }
        }
        let encoded_slots = self
            .bytes
            .get(body_start..cursor)
            .ok_or(Error::InvalidPayloadLength)?;
        self.offset = cursor;
        self.remaining -= 1;
        Ok(Some(HeapObject {
            header,
            encoded_slots,
        }))
    }

    fn header_at(&self) -> Result<Option<(HeapHeader<'a>, usize)>, Error> {
        if self.remaining == 0 {
            return if self.offset == self.bytes.len() {
                Ok(None)
            } else {
                Err(Error::InvalidPayloadLength)
            };
        }
        let mut cursor = self.offset;
        let name_len = read_u32_at(self.bytes, &mut cursor)? as usize;
        let name_end = cursor.checked_add(name_len).ok_or(Error::RecordTooLarge)?;
        let vtable_name = self
            .bytes
            .get(cursor..name_end)
            .ok_or(Error::InvalidPayloadLength)?;
        cursor = name_end;
        let word_count = read_u32_at(self.bytes, &mut cursor)?;
        Ok(Some((
            HeapHeader {
                vtable_name,
                word_count,
            },
            cursor,
        )))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Records<'a> {
    bytes: &'a [u8],
    offset: usize,
}

/// Errors raised while assembling a v4 image's framed records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    MissingAuthority,
    DuplicateAuthority,
    CodeBeforeAuthority,
    DuplicateCode,
    AuthorityMismatch,
    InvalidFields,
    PayloadTooLarge,
}

/// Encoder for one or more length-delimited v4 field values.
///
/// The encoder only owns its output bytes. It does not resolve or retain any
/// runtime object; callers supply the already-classified scalar, name, or
/// relocation value.
#[derive(Default)]
pub struct FieldEncoder {
    bytes: Vec<u8>,
}

impl FieldEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn nil(&mut self) {
        self.bytes.push(FieldTag::Nil as u8);
    }

    pub fn true_value(&mut self) {
        self.bytes.push(FieldTag::True as u8);
    }

    pub fn false_value(&mut self) {
        self.bytes.push(FieldTag::False as u8);
    }

    pub fn integer(&mut self, value: i64) {
        self.bytes.push(FieldTag::Integer as u8);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn float_bits(&mut self, bits: u64) {
        self.bytes.push(FieldTag::FloatBits as u8);
        self.bytes.extend_from_slice(&bits.to_le_bytes());
    }

    pub fn heap_ref(&mut self, id: u32) {
        self.bytes.push(FieldTag::HeapRef as u8);
        self.bytes.extend_from_slice(&id.to_le_bytes());
    }

    pub fn null_oop(&mut self) {
        self.bytes.push(FieldTag::NullOop as u8);
    }

    pub fn symbol(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.length_delimited(FieldTag::Symbol, bytes)
    }

    pub fn string(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.length_delimited(FieldTag::String, bytes)
    }

    pub fn export_ref(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.length_delimited(FieldTag::ExportRef, bytes)
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn length_delimited(&mut self, tag: FieldTag, bytes: &[u8]) -> Result<(), EncodeError> {
        let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        self.bytes.push(tag as u8);
        self.bytes.extend_from_slice(&length.to_le_bytes());
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

/// Builder for one v4 facts payload. Every fact is appended atomically as an
/// operation, transaction id, and exactly three already-classified fields.
pub struct FactsWriter {
    bytes: Vec<u8>,
    count: u32,
}

impl FactsWriter {
    pub fn new(basis: u64) -> Self {
        let mut bytes = Vec::with_capacity(12);
        bytes.extend_from_slice(&basis.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        Self { bytes, count: 0 }
    }

    pub fn fact(
        &mut self,
        op: u8,
        tx: u64,
        entity: Field<'_>,
        attribute: Field<'_>,
        value: Field<'_>,
    ) -> Result<(), EncodeError> {
        let mut fields = FieldEncoder::new();
        append_field(&mut fields, entity)?;
        append_field(&mut fields, attribute)?;
        append_field(&mut fields, value)?;
        self.fact_encoded(op, tx, fields.as_bytes())
    }

    pub fn fact_encoded(
        &mut self,
        op: u8,
        tx: u64,
        encoded_fields: &[u8],
    ) -> Result<(), EncodeError> {
        let mut fields = Fields::new(encoded_fields);
        for _ in 0..3 {
            if fields
                .next_field()
                .map_err(|_| EncodeError::InvalidFields)?
                .is_none()
            {
                return Err(EncodeError::InvalidFields);
            }
        }
        if fields
            .next_field()
            .map_err(|_| EncodeError::InvalidFields)?
            .is_some()
        {
            return Err(EncodeError::InvalidFields);
        }
        let count = self
            .count
            .checked_add(1)
            .ok_or(EncodeError::PayloadTooLarge)?;
        self.bytes.push(op);
        self.bytes.extend_from_slice(&tx.to_le_bytes());
        self.bytes.extend_from_slice(encoded_fields);
        self.count = count;
        self.bytes[8..12].copy_from_slice(&count.to_le_bytes());
        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn append_field(encoder: &mut FieldEncoder, field: Field<'_>) -> Result<(), EncodeError> {
    match field {
        Field::Nil => encoder.nil(),
        Field::True => encoder.true_value(),
        Field::False => encoder.false_value(),
        Field::Integer(value) => encoder.integer(value),
        Field::FloatBits(bits) => encoder.float_bits(bits),
        Field::Symbol(bytes) => encoder.symbol(bytes)?,
        Field::String(bytes) => encoder.string(bytes)?,
        Field::HeapRef(id) => encoder.heap_ref(id),
        Field::NullOop => encoder.null_oop(),
        Field::ExportRef(bytes) => encoder.export_ref(bytes)?,
    }
    Ok(())
}

/// Checked, allocation-owning v4 framing writer.
///
/// This type deliberately accepts already-encoded facts/heap/code payloads. It
/// does not inspect or mutate runtime objects, and it never publishes a file.
pub struct ImageWriter {
    bytes: Vec<u8>,
    authority: Option<[u8; AUTHORITY_DIGEST_LEN]>,
    code_written: bool,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Publish complete image bytes through a same-directory temporary file and
/// rename. The destination is never opened for truncation; failed writes clean
/// up their temporary file and leave an existing destination untouched.
pub fn publish_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "image path has no file name")
    })?;
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(file_name);
    temporary_name.push(format!(
        ".birnam-tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary_path = parent.join(temporary_name);

    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

impl ImageWriter {
    pub fn new(architecture: u32, abi_hash: u64) -> Self {
        let mut bytes = Vec::with_capacity(HEADER_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&ENDIAN_TAG.to_le_bytes());
        bytes.extend_from_slice(&architecture.to_le_bytes());
        bytes.extend_from_slice(&abi_hash.to_le_bytes());
        Self {
            bytes,
            authority: None,
            code_written: false,
        }
    }

    pub fn facts(&mut self, payload: &[u8]) -> Result<(), EncodeError> {
        self.record(RecordKind::Facts, payload)
    }

    pub fn heap(&mut self, payload: &[u8]) -> Result<(), EncodeError> {
        self.record(RecordKind::Heap, payload)
    }

    pub fn authority(&mut self, digest: &[u8; AUTHORITY_DIGEST_LEN]) -> Result<(), EncodeError> {
        if self.authority.is_some() {
            return Err(EncodeError::DuplicateAuthority);
        }
        self.push_record(RecordKind::Authority, digest)?;
        self.authority = Some(*digest);
        Ok(())
    }

    pub fn code(
        &mut self,
        checksum: u64,
        generation_digest: &[u8; AUTHORITY_DIGEST_LEN],
        unit: &[u8],
    ) -> Result<(), EncodeError> {
        let Some(authority) = self.authority else {
            return Err(EncodeError::CodeBeforeAuthority);
        };
        if self.code_written {
            return Err(EncodeError::DuplicateCode);
        }
        if authority != *generation_digest {
            return Err(EncodeError::AuthorityMismatch);
        }
        let payload_len = 8usize
            .checked_add(AUTHORITY_DIGEST_LEN)
            .and_then(|length| length.checked_add(unit.len()))
            .ok_or(EncodeError::PayloadTooLarge)?;
        let mut payload = Vec::with_capacity(payload_len);
        payload.extend_from_slice(&checksum.to_le_bytes());
        payload.extend_from_slice(generation_digest);
        payload.extend_from_slice(unit);
        self.push_record(RecordKind::Code, &payload)?;
        self.code_written = true;
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, EncodeError> {
        if self.authority.is_none() {
            return Err(EncodeError::MissingAuthority);
        }
        Ok(self.bytes)
    }

    fn record(&mut self, kind: RecordKind, payload: &[u8]) -> Result<(), EncodeError> {
        match kind {
            RecordKind::Facts | RecordKind::Heap => self.push_record(kind, payload),
            RecordKind::Code => Err(EncodeError::CodeBeforeAuthority),
            RecordKind::Authority => Err(EncodeError::DuplicateAuthority),
        }
    }

    fn push_record(&mut self, kind: RecordKind, payload: &[u8]) -> Result<(), EncodeError> {
        let length = u64::try_from(payload.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        self.bytes.extend_from_slice(&(kind as u32).to_le_bytes());
        self.bytes.extend_from_slice(&length.to_le_bytes());
        self.bytes.extend_from_slice(payload);
        Ok(())
    }
}

impl<'a> Records<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn next_record(&mut self) -> Result<Option<Record<'a>>, Error> {
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let end = self
            .offset
            .checked_add(RECORD_HEADER_LEN)
            .ok_or(Error::RecordTooLarge)?;
        if end > self.bytes.len() {
            return Err(Error::Truncated);
        }
        let kind = RecordKind::parse(read_u32(&self.bytes[self.offset..self.offset + 4]))?;
        let length = usize::try_from(read_u64(&self.bytes[self.offset + 4..end]))
            .map_err(|_| Error::RecordTooLarge)?;
        let payload_start = end;
        let payload_end = payload_start
            .checked_add(length)
            .ok_or(Error::RecordTooLarge)?;
        if payload_end > self.bytes.len() {
            return Err(Error::Truncated);
        }
        self.offset = payload_end;
        Ok(Some(Record {
            kind,
            payload: &self.bytes[payload_start..payload_end],
        }))
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
}

fn read_u32_at(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
    let end = (*offset).checked_add(4).ok_or(Error::RecordTooLarge)?;
    let value = bytes
        .get(*offset..end)
        .ok_or(Error::InvalidPayloadLength)
        .map(read_u32)?;
    *offset = end;
    Ok(value)
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
}

const WYHASH_SECRET: [u64; 4] = [
    0xa076_1d64_78bd_642f,
    0xe703_7ed1_a0b4_28db,
    0x8ebc_6af0_9c88_c6e3,
    0x5899_65cc_7537_4cc3,
];

fn wyhash(seed: u64, input: &[u8]) -> u64 {
    let mut state = [seed ^ mix(seed ^ WYHASH_SECRET[0], WYHASH_SECRET[1]); 3];
    let (a, b) = if input.len() <= 16 {
        let mut a = 0;
        let mut b = 0;
        small_key(input, &mut a, &mut b);
        (a, b)
    } else {
        let mut offset = 0;
        if input.len() >= 48 {
            while offset + 48 < input.len() {
                round(&mut state, &input[offset..offset + 48]);
                offset += 48;
            }
            state[0] ^= state[1] ^ state[2];
        }
        let remaining = &input[offset..];
        let mut inner = 0;
        while inner + 16 < remaining.len() {
            state[0] = mix(
                read_u64(&remaining[inner..inner + 8]) ^ WYHASH_SECRET[1],
                read_u64(&remaining[inner + 8..inner + 16]) ^ state[0],
            );
            inner += 16;
        }
        (
            read_u64(&input[input.len() - 16..input.len() - 8]),
            read_u64(&input[input.len() - 8..]),
        )
    };
    let mut a = a ^ WYHASH_SECRET[1];
    let mut b = b ^ state[0];
    let product = (a as u128).wrapping_mul(b as u128);
    a = product as u64;
    b = (product >> 64) as u64;
    mix(
        a ^ WYHASH_SECRET[0] ^ input.len() as u64,
        b ^ WYHASH_SECRET[1],
    )
}

fn small_key(input: &[u8], a: &mut u64, b: &mut u64) {
    if input.len() >= 4 {
        let end = input.len() - 4;
        let quarter = (input.len() >> 3) << 2;
        *a = (u64::from(read_u32(&input[..4])) << 32)
            | u64::from(read_u32(&input[quarter..quarter + 4]));
        *b = (u64::from(read_u32(&input[end..end + 4])) << 32)
            | u64::from(read_u32(&input[end - quarter..end - quarter + 4]));
    } else if !input.is_empty() {
        *a = (u64::from(input[0]) << 16)
            | (u64::from(input[input.len() >> 1]) << 8)
            | u64::from(input[input.len() - 1]);
    }
}

fn round(state: &mut [u64; 3], input: &[u8]) {
    for index in 0..3 {
        state[index] = mix(
            read_u64(&input[index * 16..index * 16 + 8]) ^ WYHASH_SECRET[index + 1],
            read_u64(&input[index * 16 + 8..index * 16 + 16]) ^ state[index],
        );
    }
}

fn mix(left: u64, right: u64) -> u64 {
    let product = (left as u128).wrapping_mul(right as u128);
    product as u64 ^ (product >> 64) as u64
}

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Mutex, OnceLock};

const RUST_IMAGE_IDENTITY: &[u8] = b"rust-image-r6-shell\0";
const IMAGE_REPORT_VT_NAME: &[u8] = b"ImageSaveReportVt\0";
const IMAGE_REPORT_NAME: &[u8] = b"ImageSaveReport\0";
const COLLECTOR_VT_NAME: &[u8] = b"ImageCollectorVt\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    report_obj: usize,
    report: &'static Mutex<SaveReport>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

struct FactCollection {
    writer: FactsWriter,
    heap: HeapPreflight,
    kept: u32,
    rejected: u32,
}

static FACT_COLLECTION: Mutex<Option<FactCollection>> = Mutex::new(None);

struct HeapPreflight {
    objects: Vec<usize>,
    relocations: HashMap<usize, u32>,
    recovered_slots: u32,
    rejected_slots: u32,
}

unsafe fn runtime_oop_is_inline(api: &IdApi, object: Oop) -> bool {
    if object.is_null()
        || object == api.nilObj
        || object == api.trueObj
        || object == api.falseObj
        || unsafe { (api.isSmallInt)(object) } != 0
        || unsafe { (api.isFloat)(object) } != 0
    {
        return true;
    }
    let vtable = unsafe { (api.vtableOf)(object) };
    vtable == api.symbol_vt
        || vtable == api.string_vt
        || unsafe { c_string_bytes((api.exportedNameOf)(object)) }.is_some()
}

impl HeapPreflight {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
            relocations: HashMap::new(),
            recovered_slots: 0,
            rejected_slots: 0,
        }
    }

    unsafe fn intern(&mut self, api: &IdApi, object: Oop) -> Option<u32> {
        let key = object as usize;
        if let Some(id) = self.relocations.get(&key) {
            return Some(*id);
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if vtable.is_null() {
            return None;
        }
        let has_exported_vtable = unsafe { c_string_bytes((api.exportedNameOf)(vtable)) }.is_some();
        let owned_parent = unsafe { (api.vtableOwner)(vtable) } == object;
        let parent = unsafe { (api.vtableParent)(vtable) };
        let has_exported_parent = owned_parent
            && !parent.is_null()
            && unsafe { c_string_bytes((api.exportedNameOf)(parent)) }.is_some();
        if !has_exported_vtable && !has_exported_parent {
            return None;
        }
        let payload_size = unsafe { (api.objectPayloadSize)(object) };
        if !payload_size.is_multiple_of(core::mem::size_of::<usize>()) {
            return None;
        }
        let words = payload_size / core::mem::size_of::<usize>();
        for index in 0..words {
            let kind = unsafe { (api.slotKind)(vtable, index) };
            if kind != birnam_abi::SLOT_RAW
                && kind != birnam_abi::SLOT_OOP
                && kind != birnam_abi::SLOT_CODE
            {
                return None;
            }
        }
        let id = u32::try_from(self.objects.len()).ok()?;
        self.relocations.insert(key, id);
        self.objects.push(key);
        for index in 0..words {
            if unsafe { (api.slotKind)(vtable, index) } != birnam_abi::SLOT_OOP {
                continue;
            }
            let address = (object as *const u8).wrapping_add(index * core::mem::size_of::<usize>());
            // SAFETY: the layout marks this initialized payload word as an Oop.
            let child = unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
            if unsafe { runtime_oop_is_inline(api, child) } {
                continue;
            }
            unsafe { self.intern(api, child) }?;
        }
        Some(id)
    }

    unsafe fn finish(&mut self, api: &IdApi) -> Option<Vec<u8>> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&u32::try_from(self.objects.len()).ok()?.to_le_bytes());
        for object_index in 0..self.objects.len() {
            let object = self.objects[object_index] as Oop;
            let vtable = unsafe { (api.vtableOf)(object) };
            let (vtable, name) =
                if let Some(name) = unsafe { c_string_bytes((api.exportedNameOf)(vtable)) } {
                    (vtable, name)
                } else {
                    let parent = unsafe { (api.vtableParent)(vtable) };
                    let name = unsafe { c_string_bytes((api.exportedNameOf)(parent)) }?;
                    (parent, name)
                };
            let payload_size = unsafe { (api.objectPayloadSize)(object) };
            let words = payload_size / core::mem::size_of::<usize>();
            payload.extend_from_slice(&u32::try_from(name.len()).ok()?.to_le_bytes());
            payload.extend_from_slice(name);
            payload.extend_from_slice(&u32::try_from(words).ok()?.to_le_bytes());
            for index in 0..words {
                let kind = unsafe { (api.slotKind)(vtable, index) };
                if kind == birnam_abi::SLOT_CODE {
                    self.recovered_slots = self.recovered_slots.saturating_add(1);
                    continue;
                }
                let address =
                    (object as *const u8).wrapping_add(index * core::mem::size_of::<usize>());
                if kind == birnam_abi::SLOT_OOP {
                    // SAFETY: the preflight checked payload size and Oop layout.
                    let child =
                        unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
                    let mut field = FieldEncoder::new();
                    if !unsafe { encode_runtime_oop(api, &mut field, child, self) } {
                        self.rejected_slots = self.rejected_slots.saturating_add(1);
                        return None;
                    }
                    payload.extend_from_slice(field.as_bytes());
                } else {
                    // SAFETY: the preflight checked payload size and raw layout.
                    let word = unsafe { core::ptr::read_unaligned(address.cast::<usize>()) } as u64;
                    payload.extend_from_slice(&word.to_le_bytes());
                }
            }
        }
        Some(payload)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Default)]
enum ReportStatus {
    #[default]
    Never,
    Saved,
    Rejected,
    IoFailed,
}

#[derive(Clone, Copy)]
struct SaveReport {
    status: ReportStatus,
    saved_facts: u32,
    recovered_facts: u32,
    rejected_facts: u32,
    recovered_slots: u32,
    rejected_slots: u32,
    code_units: u32,
    detail: [u8; 256],
    detail_len: usize,
}

impl Default for SaveReport {
    fn default() -> Self {
        Self {
            status: ReportStatus::Never,
            saved_facts: 0,
            recovered_facts: 0,
            rejected_facts: 0,
            recovered_slots: 0,
            rejected_slots: 0,
            code_units: 0,
            detail: [0; 256],
            detail_len: 0,
        }
    }
}

static SAVE_REPORT: Mutex<SaveReport> = Mutex::new(SaveReport {
    status: ReportStatus::Never,
    saved_facts: 0,
    recovered_facts: 0,
    rejected_facts: 0,
    recovered_slots: 0,
    rejected_slots: 0,
    code_units: 0,
    detail: [0; 256],
    detail_len: 0,
});

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Image: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api_ref() -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains that table for the loaded floor's lifetime.
    unsafe { &*(runtime().api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    // SAFETY: all current selectors are static NUL-terminated byte strings.
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}

unsafe fn c_string_bytes<'a>(pointer: *const c_char) -> Option<&'a [u8]> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: IdApi returns process-owned NUL-terminated names that remain
    // valid for the loaded floor's lifetime.
    Some(unsafe { CStr::from_ptr(pointer) }.to_bytes())
}

unsafe fn encode_runtime_oop(
    api: &IdApi,
    encoder: &mut FieldEncoder,
    object: Oop,
    heap: &mut HeapPreflight,
) -> bool {
    if object.is_null() {
        encoder.null_oop();
        return true;
    }
    if object == api.nilObj {
        encoder.nil();
        return true;
    }
    if object == api.trueObj {
        encoder.true_value();
        return true;
    }
    if object == api.falseObj {
        encoder.false_value();
        return true;
    }
    if unsafe { (api.isSmallInt)(object) } != 0 {
        encoder.integer(unsafe { (api.smallIntValue)(object) });
        return true;
    }
    if unsafe { (api.isFloat)(object) } != 0 {
        encoder.float_bits(unsafe { (api.floatValue)(object).to_bits() });
        return true;
    }
    let vtable = unsafe { (api.vtableOf)(object) };
    if vtable == api.symbol_vt {
        let Some(bytes) = (unsafe { c_string_bytes((api.symbolName)(object)) }) else {
            return false;
        };
        return encoder.symbol(bytes).is_ok();
    }
    if let Some(name) = unsafe { c_string_bytes((api.exportedNameOf)(object)) } {
        return encoder.export_ref(name).is_ok();
    }
    if vtable == api.string_vt {
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(object, &mut length) };
        if bytes.is_null() && length != 0 {
            return false;
        }
        // SAFETY: stringBytes supplies `length` readable bytes for a live String.
        let bytes = if length == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        return encoder.string(bytes).is_ok();
    }
    let Some(id) = (unsafe { heap.intern(api, object) }) else {
        return false;
    };
    encoder.heap_ref(id);
    true
}

fn record_fact_counts(kept: u32, rejected: u32, recovered_slots: u32, rejected_slots: u32) {
    let mut report = runtime()
        .report
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    report.saved_facts = kept;
    report.rejected_facts = rejected;
    report.recovered_slots = recovered_slots;
    report.rejected_slots = rejected_slots;
}

unsafe fn collect_store_facts(api: &IdApi) -> (u32, u32, bool, u32, u32) {
    let store = unsafe { (api.import)(c"Store".as_ptr()) };
    if store.is_null() || store == api.nilObj {
        return (0, 0, true, 0, 0);
    }
    let collector_vt = unsafe { (api.import)(COLLECTOR_VT_NAME.as_ptr().cast()) };
    if collector_vt.is_null() {
        return (0, 0, false, 0, 0);
    }
    let basis_object = unsafe { (api.send)(store, intern(api, b"basis\0"), ptr::null(), 0) };
    let basis = if unsafe { (api.isSmallInt)(basis_object) } != 0 {
        unsafe { (api.smallIntValue)(basis_object) }.max(0) as u64
    } else {
        0
    };
    let collector = unsafe { (api.alloc)(collector_vt, 0) };
    if collector.is_null() {
        return (0, 0, false, 0, 0);
    }
    {
        let mut collection = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *collection = Some(FactCollection {
            writer: FactsWriter::new(basis),
            heap: HeapPreflight::new(),
            kept: 0,
            rejected: 0,
        });
    }
    let arguments = [collector];
    let _ = unsafe {
        (api.send)(
            store,
            intern(api, b"imageFactsInto:\0"),
            arguments.as_ptr(),
            arguments.len(),
        )
    };
    let collection = FACT_COLLECTION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let Some(mut collection) = collection else {
        return (0, 0, false, 0, 0);
    };
    let heap_ok = unsafe { collection.heap.finish(api).is_some() };
    let counts = (
        collection.kept,
        collection.rejected,
        heap_ok,
        collection.heap.recovered_slots,
        collection.heap.rejected_slots,
    );
    let _payload = collection.writer.finish();
    counts
}

unsafe extern "C" fn system_save_image(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        reject_save("saveImage: expects one String path");
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        reject_save("saveImage: received an invalid argument buffer");
        return api.nilObj;
    };
    let path = arguments[0];
    if path.is_null() || unsafe { (api.vtableOf)(path) } != api.string_vt {
        reject_save("saveImage: expects one String path");
        return api.nilObj;
    }
    // The shell deliberately has no authority to mutate or publish an image.
    // Later R6 slices install the codec-backed save transaction here.
    let (kept, rejected, heap_ok, recovered_slots, rejected_slots) =
        unsafe { collect_store_facts(api) };
    reject_save(if !heap_ok {
        "Rust Image heap preflight rejected unsupported layout"
    } else if rejected != 0 {
        "Rust Image fact preflight rejected unsupported values"
    } else {
        "Rust Image writer/publisher orchestration is not installed"
    });
    record_fact_counts(kept, rejected, recovered_slots, rejected_slots);
    api.nilObj
}

unsafe extern "C" fn system_load_image(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { borrowed_oop_args(args, count) }.is_none() {
        return api.nilObj;
    }
    api.nilObj
}

unsafe extern "C" fn system_load_image_rebuild(
    _self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 || unsafe { borrowed_oop_args(args, count) }.is_none() {
        return api.nilObj;
    }
    api.nilObj
}

unsafe extern "C" fn system_image_save_report(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        return unsafe { api_ref() }.nilObj;
    }
    runtime().report_obj as Oop
}

unsafe extern "C" fn system_image_composition_identity(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    // This identity is intentionally distinct from the eventual composition
    // digest; it proves the Rust shell was selected without claiming image
    // orchestration authority before the later R6 slices land.
    unsafe { (api.mkString)(RUST_IMAGE_IDENTITY.as_ptr(), RUST_IMAGE_IDENTITY.len() - 1) }
}

unsafe extern "C" fn system_bake_exe(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { borrowed_oop_args(args, count) }.is_none() {
        return api.nilObj;
    }
    api.nilObj
}

unsafe extern "C" fn report_status(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let status = runtime()
        .report
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .status;
    unsafe {
        intern(
            api,
            match status {
                ReportStatus::Never => b"never\0",
                ReportStatus::Saved => b"saved\0",
                ReportStatus::Rejected => b"rejected\0",
                ReportStatus::IoFailed => b"ioFailed\0",
            },
        )
    }
}

fn report_value(field: fn(&SaveReport) -> u32) -> u32 {
    let report = runtime()
        .report
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    field(&report)
}

unsafe fn report_count_value(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
    field: fn(&SaveReport) -> u32,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)(i64::from(report_value(field))) }
}

unsafe extern "C" fn report_saved_facts(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.saved_facts) }
}

unsafe extern "C" fn report_recovered_facts(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.recovered_facts) }
}

unsafe extern "C" fn report_rejected_facts(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.rejected_facts) }
}

unsafe extern "C" fn report_recovered_slots(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.recovered_slots) }
}

unsafe extern "C" fn report_rejected_slots(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.rejected_slots) }
}

unsafe extern "C" fn report_code_units(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { report_count_value(self_object, args, count, |report| report.code_units) }
}

unsafe extern "C" fn report_detail(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let (detail, detail_len) = {
        let report = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (report.detail, report.detail_len)
    };
    unsafe { (api.mkString)(detail.as_ptr(), detail_len) }
}

unsafe extern "C" fn collector_collect(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 5 {
        if let Some(collection) = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            collection.rejected = collection.rejected.saturating_add(1);
        }
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(_args, count) }) else {
        if let Some(collection) = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            collection.rejected = collection.rejected.saturating_add(1);
        }
        return api.nilObj;
    };
    let tx_object = arguments[3];
    let op_object = arguments[4];
    if unsafe { (api.isSmallInt)(tx_object) } == 0 || unsafe { (api.isSmallInt)(op_object) } == 0 {
        if let Some(collection) = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            collection.rejected = collection.rejected.saturating_add(1);
        }
        return api.nilObj;
    }
    let tx = unsafe { (api.smallIntValue)(tx_object) };
    let op = unsafe { (api.smallIntValue)(op_object) };
    if tx < 0 || !(0..=1).contains(&op) {
        if let Some(collection) = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            collection.rejected = collection.rejected.saturating_add(1);
        }
        return api.nilObj;
    }
    let mut collection = FACT_COLLECTION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(collection) = collection.as_mut() else {
        return api.nilObj;
    };
    let mut fields = FieldEncoder::new();
    let encoded = unsafe {
        encode_runtime_oop(api, &mut fields, arguments[0], &mut collection.heap)
            && encode_runtime_oop(api, &mut fields, arguments[1], &mut collection.heap)
            && encode_runtime_oop(api, &mut fields, arguments[2], &mut collection.heap)
    };
    if !encoded
        || collection
            .writer
            .fact_encoded(op as u8, tx as u64, fields.as_bytes())
            .is_err()
    {
        collection.rejected = collection.rejected.saturating_add(1);
    } else {
        collection.kept = collection.kept.saturating_add(1);
    }
    api.nilObj
}

unsafe fn bring_up(api: *mut IdApi) {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    let api_ref = unsafe { &*api };
    let report_vt = if unsafe { (api_ref.isExported)(IMAGE_REPORT_VT_NAME.as_ptr().cast()) } != 0 {
        unsafe { (api_ref.import)(IMAGE_REPORT_VT_NAME.as_ptr().cast()) }
    } else {
        let vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
        unsafe { (api_ref.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW) };
        vt
    };
    let report_obj = if unsafe { (api_ref.isExported)(IMAGE_REPORT_NAME.as_ptr().cast()) } != 0 {
        unsafe { (api_ref.import)(IMAGE_REPORT_NAME.as_ptr().cast()) }
    } else {
        let object = unsafe { (api_ref.alloc)(report_vt, 0) };
        unsafe {
            (api_ref.setOwner)(report_vt, object);
            let _ = (api_ref.exportObj)(IMAGE_REPORT_VT_NAME.as_ptr().cast(), report_vt);
            let _ = (api_ref.exportObj)(IMAGE_REPORT_NAME.as_ptr().cast(), object);
        }
        object
    };
    let collector_vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
    unsafe {
        (api_ref.method)(report_vt, intern(api_ref, b"status\0"), report_status);
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"savedFacts\0"),
            report_saved_facts,
        );
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"recoveredFacts\0"),
            report_recovered_facts,
        );
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"rejectedFacts\0"),
            report_rejected_facts,
        );
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"recoveredSlots\0"),
            report_recovered_slots,
        );
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"rejectedSlots\0"),
            report_rejected_slots,
        );
        (api_ref.method)(
            report_vt,
            intern(api_ref, b"codeUnits\0"),
            report_code_units,
        );
        (api_ref.method)(report_vt, intern(api_ref, b"detail\0"), report_detail);
        (api_ref.method)(
            collector_vt,
            intern(api_ref, b"e:a:v:tx:op:\0"),
            collector_collect,
        );
        let _ = (api_ref.exportObj)(COLLECTOR_VT_NAME.as_ptr().cast(), collector_vt);
        let system_vt = (api_ref.import)(c"SystemVt".as_ptr());
        if !system_vt.is_null() {
            (api_ref.method)(
                system_vt,
                intern(api_ref, b"saveImage:\0"),
                system_save_image,
            );
            (api_ref.method)(
                system_vt,
                intern(api_ref, b"imageSaveReport\0"),
                system_image_save_report,
            );
            (api_ref.method)(
                system_vt,
                intern(api_ref, b"loadImage:\0"),
                system_load_image,
            );
            (api_ref.method)(
                system_vt,
                intern(api_ref, b"loadImage:rebuild:\0"),
                system_load_image_rebuild,
            );
            (api_ref.method)(
                system_vt,
                intern(api_ref, b"imageCompositionIdentity\0"),
                system_image_composition_identity,
            );
            (api_ref.method)(system_vt, intern(api_ref, b"bakeExe:\0"), system_bake_exe);
        }
    }
    let _ = RUNTIME.set(Runtime {
        api: api as usize,
        report_obj: report_obj as usize,
        report: &SAVE_REPORT,
    });
}

fn reject_save(detail: &str) {
    let mut report = runtime()
        .report
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *report = SaveReport {
        status: ReportStatus::Rejected,
        detail_len: detail.len().min(256),
        ..SaveReport::default()
    };
    let detail_len = report.detail_len;
    report.detail[..detail_len].copy_from_slice(&detail.as_bytes()[..detail_len]);
}

/// Identifies the first dynamic Rust Image orchestration shell.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_image_rust_identity() -> *const c_char {
    RUST_IMAGE_IDENTITY.as_ptr().cast()
}

/// Initialize the Rust Image shell against the process floor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void) -> *mut c_void {
    let _ = state;
    if RUNTIME.get().is_none() {
        unsafe { bring_up(api) };
    }
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn header(architecture: u32, abi_hash: u64) -> Vec<u8> {
        let mut bytes = Vec::from(MAGIC.as_slice());
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&ENDIAN_TAG.to_le_bytes());
        bytes.extend_from_slice(&architecture.to_le_bytes());
        bytes.extend_from_slice(&abi_hash.to_le_bytes());
        bytes
    }

    fn record(kind: RecordKind, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(kind as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn header_and_records_are_borrowed() {
        let parsed = Header::parse(&header(1, 99))
            .unwrap()
            .require(1, 99)
            .unwrap();
        assert_eq!(parsed.architecture, 1);
        assert_eq!(parsed.abi_hash, 99);
        let mut bytes = record(RecordKind::Authority, b"digest");
        bytes.extend_from_slice(&record(RecordKind::Code, b"unit"));
        let mut records = Records::new(&bytes);
        assert_eq!(
            records.next_record().unwrap(),
            Some(Record {
                kind: RecordKind::Authority,
                payload: b"digest"
            })
        );
        assert_eq!(
            records.next_record().unwrap(),
            Some(Record {
                kind: RecordKind::Code,
                payload: b"unit"
            })
        );
        assert_eq!(records.next_record().unwrap(), None);
    }

    #[test]
    fn writer_emits_parser_compatible_v4_records() {
        let digest = [b'0'; AUTHORITY_DIGEST_LEN];
        let mut writer = ImageWriter::new(1, 99);
        writer.facts(b"facts").unwrap();
        writer.heap(b"heap").unwrap();
        writer.authority(&digest).unwrap();
        writer.code(wyhash(0, b"unit"), &digest, b"unit").unwrap();
        let bytes = writer.finish().unwrap();

        let image = Image::parse(&bytes).unwrap().header.require(1, 99).unwrap();
        assert_eq!(image.architecture, 1);
        assert_eq!(image.abi_hash, 99);
        let mut records = Image::parse(&bytes).unwrap().records();
        let mut kinds = Vec::new();
        while let Some(record) = records.next_record().unwrap() {
            kinds.push(record.kind);
        }
        assert_eq!(
            kinds,
            vec![
                RecordKind::Facts,
                RecordKind::Heap,
                RecordKind::Authority,
                RecordKind::Code
            ]
        );
    }

    #[test]
    fn field_encoder_round_trips_every_v4_tag() {
        let mut encoder = FieldEncoder::new();
        encoder.nil();
        encoder.true_value();
        encoder.false_value();
        encoder.integer(-7);
        encoder.float_bits(0x7ff8_0000_0000_0042);
        encoder.symbol(b"answer").unwrap();
        encoder.string(b"text").unwrap();
        encoder.heap_ref(23);
        encoder.null_oop();
        encoder.export_ref(b"System").unwrap();

        let mut fields = Fields::new(encoder.as_bytes());
        assert_eq!(fields.next_field().unwrap(), Some(Field::Nil));
        assert_eq!(fields.next_field().unwrap(), Some(Field::True));
        assert_eq!(fields.next_field().unwrap(), Some(Field::False));
        assert_eq!(fields.next_field().unwrap(), Some(Field::Integer(-7)));
        assert_eq!(
            fields.next_field().unwrap(),
            Some(Field::FloatBits(0x7ff8_0000_0000_0042))
        );
        assert_eq!(fields.next_field().unwrap(), Some(Field::Symbol(b"answer")));
        assert_eq!(fields.next_field().unwrap(), Some(Field::String(b"text")));
        assert_eq!(fields.next_field().unwrap(), Some(Field::HeapRef(23)));
        assert_eq!(fields.next_field().unwrap(), Some(Field::NullOop));
        assert_eq!(
            fields.next_field().unwrap(),
            Some(Field::ExportRef(b"System"))
        );
        assert_eq!(fields.next_field().unwrap(), None);
    }

    #[test]
    fn facts_writer_round_trips_atomic_facts() {
        let mut writer = FactsWriter::new(41);
        writer
            .fact(
                1,
                42,
                Field::Symbol(b"entity"),
                Field::Symbol(b"attribute"),
                Field::Integer(-9),
            )
            .unwrap();
        writer
            .fact(0, 43, Field::Nil, Field::True, Field::FloatBits(7))
            .unwrap();

        let payload = writer.finish();
        let mut facts = Facts::parse(&payload).unwrap();
        assert_eq!(facts.basis, 41);
        assert_eq!(
            facts.next_fact().unwrap(),
            Some(Fact {
                op: 1,
                tx: 42,
                entity: Field::Symbol(b"entity"),
                attribute: Field::Symbol(b"attribute"),
                value: Field::Integer(-9),
            })
        );
        assert_eq!(
            facts.next_fact().unwrap(),
            Some(Fact {
                op: 0,
                tx: 43,
                entity: Field::Nil,
                attribute: Field::True,
                value: Field::FloatBits(7),
            })
        );
        assert_eq!(facts.next_fact().unwrap(), None);
    }

    #[test]
    fn writer_rejects_authority_and_code_ordering_errors() {
        let digest = [b'0'; AUTHORITY_DIGEST_LEN];
        let other_digest = [b'1'; AUTHORITY_DIGEST_LEN];
        assert_eq!(
            ImageWriter::new(1, 99).finish(),
            Err(EncodeError::MissingAuthority)
        );

        let mut writer = ImageWriter::new(1, 99);
        assert_eq!(
            writer.code(7, &digest, b"unit"),
            Err(EncodeError::CodeBeforeAuthority)
        );
        writer.authority(&digest).unwrap();
        assert_eq!(
            writer.authority(&digest),
            Err(EncodeError::DuplicateAuthority)
        );
        assert_eq!(
            writer.code(7, &other_digest, b"unit"),
            Err(EncodeError::AuthorityMismatch)
        );
        writer.code(7, &digest, b"unit").unwrap();
        assert_eq!(
            writer.code(8, &digest, b"unit"),
            Err(EncodeError::DuplicateCode)
        );
    }

    #[test]
    fn publisher_replaces_destination_without_truncation_first() {
        let path = std::env::temp_dir().join(format!(
            "birnam-image-publish-{}-{}.image",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        fs::write(&path, b"old image").unwrap();
        publish_atomic(&path, b"new image").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new image");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn publisher_cleans_temporary_bytes_when_rename_fails() {
        let path = std::env::temp_dir().join(format!(
            "birnam-image-publish-directory-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        let temporary_prefix = format!(".{}", path.file_name().unwrap().to_string_lossy());
        let before = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&temporary_prefix)
            })
            .count();
        assert!(publish_atomic(&path, b"cannot replace a directory").is_err());
        let after = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&temporary_prefix)
            })
            .count();
        assert_eq!(after, before);
        assert!(path.is_dir());
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn header_rejects_bad_identity_and_truncation() {
        assert_eq!(Header::parse(&[]), Err(Error::Truncated));
        let mut bytes = header(1, 99);
        bytes[0] = b'X';
        assert_eq!(Header::parse(&bytes), Err(Error::MagicMismatch));
        assert_eq!(
            Header::parse(&header(1, 99)).unwrap().require(2, 99),
            Err(Error::ArchitectureMismatch)
        );
        assert_eq!(
            Header::parse(&header(1, 99)).unwrap().require(1, 100),
            Err(Error::AbiMismatch)
        );
    }

    #[test]
    fn records_fail_closed_on_unknown_kind_truncation_and_overflow() {
        let mut unknown = record(RecordKind::Heap, b"x");
        unknown[..4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            Records::new(&unknown).next_record(),
            Err(Error::InvalidRecordKind)
        );
        assert_eq!(
            Records::new(&[0; RECORD_HEADER_LEN - 1]).next_record(),
            Err(Error::Truncated)
        );
        let mut truncated = record(RecordKind::Heap, b"x");
        truncated[4..12].copy_from_slice(&2u64.to_le_bytes());
        assert_eq!(
            Records::new(&truncated).next_record(),
            Err(Error::Truncated)
        );
    }

    #[test]
    fn authority_and_code_payloads_are_borrowed_and_length_checked() {
        let digest = [b'd'; AUTHORITY_DIGEST_LEN];
        let authority = Authority::parse(&digest).unwrap();
        assert_eq!(authority.digest, &digest);
        assert_eq!(
            Authority::parse(&digest[..AUTHORITY_DIGEST_LEN - 1]),
            Err(Error::InvalidPayloadLength)
        );

        let mut payload = Vec::new();
        payload.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        payload.extend_from_slice(&digest);
        payload.extend_from_slice(b"dylib");
        let code = Code::parse(&payload).unwrap();
        assert_eq!(code.checksum, 0x0102_0304_0506_0708);
        assert_eq!(code.generation_digest, &digest);
        assert_eq!(code.unit, b"dylib");
        assert_eq!(
            Code::parse(&payload[..CODE_PREFIX_LEN - 1]),
            Err(Error::InvalidPayloadLength)
        );
    }

    #[test]
    fn code_checksum_matches_the_zig_wyhash_vectors() {
        assert_eq!(wyhash(0, b""), 0x0409_638e_e2bd_e459);
        assert_eq!(wyhash(1, b"a"), 0xa841_2d09_1b5f_e0a9);
        assert_eq!(wyhash(2, b"abc"), 0x32dd_92e4_b291_5153);
        assert_eq!(wyhash(3, b"message digest"), 0x8619_1240_89a3_a16b);
        assert_eq!(
            wyhash(4, b"abcdefghijklmnopqrstuvwxyz"),
            0x7a43_afb6_1d7f_5f40
        );
        assert_eq!(
            wyhash(
                5,
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
            ),
            0xff42_329b_90e5_0d58
        );
        assert_eq!(
            wyhash(
                6,
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            0xc39c_ab13_b115_aad3
        );
    }

    #[test]
    fn image_preflight_binds_code_to_one_authority() {
        let digest = [b'g'; AUTHORITY_DIGEST_LEN];
        let mut bytes = header(1, 99);
        bytes.extend_from_slice(&record(RecordKind::Authority, &digest));
        let image = Image::parse(&bytes).unwrap();
        assert_eq!(image.header.architecture, 1);
        assert_eq!(image.authority.digest, &digest);
        assert_eq!(image.code(), None);

        let mut code_payload = wyhash(0, b"unit").to_le_bytes().to_vec();
        code_payload.extend_from_slice(&digest);
        code_payload.extend_from_slice(b"unit");
        bytes.extend_from_slice(&record(RecordKind::Code, &code_payload));
        let image = Image::parse(&bytes).unwrap();
        assert_eq!(image.code().unwrap().unit, b"unit");
        assert_eq!(
            image.records().next_record().unwrap().unwrap().kind,
            RecordKind::Authority
        );
    }

    #[test]
    fn image_preflight_rejects_missing_duplicate_or_mismatched_authority() {
        let digest = [b'g'; AUTHORITY_DIGEST_LEN];
        let other = [b'h'; AUTHORITY_DIGEST_LEN];
        assert!(matches!(
            Image::parse(&header(1, 99)),
            Err(Error::MissingAuthority)
        ));

        let mut duplicate = header(1, 99);
        duplicate.extend_from_slice(&record(RecordKind::Authority, &digest));
        duplicate.extend_from_slice(&record(RecordKind::Authority, &digest));
        assert!(matches!(
            Image::parse(&duplicate),
            Err(Error::DuplicateAuthority)
        ));

        let mut before = header(1, 99);
        let mut code = wyhash(0, b"").to_le_bytes().to_vec();
        code.extend_from_slice(&digest);
        before.extend_from_slice(&record(RecordKind::Code, &code));
        assert!(matches!(
            Image::parse(&before),
            Err(Error::CodeBeforeAuthority)
        ));

        let mut mismatch = header(1, 99);
        mismatch.extend_from_slice(&record(RecordKind::Authority, &digest));
        let mut wrong_code = 0u64.to_le_bytes().to_vec();
        wrong_code.extend_from_slice(&other);
        mismatch.extend_from_slice(&record(RecordKind::Code, &wrong_code));
        assert!(matches!(
            Image::parse(&mismatch),
            Err(Error::AuthorityMismatch)
        ));

        let mut duplicate_code = header(1, 99);
        duplicate_code.extend_from_slice(&record(RecordKind::Authority, &digest));
        duplicate_code.extend_from_slice(&record(RecordKind::Code, &code));
        duplicate_code.extend_from_slice(&record(RecordKind::Code, &code));
        assert!(matches!(
            Image::parse(&duplicate_code),
            Err(Error::DuplicateCode)
        ));

        let mut bad_checksum = header(1, 99);
        bad_checksum.extend_from_slice(&record(RecordKind::Authority, &digest));
        let mut bad_code = 0u64.to_le_bytes().to_vec();
        bad_code.extend_from_slice(&digest);
        bad_code.extend_from_slice(b"unit");
        bad_checksum.extend_from_slice(&record(RecordKind::Code, &bad_code));
        assert!(matches!(
            Image::parse(&bad_checksum),
            Err(Error::CodeChecksumMismatch)
        ));
    }

    #[test]
    fn fields_decode_without_copying_or_materializing_values() {
        let mut bytes = vec![
            FieldTag::Nil as u8,
            FieldTag::True as u8,
            FieldTag::False as u8,
            FieldTag::NullOop as u8,
            FieldTag::Integer as u8,
        ];
        bytes.extend_from_slice(&(-41i64).to_le_bytes());
        bytes.push(FieldTag::FloatBits as u8);
        bytes.extend_from_slice(&0x8000_0000_0000_0000u64.to_le_bytes());
        bytes.push(FieldTag::HeapRef as u8);
        bytes.extend_from_slice(&17u32.to_le_bytes());
        for (tag, value) in [
            (FieldTag::Symbol, b"selector".as_slice()),
            (FieldTag::String, b"text".as_slice()),
            (FieldTag::ExportRef, b"StringVt".as_slice()),
        ] {
            bytes.push(tag as u8);
            bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
            bytes.extend_from_slice(value);
        }

        let mut fields = Fields::new(&bytes);
        assert_eq!(fields.next_field().unwrap(), Some(Field::Nil));
        assert_eq!(fields.next_field().unwrap(), Some(Field::True));
        assert_eq!(fields.next_field().unwrap(), Some(Field::False));
        assert_eq!(fields.next_field().unwrap(), Some(Field::NullOop));
        assert_eq!(fields.next_field().unwrap(), Some(Field::Integer(-41)));
        assert_eq!(
            fields.next_field().unwrap(),
            Some(Field::FloatBits(0x8000_0000_0000_0000))
        );
        assert_eq!(fields.next_field().unwrap(), Some(Field::HeapRef(17)));
        assert_eq!(
            fields.next_field().unwrap(),
            Some(Field::Symbol(b"selector"))
        );
        assert_eq!(fields.next_field().unwrap(), Some(Field::String(b"text")));
        assert_eq!(
            fields.next_field().unwrap(),
            Some(Field::ExportRef(b"StringVt"))
        );
        assert_eq!(fields.next_field().unwrap(), None);
    }

    #[test]
    fn fields_fail_closed_on_unknown_tags_and_truncated_lengths() {
        assert_eq!(
            Fields::new(&[0xff]).next_field(),
            Err(Error::InvalidFieldTag)
        );
        assert_eq!(
            Fields::new(&[FieldTag::Integer as u8, 0, 0]).next_field(),
            Err(Error::InvalidPayloadLength)
        );
        assert_eq!(
            Fields::new(&[FieldTag::String as u8, 4, 0, 0, 0, b'x']).next_field(),
            Err(Error::InvalidPayloadLength)
        );
    }

    #[test]
    fn facts_iterate_basis_and_tagged_values_without_copying() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&73u64.to_le_bytes());
        payload.extend_from_slice(&2u32.to_le_bytes());
        for (op, tx, fields) in [
            (
                1u8,
                74u64,
                vec![
                    (FieldTag::HeapRef, 17u32.to_le_bytes().to_vec()),
                    (FieldTag::Symbol, b"name".to_vec()),
                    (FieldTag::NullOop, Vec::new()),
                ],
            ),
            (
                2u8,
                75u64,
                vec![
                    (FieldTag::Integer, (-9i64).to_le_bytes().to_vec()),
                    (FieldTag::String, b"value".to_vec()),
                    (FieldTag::ExportRef, b"StringVt".to_vec()),
                ],
            ),
        ] {
            payload.push(op);
            payload.extend_from_slice(&tx.to_le_bytes());
            for (tag, value) in fields {
                payload.push(tag as u8);
                match tag {
                    FieldTag::Symbol | FieldTag::String | FieldTag::ExportRef => {
                        payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
                    }
                    _ => {}
                }
                payload.extend_from_slice(&value);
            }
        }

        let mut facts = Facts::parse(&payload).unwrap();
        assert_eq!(facts.basis, 73);
        assert_eq!(
            facts.next_fact().unwrap(),
            Some(Fact {
                op: 1,
                tx: 74,
                entity: Field::HeapRef(17),
                attribute: Field::Symbol(b"name"),
                value: Field::NullOop,
            })
        );
        assert_eq!(
            facts.next_fact().unwrap(),
            Some(Fact {
                op: 2,
                tx: 75,
                entity: Field::Integer(-9),
                attribute: Field::String(b"value"),
                value: Field::ExportRef(b"StringVt"),
            })
        );
        assert_eq!(facts.next_fact().unwrap(), None);
    }

    #[test]
    fn facts_reject_truncated_counts_and_trailing_bytes() {
        assert!(matches!(
            Facts::parse(&[0; 11]),
            Err(Error::InvalidPayloadLength)
        ));

        let mut count = vec![0; 12];
        count[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            Facts::parse(&count).unwrap().next_fact(),
            Err(Error::InvalidPayloadLength)
        );

        let mut trailing = vec![0; 12];
        trailing.extend_from_slice(&[FieldTag::Nil as u8]);
        assert_eq!(
            Facts::parse(&trailing).unwrap().next_fact(),
            Err(Error::InvalidPayloadLength)
        );
    }

    #[test]
    fn heap_iterates_layout_driven_borrowed_objects() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes());

        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(b"Foo");
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        payload.push(FieldTag::String as u8);
        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(b"abc");

        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(b"Bar");
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.push(FieldTag::Nil as u8);

        let mut heap = Heap::parse(&payload).unwrap();
        assert_eq!(
            heap.peek_header().unwrap(),
            Some(HeapHeader {
                vtable_name: b"Foo",
                word_count: 2
            })
        );
        let first = heap
            .next_object(&[SlotKind::RawWord, SlotKind::Oop])
            .unwrap()
            .unwrap();
        assert_eq!(first.header.vtable_name, b"Foo");
        assert_eq!(first.header.word_count, 2);
        assert_eq!(
            first.encoded_slots,
            &[
                0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 6, 3, 0, 0, 0, b'a', b'b', b'c'
            ]
        );
        let second = heap
            .next_object(&[SlotKind::Code, SlotKind::Oop])
            .unwrap()
            .unwrap();
        assert_eq!(second.header.vtable_name, b"Bar");
        assert_eq!(second.encoded_slots, &[FieldTag::Nil as u8]);
        assert_eq!(heap.peek_header().unwrap(), None);
    }

    #[test]
    fn heap_rejects_layout_mismatch_truncation_and_trailing_bytes() {
        let mut mismatch = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
        assert_eq!(
            Heap::parse(&mismatch).unwrap().next_object(&[]),
            Err(Error::LayoutMismatch)
        );

        mismatch[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            Heap::parse(&mismatch)
                .unwrap()
                .next_object(&[SlotKind::RawWord]),
            Err(Error::InvalidPayloadLength)
        );

        let trailing = [0, 0, 0, 0, 0xff];
        assert_eq!(
            Heap::parse(&trailing).unwrap().peek_header(),
            Err(Error::InvalidPayloadLength)
        );
    }
}
