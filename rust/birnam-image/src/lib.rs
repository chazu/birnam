//! Borrowed framing primitives and a checked writer for the portable Birnam v4 image.
//!
//! This format-only crate owns no `IdApi` values and performs no allocation. It
//! validates the v4 image header, authority/code relationship, borrowed field
//! envelopes, facts, and layout-driven heap records without materializing any
//! runtime state. The writer emits only framing and caller-owned payloads;
//! higher layers remain responsible for name resolution, swizzling, loading,
//! heap walking, and Store mutation.

#![allow(clippy::missing_safety_doc)]

pub mod v5;
pub mod v6;
pub mod v7;

/// Provider-free image framing and codec surface.
///
/// The dynamic Image shell below is intentionally kept outside this module so
/// Codegen and other pure consumers can depend on the codec without pulling a
/// provider initializer into their dylib.
pub mod codec {

    pub const MAGIC: &[u8; 8] = b"BIRNAMIM";
    pub const FORMAT_VERSION: u32 = 4;
    pub const ENDIAN_TAG: u32 = 0x0102_0304;
    pub const HEADER_LEN: usize = 28;
    pub const RECORD_HEADER_LEN: usize = 12;
    pub const AUTHORITY_DIGEST_LEN: usize = 64;
    pub const CODE_PREFIX_LEN: usize = 8 + AUTHORITY_DIGEST_LEN;
    pub const FIELD_LENGTH_PREFIX: usize = 4;

    pub const IMAGE_ARCH_TAG: u32 = if cfg!(target_arch = "aarch64") {
        1
    } else if cfg!(target_arch = "x86_64") {
        2
    } else {
        0
    };
    const RUNTIME_ABI_VERSION: u32 = 3;
    const RUNTIME_SHIM_NAMES: &[&[u8]] = &[
        b"birnam_send",
        b"birnam_intern",
        b"birnam_mkSmallInt",
        b"birnam_smallIntValue",
        b"birnam_mkFloat",
        b"birnam_floatValue",
        b"birnam_mkString",
        b"birnam_nil",
        b"birnam_true",
        b"birnam_false",
        b"birnam_objectVt",
        b"birnam_proto",
        b"birnam_method",
        b"birnam_alloc",
        b"birnam_cstr",
        b"birnam_free_cstr",
        b"birnam_layoutAllOops",
        b"birnam_global",
        b"birnam_candidate_validate_binding",
    ];
    const REGISTRATION_SHIM_NAME: &[u8] = b"birnam_register_resumable_v1";
    const REGISTRATION_ABI_FIELDS_V1: &[(&[u8], u32, u32, u32, &[u8], &[u8])] = &[
        (b"carrier_version", 0x00, 4, 4, b"u32", b"not-applicable"),
        (b"struct_size", 0x04, 4, 4, b"u32", b"not-applicable"),
        (b"operation", 0x08, 4, 4, b"u32", b"not-applicable"),
        (b"flags", 0x0c, 4, 4, b"u32", b"not-applicable"),
        (
            b"manifest_entry_index",
            0x10,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (b"reserved0", 0x14, 4, 4, b"u32", b"not-applicable"),
        (b"registration_token", 0x18, 8, 8, b"u64", b"not-applicable"),
        (
            b"descriptor_bytes",
            0x20,
            8,
            8,
            b"pointer<const-u8>",
            b"prepare-nonnull-abort-null",
        ),
        (b"descriptor_length", 0x28, 8, 8, b"u64", b"not-applicable"),
        (
            b"entry_thunk",
            0x30,
            8,
            8,
            b"function-pointer<BirnamMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"step_method",
            0x38,
            8,
            8,
            b"function-pointer<BirnamStepMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"implementation_oop",
            0x40,
            8,
            8,
            b"BirnamOopV1",
            b"prepare-named-nonnull-block-null-abort-null",
        ),
        (
            b"code_generation_identity",
            0x48,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"code_generation_lease",
            0x50,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"portable_implementation_identity",
            0x58,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (b"language_arity", 0x60, 4, 4, b"u32", b"not-applicable"),
        (
            b"driver_argument_count",
            0x64,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (
            b"descriptor_sha256",
            0x68,
            32,
            1,
            b"bytes[32]",
            b"not-applicable",
        ),
    ];
    const REGISTRATION_ABI_CONSTANTS_V1: &[(&[u8], u32)] = &[
        (b"BIRNAM_REGISTRATION_CARRIER_VERSION_V1", 1),
        (b"BIRNAM_REGISTRATION_PREPARE_V1", 1),
        (b"BIRNAM_REGISTRATION_ABORT_V1", 2),
        (b"BIRNAM_REGISTRATION_OK_V1", 0),
        (b"BIRNAM_REGISTRATION_REJECTED_V1", 1),
    ];

    pub(crate) fn hash_registration_abi_tuple_v1(mut hash: u64) -> (u64, usize) {
        fn bytes(hash: &mut u64, count: &mut usize, value: &[u8]) {
            for byte in value {
                *hash ^= u64::from(*byte);
                *hash = hash.wrapping_mul(0x100000001b3);
            }
            *count += value.len();
        }

        fn u32_le(hash: &mut u64, count: &mut usize, value: u32) {
            bytes(hash, count, &value.to_le_bytes());
        }

        fn string(hash: &mut u64, count: &mut usize, value: &[u8]) {
            u32_le(hash, count, value.len() as u32);
            bytes(hash, count, value);
        }

        let mut count = 0;
        bytes(&mut hash, &mut count, b"birnam.registration-envelope.v1\0");
        string(
            &mut hash,
            &mut count,
            b"aarch64-apple-darwin|Mach-O|little-endian",
        );
        string(&mut hash, &mut count, b"birnam_register_resumable_v1");
        string(
            &mut hash,
            &mut count,
            b"u32(BirnamResumableRegistrationV1 *)",
        );
        for value in [136, 8, REGISTRATION_ABI_FIELDS_V1.len() as u32] {
            u32_le(&mut hash, &mut count, value);
        }
        for &(name, offset, width, alignment, abi_kind, nullability) in REGISTRATION_ABI_FIELDS_V1 {
            string(&mut hash, &mut count, name);
            for value in [offset, width, alignment] {
                u32_le(&mut hash, &mut count, value);
            }
            string(&mut hash, &mut count, abi_kind);
            string(&mut hash, &mut count, nullability);
        }
        u32_le(
            &mut hash,
            &mut count,
            REGISTRATION_ABI_CONSTANTS_V1.len() as u32,
        );
        for &(name, value) in REGISTRATION_ABI_CONSTANTS_V1 {
            string(&mut hash, &mut count, name);
            u32_le(&mut hash, &mut count, value);
        }
        (hash, count)
    }

    pub fn runtime_abi_hash() -> u64 {
        let mut hash = 0xcbf29ce484222325;
        for name in RUNTIME_SHIM_NAMES {
            for byte in *name {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        for byte in REGISTRATION_SHIM_NAME {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash = hash_registration_abi_tuple_v1(hash).0;
        let mut version = RUNTIME_ABI_VERSION;
        for _ in 0..4 {
            hash ^= u64::from((version & 0xff) as u8);
            hash = hash.wrapping_mul(0x100000001b3);
            version >>= 8;
        }
        hash
    }

    #[cfg(any(feature = "module", test))]
    pub(crate) fn authority_digest_from_bytes(bytes: &[u8]) -> Option<[u8; AUTHORITY_DIGEST_LEN]> {
        bytes.try_into().ok()
    }

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

        /// Perform the complete read-only admission required before a higher
        /// layer may consider loading this image. No runtime objects are resolved
        /// or mutated; mapping, source rebuild, and Store replay remain outside
        /// the codec boundary.
        pub fn preflight(bytes: &'a [u8], architecture: u32, abi_hash: u64) -> Result<Self, Error> {
            let image = Self::parse(bytes)?;
            image.header.require(architecture, abi_hash)?;
            image.validate_facts()?;
            Ok(image)
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

        pub fn next_object(
            &mut self,
            layout: &[SlotKind],
        ) -> Result<Option<HeapObject<'a>>, Error> {
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

        #[cfg(feature = "module")]
        pub(crate) fn resumable_ref_v5(&mut self, index: u32) {
            crate::v5::encode_resumable_ref(&mut self.bytes, index);
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

    pub(crate) static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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

        pub fn authority(
            &mut self,
            digest: &[u8; AUTHORITY_DIGEST_LEN],
        ) -> Result<(), EncodeError> {
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

    /// Compose a complete v4 image from already-preflighted payloads and explicit
    /// authority/code inputs. This pure seam owns framing only: callers remain
    /// responsible for deriving the generation digest, validating the code unit,
    /// and deciding whether the returned bytes may be published.
    pub fn compose_complete_image(
        architecture: u32,
        abi_hash: u64,
        facts: &[u8],
        heap: &[u8],
        authority_digest: &[u8; AUTHORITY_DIGEST_LEN],
        code_checksum: u64,
        code_unit: &[u8],
    ) -> Result<Vec<u8>, EncodeError> {
        let mut writer = ImageWriter::new(architecture, abi_hash);
        writer.authority(authority_digest)?;
        writer.heap(heap)?;
        writer.facts(facts)?;
        writer.code(code_checksum, authority_digest, code_unit)?;
        writer.finish()
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

    pub(crate) fn wyhash(seed: u64, input: &[u8]) -> u64 {
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
}

#[cfg(any(feature = "module", test))]
pub(crate) use codec::authority_digest_from_bytes;
#[allow(unused_imports)]
pub use codec::*;

#[cfg(feature = "module")]
mod runtime {
    use super::*;
    use birnam_abi::{
        IdApi, MarkFn, Oop, PORTABLE_CODE_API_V2_VERSION, PORTABLE_CODE_API_V3_VERSION,
        PORTABLE_PROGRAM_VIEW_API_V1_VERSION, PortableCodeApiV2, PortableCodeApiV3,
        PortableProgramViewApiV1, borrowed_oop_args,
    };
    use core::ffi::{c_char, c_int, c_void};
    use core::mem::size_of;
    use core::ptr;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::ffi::{CStr, OsStr};
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    const RUST_IMAGE_IDENTITY: &[u8] = b"rust-image-r7-portable\0";
    const IMAGE_REPORT_VT_NAME: &[u8] = b"ImageSaveReportVt\0";
    const IMAGE_REPORT_NAME: &[u8] = b"ImageSaveReport\0";
    const COLLECTOR_VT_NAME: &[u8] = b"ImageCollectorVt\0";
    const MAX_IMAGE_BYTES: u64 = 1 << 30;
    const RUST_CODEGEN_NAME: &[u8] = b"RustCodegen\0";
    const RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION: &[u8] = b"initializeCurrentGeneration\0";
    const RUST_CODEGEN_PREFLIGHT_IMAGE: &[u8] = b"preflightImage:\0";
    const RUST_CODEGEN_BEGIN_IMAGE_RECOVERY: &[u8] = b"beginImageDispatchRecovery\0";
    const RUST_CODEGEN_END_IMAGE_RECOVERY: &[u8] = b"endImageDispatchRecovery\0";
    const RUST_CODEGEN_REPLAY_IMAGE_FACTS: &[u8] = b"replayImageFacts:\0";
    const RUST_CODEGEN_REPLAY_STALE_IMAGE_FACTS: &[u8] = b"replayStaleImageFacts:\0";
    const RUST_CODEGEN_DERIVE_IMAGE_METHOD_TARGETS: &[u8] = b"deriveImageMethodTargets\0";
    const RUST_CODEGEN_MAP_IMAGE_CODE: &[u8] = b"mapImageCode:\0";
    const COMPILE_PORTABLE_IMAGE_CODE: &[u8] = b"compilePortableImageCode:bindings:selection:\0";
    const BEGIN_PORTABLE_IMAGE_REBUILD: &[u8] = b"beginPortableImageRebuild:bindings:selection:\0";
    const BEGIN_PORTABLE_IMAGE_MAPPED: &[u8] =
        b"beginPortableImageMapped:programs:bindings:selection:\0";
    const PORTABLE_IMAGE_BLOCK_VTABLE: &[u8] = b"portableImageBlockVtable:\0";
    const PREPARE_PORTABLE_IMAGE_BINDING: &[u8] = b"preparePortableImageBinding:implementation:\0";
    const FINISH_PORTABLE_IMAGE_UNIT: &[u8] = b"finishPortableImageUnit\0";
    const CANCEL_PORTABLE_IMAGE_UNIT: &[u8] = b"cancelPortableImageUnit\0";
    const INSTALL_PORTABLE_IMAGE_BINDING: &[u8] = b"installPortableImageBinding:on:as:source:\0";
    const MACHINE_NAME: &[u8] = b"Machine\0";
    const FRAME_VIEW_NAME: &[u8] = b"FrameView\0";
    const CONTINUATION_NAME: &[u8] = b"Continuation\0";
    const CONTINUE_ACTION_NAME: &[u8] = b"ContinueAction\0";
    const CALL_ACTION_NAME: &[u8] = b"CallAction\0";
    const TAIL_CALL_ACTION_NAME: &[u8] = b"TailCallAction\0";
    const RETURN_ACTION_NAME: &[u8] = b"ReturnAction\0";
    const SUSPEND_ACTION_NAME: &[u8] = b"SuspendAction\0";
    const CAPTURE_ACTION_NAME: &[u8] = b"CaptureAction\0";
    const RESTORE_ACTION_NAME: &[u8] = b"RestoreAction\0";
    const ABORT_ACTION_NAME: &[u8] = b"AbortAction\0";
    const DISCARD_RECEIPT_NAME: &[u8] = b"DiscardReceipt\0";
    const PORTABLE_BLOCK_CARRIER_VTABLE_NAME: &[u8] = b"_BirnamPortableBlockCarrierVt";
    const PORTABLE_PROGRAM_VIEW_API_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1\0";
    const PORTABLE_PROGRAM_VIEW_API_VTABLE_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1Vt\0";
    const PORTABLE_CODE_API_V3_NAME: &[u8] = b"_BirnamPortableCodeApiV3\0";
    const PORTABLE_CODE_API_V3_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV3Vt\0";
    const PORTABLE_CODE_API_V2_NAME: &[u8] = b"_BirnamPortableCodeApiV2\0";
    const PORTABLE_CODE_API_V2_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV2Vt\0";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ImageRejectionKind {
        Machine,
        StepAction,
        FrameView,
        Continuation,
        DiscardReceipt,
    }

    impl ImageRejectionKind {
        fn symbol(self) -> &'static [u8] {
            match self {
                Self::Machine => b"machineNotImageable\0",
                Self::StepAction => b"stepActionNotImageable\0",
                Self::FrameView => b"frameViewNotImageable\0",
                Self::Continuation => b"continuationNotImageable\0",
                Self::DiscardReceipt => b"discardReceiptNotImageable\0",
            }
        }

        fn detail(self) -> &'static str {
            match self {
                Self::Machine => "Rust Image rejected process-local Machine",
                Self::StepAction => "Rust Image rejected process-local StepAction",
                Self::FrameView => "Rust Image rejected process-local FrameView",
                Self::Continuation => "Rust Image rejected process-local Continuation",
                Self::DiscardReceipt => "Rust Image rejected process-local DiscardReceipt",
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Runtime {
        api: usize,
        report_obj: usize,
        report: &'static Mutex<SaveReport>,
    }

    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
    unsafe extern "C" fn mark_runtime_roots(mark: MarkFn) {
        if let Some(runtime) = RUNTIME.get() {
            unsafe { mark(runtime.report_obj as Oop) };
        }
    }

    struct FactCollection {
        writer: v6::FactsWriter,
        heap: HeapPreflight,
        kept: u32,
        recovered: u32,
        recoveries_needing_code: u32,
        rejected: u32,
    }

    struct FactPreflight {
        kept: u32,
        recovered: u32,
        rejected: u32,
        facts_payload: Vec<u8>,
        heap_payload: Option<Vec<u8>>,
        heap_ok: bool,
        recovered_slots: u32,
        rejected_slots: u32,
        writer_ok: bool,
        image_rejection: Option<ImageRejectionKind>,
    }

    static FACT_COLLECTION: Mutex<Option<FactCollection>> = Mutex::new(None);
    struct PortableSaveRecords {
        programs: Vec<u8>,
        bindings: Vec<u8>,
        selection: Vec<u8>,
    }
    static PORTABLE_SAVE_RECORDS: Mutex<Option<PortableSaveRecords>> = Mutex::new(None);
    static IMAGE_COMPOSITION_IDENTITY: Mutex<Option<[u8; 64]>> = Mutex::new(None);

    #[repr(C)]
    struct DlInfo {
        filename: *const c_char,
        base: *mut c_void,
        symbol_name: *const c_char,
        symbol_address: *mut c_void,
    }

    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }

    unsafe fn floor_symbol(api: &IdApi, name: &[u8]) -> Option<usize> {
        if name.last() != Some(&0) {
            return None;
        }
        static FLOOR_HANDLE: OnceLock<usize> = OnceLock::new();
        let handle = *FLOOR_HANDLE.get_or_init(|| {
            let mut info = DlInfo {
                filename: ptr::null(),
                base: ptr::null_mut(),
                symbol_name: ptr::null(),
                symbol_address: ptr::null_mut(),
            };
            // The host loads the selected floor RTLD_LOCAL. Resolve private
            // image seams from that exact image, never from global symbol order.
            if unsafe { dladdr(api.dlsym as *const () as *const c_void, &mut info) } == 0
                || info.filename.is_null()
            {
                return 0;
            }
            #[cfg(target_os = "macos")]
            const RTLD_NOW_LOCAL: c_int = 2 | 4;
            #[cfg(not(target_os = "macos"))]
            const RTLD_NOW_LOCAL: c_int = 2;
            unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) as usize }
        });
        if handle == 0 {
            return None;
        }
        let address = unsafe { (api.dlsym)(handle as *mut c_void, name.as_ptr().cast()) };
        (!address.is_null()).then_some(address as usize)
    }

    struct HeapPreflight {
        authority: Option<[u8; AUTHORITY_DIGEST_LEN]>,
        objects: Vec<usize>,
        relocations: HashMap<usize, u32>,
        portable_programs: Vec<(usize, Vec<u8>)>,
        recovered_slots: u32,
        rejected_slots: u32,
        image_rejection: Option<ImageRejectionKind>,
    }

    #[derive(Clone, Copy)]
    struct PortableBindingSnapshot {
        target: Oop,
        selector: Oop,
        implementation: Oop,
        provenance: Oop,
    }

    enum OwnedBindingField {
        Nil,
        True,
        False,
        Integer(i64),
        Symbol(Vec<u8>),
        String(Vec<u8>),
        HeapRef(u32),
        ExportRef(Vec<u8>),
    }

    impl OwnedBindingField {
        fn borrowed(&self) -> birnam_artifact::code_binding_inventory::Field<'_> {
            use birnam_artifact::code_binding_inventory::Field;
            match self {
                Self::Nil => Field::Nil,
                Self::True => Field::True,
                Self::False => Field::False,
                Self::Integer(value) => Field::Integer(*value),
                Self::Symbol(bytes) => Field::Symbol(bytes),
                Self::String(bytes) => Field::String(bytes),
                Self::HeapRef(index) => Field::HeapRef(*index),
                Self::ExportRef(bytes) => Field::ExportRef(bytes),
            }
        }
    }

    struct OwnedBinding {
        target: OwnedBindingField,
        selector: Vec<u8>,
        implementation: OwnedBindingField,
        provenance: OwnedBindingField,
    }

    unsafe fn vector_values(api: &IdApi, vector: Oop) -> Option<Vec<Oop>> {
        let size = unsafe { send0(api, vector, b"size\0") };
        if unsafe { (api.isSmallInt)(size) } == 0 {
            return None;
        }
        let size = usize::try_from(unsafe { (api.smallIntValue)(size) }).ok()?;
        if size > 1_048_576 {
            return None;
        }
        (0..size)
            .map(|index| {
                let index = i64::try_from(index).ok()?;
                Some(unsafe { send1(api, vector, b"at:\0", (api.mkSmallInt)(index)) })
            })
            .collect()
    }

    unsafe fn portable_binding_snapshot(api: &IdApi) -> Option<Vec<PortableBindingSnapshot>> {
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let rows = unsafe { send0(api, codegen, b"portableImageBindings\0") };
        let rows = unsafe { vector_values(api, rows) }?;
        if rows.len() > 262_144 {
            return None;
        }
        rows.into_iter()
            .map(|row| {
                let fields = unsafe { vector_values(api, row) }?;
                (fields.len() == 4).then_some(PortableBindingSnapshot {
                    target: fields[0],
                    selector: fields[1],
                    implementation: fields[2],
                    provenance: fields[3],
                })
            })
            .collect()
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

    unsafe fn vtable_inherits_from(api: &IdApi, mut vtable: Oop, ancestor: Oop) -> bool {
        let mut remaining = unsafe { (api.vtableCount)() }.saturating_add(1);
        while !vtable.is_null() && remaining != 0 {
            if vtable == ancestor {
                return true;
            }
            vtable = unsafe { (api.vtableParent)(vtable) };
            remaining -= 1;
        }
        false
    }

    unsafe fn belongs_to_exported_capability_family(
        api: &IdApi,
        object: Oop,
        prototype_name: &[u8],
    ) -> bool {
        let prototype = unsafe { (api.import)(prototype_name.as_ptr().cast()) };
        if prototype.is_null() || prototype == api.nilObj || object == prototype {
            return false;
        }
        let family_vtable = unsafe { (api.vtableOf)(prototype) };
        let object_vtable = unsafe { (api.vtableOf)(object) };
        !family_vtable.is_null()
            && !object_vtable.is_null()
            && unsafe { vtable_inherits_from(api, object_vtable, family_vtable) }
    }

    unsafe fn image_rejection_kind(api: &IdApi, object: Oop) -> Option<ImageRejectionKind> {
        if unsafe { belongs_to_exported_capability_family(api, object, MACHINE_NAME) } {
            return Some(ImageRejectionKind::Machine);
        }
        if [
            CONTINUE_ACTION_NAME,
            CALL_ACTION_NAME,
            TAIL_CALL_ACTION_NAME,
            RETURN_ACTION_NAME,
            SUSPEND_ACTION_NAME,
            CAPTURE_ACTION_NAME,
            RESTORE_ACTION_NAME,
            ABORT_ACTION_NAME,
        ]
        .into_iter()
        .any(|name| unsafe { belongs_to_exported_capability_family(api, object, name) })
        {
            return Some(ImageRejectionKind::StepAction);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, FRAME_VIEW_NAME) } {
            return Some(ImageRejectionKind::FrameView);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, CONTINUATION_NAME) } {
            return Some(ImageRejectionKind::Continuation);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, DISCARD_RECEIPT_NAME) } {
            return Some(ImageRejectionKind::DiscardReceipt);
        }
        None
    }

    impl HeapPreflight {
        fn new(authority: Option<[u8; AUTHORITY_DIGEST_LEN]>) -> Self {
            Self {
                authority,
                objects: Vec::new(),
                relocations: HashMap::new(),
                portable_programs: Vec::new(),
                recovered_slots: 0,
                rejected_slots: 0,
                image_rejection: None,
            }
        }

        unsafe fn intern(&mut self, api: &IdApi, object: Oop) -> Option<u32> {
            if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
                self.image_rejection.get_or_insert(kind);
                return None;
            }
            let key = object as usize;
            if let Some(id) = self.relocations.get(&key) {
                return Some(*id);
            }
            let vtable = unsafe { (api.vtableOf)(object) };
            if vtable.is_null() {
                return None;
            }
            let portable_program = unsafe { copy_code_object_program(api, object) };
            let portable_block = unsafe { portable_block_code_object(api, vtable) };
            let has_exported_vtable =
                unsafe { c_string_bytes((api.exportedNameOf)(vtable)) }.is_some();
            let has_resumable_vtable =
                unsafe { self.block_vtable_image_reference(api, vtable).is_some() };
            let owned_parent = unsafe { (api.vtableOwner)(vtable) } == object;
            let parent = unsafe { (api.vtableParent)(vtable) };
            let has_exported_parent = owned_parent
                && !parent.is_null()
                && unsafe { c_string_bytes((api.exportedNameOf)(parent)) }.is_some();
            if !has_exported_vtable
                && !has_resumable_vtable
                && !has_exported_parent
                && portable_block.is_none()
            {
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
            if let Some(program) = portable_program {
                self.portable_programs.push((key, program));
                let provenance = unsafe { send0(api, object, b"sourceProvenance\0") };
                if !unsafe { runtime_oop_is_inline(api, provenance) }
                    && unsafe { c_string_bytes((api.exportedNameOf)(provenance)) }.is_none()
                {
                    unsafe { self.intern(api, provenance) }?;
                }
                return Some(id);
            }
            for index in 0..words {
                if unsafe { (api.slotKind)(vtable, index) } != birnam_abi::SLOT_OOP {
                    continue;
                }
                let address =
                    (object as *const u8).wrapping_add(index * core::mem::size_of::<usize>());
                // SAFETY: the layout marks this initialized payload word as an Oop.
                let child = unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
                if unsafe { runtime_oop_is_inline(api, child) }
                    || unsafe { self.block_vtable_image_reference(api, child).is_some() }
                {
                    continue;
                }
                unsafe { self.intern(api, child) }?;
            }
            Some(id)
        }

        unsafe fn finish_v7(&mut self, api: &IdApi) -> Option<(Vec<u8>, Vec<u8>)> {
            let roots = self
                .portable_programs
                .iter()
                .map(|(_, program)| program.as_slice())
                .collect::<Vec<_>>();
            let programs = birnam_artifact::portable_inventory::encode_closure(&roots).ok()?;
            let inventory = birnam_artifact::portable_inventory::inspect(&programs).ok()?;
            let mut payload = Vec::new();
            payload.extend_from_slice(&u32::try_from(self.objects.len()).ok()?.to_le_bytes());
            for object_index in 0..self.objects.len() {
                let object = self.objects[object_index] as Oop;
                if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
                    self.image_rejection.get_or_insert(kind);
                    return None;
                }
                let vtable = unsafe { (api.vtableOf)(object) };
                if let Some(program) = self
                    .portable_programs
                    .iter()
                    .find(|(candidate, _)| *candidate == object as usize)
                    .map(|(_, program)| program)
                {
                    encode_v7_name_field(&mut payload, 9, b"CodeObjectVt")?;
                    payload.extend_from_slice(&2u32.to_le_bytes());
                    let program_index = (0..inventory.program_count as usize)
                        .find(|index| inventory.program(*index) == Some(program.as_slice()))?;
                    payload.push(11);
                    payload.extend_from_slice(&u32::try_from(program_index).ok()?.to_le_bytes());
                    let provenance = unsafe { send0(api, object, b"sourceProvenance\0") };
                    unsafe { encode_v7_oop(api, &mut payload, provenance, self) }?;
                    continue;
                }
                let portable_code_object = unsafe { portable_block_code_object(api, vtable) };
                let layout_vtable = if portable_code_object.is_some()
                    || unsafe { c_string_bytes((api.exportedNameOf)(vtable)) }.is_some()
                {
                    vtable
                } else {
                    let parent = unsafe { (api.vtableParent)(vtable) };
                    unsafe { c_string_bytes((api.exportedNameOf)(parent)) }?;
                    parent
                };
                if let Some(code_object) = portable_code_object {
                    let code_object = *self.relocations.get(&(code_object as usize))?;
                    payload.push(12);
                    payload.extend_from_slice(&code_object.to_le_bytes());
                } else {
                    let name = unsafe { c_string_bytes((api.exportedNameOf)(layout_vtable)) }?;
                    encode_v7_name_field(&mut payload, 9, &name)?;
                }
                let payload_size = unsafe { (api.objectPayloadSize)(object) };
                let words = payload_size / size_of::<usize>();
                payload.extend_from_slice(&u32::try_from(words).ok()?.to_le_bytes());
                for index in 0..words {
                    let kind = unsafe { (api.slotKind)(layout_vtable, index) };
                    if kind == birnam_abi::SLOT_CODE {
                        self.rejected_slots = self.rejected_slots.saturating_add(1);
                        return None;
                    }
                    let address = (object as *const u8).wrapping_add(index * size_of::<usize>());
                    if kind == birnam_abi::SLOT_OOP {
                        let child =
                            unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
                        unsafe { encode_v7_oop(api, &mut payload, child, self) }?;
                    } else if kind == birnam_abi::SLOT_RAW {
                        let word = unsafe { core::ptr::read_unaligned(address.cast::<i64>()) };
                        payload.push(3);
                        payload.extend_from_slice(&word.to_le_bytes());
                    } else {
                        return None;
                    }
                }
            }
            Some((payload, programs))
        }

        unsafe fn block_vtable_image_reference(&self, api: &IdApi, vtable: Oop) -> Option<u32> {
            let authority = self.authority.as_ref()?;
            let raw_authority = v5::Authority::parse(authority).ok()?.raw;
            let address = unsafe {
                floor_symbol(
                    api,
                    b"birnam_id_stackless_block_vtable_image_reference_v1\0",
                )
            }?;
            // SAFETY: the selected floor exports this exact loader-private v1
            // signature. `floor_symbol` binds the image that owns this IdApi.
            let query: unsafe extern "C" fn(*const u8, usize, Oop, *mut u32) -> u32 =
                unsafe { core::mem::transmute(address) };
            let mut index = 0;
            (unsafe {
                query(
                    raw_authority.as_ptr(),
                    raw_authority.len(),
                    vtable,
                    &mut index,
                )
            } == 1)
                .then_some(index)
        }
    }

    fn encode_v7_name_field(output: &mut Vec<u8>, tag: u8, bytes: &[u8]) -> Option<()> {
        if bytes.is_empty() || bytes.contains(&0) {
            return None;
        }
        output.push(tag);
        output.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_le_bytes());
        output.extend_from_slice(bytes);
        Some(())
    }

    unsafe fn encode_v7_oop(
        api: &IdApi,
        output: &mut Vec<u8>,
        object: Oop,
        heap: &HeapPreflight,
    ) -> Option<()> {
        if object.is_null() {
            output.push(8);
        } else if object == api.nilObj {
            output.push(0);
        } else if object == api.trueObj {
            output.push(1);
        } else if object == api.falseObj {
            output.push(2);
        } else if unsafe { (api.isSmallInt)(object) } != 0 {
            output.push(3);
            output.extend_from_slice(&unsafe { (api.smallIntValue)(object) }.to_le_bytes());
        } else if unsafe { (api.isFloat)(object) } != 0 {
            output.push(4);
            output.extend_from_slice(&unsafe { (api.floatValue)(object) }.to_bits().to_le_bytes());
        } else if unsafe { (api.vtableOf)(object) } == api.vtable_vt
            && let Some(code_object) = unsafe { portable_block_code_object(api, object) }
        {
            output.push(12);
            output.extend_from_slice(&heap.relocations.get(&(code_object as usize))?.to_le_bytes());
        } else if unsafe { (api.vtableOf)(object) } == api.symbol_vt {
            encode_v7_name_field(output, 5, &unsafe {
                c_string_bytes((api.symbolName)(object))
            }?)?;
        } else if let Some(name) = unsafe { c_string_bytes((api.exportedNameOf)(object)) } {
            encode_v7_name_field(output, 9, &name)?;
        } else if unsafe { (api.vtableOf)(object) } == api.string_vt {
            let mut length = 0usize;
            let pointer = unsafe { (api.stringBytes)(object, &mut length) };
            if pointer.is_null() && length != 0 {
                return None;
            }
            let bytes = if length == 0 {
                &[][..]
            } else {
                unsafe { core::slice::from_raw_parts(pointer, length) }
            };
            output.push(6);
            output.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_le_bytes());
            output.extend_from_slice(bytes);
        } else {
            output.push(7);
            output.extend_from_slice(&heap.relocations.get(&(object as usize))?.to_le_bytes());
        }
        Some(())
    }

    unsafe fn binding_field(
        api: &IdApi,
        object: Oop,
        heap: &HeapPreflight,
        provenance: bool,
    ) -> Option<OwnedBindingField> {
        if object == api.nilObj {
            return Some(OwnedBindingField::Nil);
        }
        if object == api.trueObj {
            return Some(OwnedBindingField::True);
        }
        if object == api.falseObj {
            return Some(OwnedBindingField::False);
        }
        if unsafe { (api.isSmallInt)(object) } != 0 {
            return Some(OwnedBindingField::Integer(unsafe {
                (api.smallIntValue)(object)
            }));
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if vtable == api.symbol_vt && !provenance {
            return Some(OwnedBindingField::Symbol(unsafe {
                c_string_bytes((api.symbolName)(object))
            }?));
        }
        if provenance && vtable == api.string_vt {
            let mut length = 0usize;
            let pointer = unsafe { (api.stringBytes)(object, &mut length) };
            if pointer.is_null() && length != 0 {
                return None;
            }
            return Some(OwnedBindingField::String(if length == 0 {
                Vec::new()
            } else {
                unsafe { core::slice::from_raw_parts(pointer, length) }.to_vec()
            }));
        }
        if let Some(name) = unsafe { c_string_bytes((api.exportedNameOf)(object)) } {
            return Some(OwnedBindingField::ExportRef(name));
        }
        heap.relocations
            .get(&(object as usize))
            .copied()
            .map(OwnedBindingField::HeapRef)
    }

    unsafe fn portable_binding_records(
        api: &IdApi,
        snapshot: &[PortableBindingSnapshot],
        heap: &HeapPreflight,
        programs: &[u8],
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let mut owned = Vec::with_capacity(snapshot.len());
        for binding in snapshot {
            owned.push(OwnedBinding {
                target: unsafe { binding_field(api, binding.target, heap, false) }?,
                selector: unsafe { symbol_bytes(api, binding.selector) }?,
                implementation: unsafe { binding_field(api, binding.implementation, heap, false) }?,
                provenance: unsafe { binding_field(api, binding.provenance, heap, true) }?,
            });
        }
        let specs = owned
            .iter()
            .map(
                |binding| birnam_artifact::code_binding_inventory::BindingSpec {
                    target: binding.target.borrowed(),
                    selector: &binding.selector,
                    implementation: binding.implementation.borrowed(),
                    provenance: binding.provenance.borrowed(),
                },
            )
            .collect::<Vec<_>>();
        let bindings = birnam_artifact::code_binding_inventory::encode(&specs).ok()?;
        let program_count = birnam_artifact::portable_inventory::inspect(programs)
            .ok()?
            .program_count as usize;
        let selection = birnam_artifact::materialization_selection::encode(
            programs,
            &bindings,
            &vec![birnam_artifact::materialization_selection::Disposition::Resumable; owned.len()],
            &vec![
                birnam_artifact::materialization_selection::Disposition::Resumable;
                program_count
            ],
        )
        .ok()?;
        Some((bindings, selection))
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

    unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
        // Keep the argument pointer non-null even for a zero-argument send. Native
        // primitive methods use a non-null slice pointer in their native signature;
        // passing an empty Rust slice preserves that contract across the C ABI.
        let arguments: [Oop; 0] = [];
        unsafe { (api.send)(receiver, intern(api, selector), arguments.as_ptr(), 0) }
    }

    unsafe fn send1(api: &IdApi, receiver: Oop, selector: &[u8], argument: Oop) -> Oop {
        let arguments = [argument];
        unsafe {
            (api.send)(
                receiver,
                intern(api, selector),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn send2(api: &IdApi, receiver: Oop, selector: &[u8], first: Oop, second: Oop) -> Oop {
        let arguments = [first, second];
        unsafe {
            (api.send)(
                receiver,
                intern(api, selector),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn image_rejection_error(api: &IdApi, kind: ImageRejectionKind) -> Oop {
        let error = unsafe { (api.import)(c"Error".as_ptr()) };
        if error.is_null() || error == api.nilObj {
            return api.nilObj;
        }
        let arguments = [unsafe { intern(api, kind.symbol()) }];
        unsafe {
            (api.send)(
                error,
                intern(api, b"kind:\0"),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn c_string_bytes(pointer: *const c_char) -> Option<Vec<u8>> {
        if pointer.is_null() {
            return None;
        }
        // SAFETY: IdApi lends one NUL-terminated name while the owning Symbol
        // remains rooted. Copy it before any later allocation, send, or GC.
        Some(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec())
    }

    unsafe fn oop_string_bytes(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        if object.is_null() || unsafe { (api.vtableOf)(object) } != api.string_vt {
            return None;
        }
        let mut length = 0usize;
        let bytes = unsafe { (api.stringBytes)(object, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        Some(if length == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec()
        })
    }

    unsafe fn symbol_bytes(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        if object.is_null() || unsafe { (api.vtableOf)(object) } != api.symbol_vt {
            return None;
        }
        unsafe { c_string_bytes((api.symbolName)(object)) }
    }

    unsafe fn portable_program_view_api(api: &IdApi) -> Option<&'static PortableProgramViewApiV1> {
        let object = unsafe { (api.import)(PORTABLE_PROGRAM_VIEW_API_NAME.as_ptr().cast()) };
        let vtable = unsafe { (api.import)(PORTABLE_PROGRAM_VIEW_API_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableProgramViewApiV1>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableProgramViewApiV1>() };
        (capability.abi_version == PORTABLE_PROGRAM_VIEW_API_V1_VERSION
            && capability.struct_size == size_of::<PortableProgramViewApiV1>() as u64)
            .then_some(capability)
    }

    unsafe fn portable_code_api_v3(api: &IdApi) -> Option<&'static PortableCodeApiV3> {
        let object = unsafe { (api.import)(PORTABLE_CODE_API_V3_NAME.as_ptr().cast()) };
        let vtable = unsafe { (api.import)(PORTABLE_CODE_API_V3_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV3>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableCodeApiV3>() };
        (capability.abi_version == PORTABLE_CODE_API_V3_VERSION
            && capability.struct_size == size_of::<PortableCodeApiV3>() as u64)
            .then_some(capability)
    }

    unsafe fn portable_code_api_v2(api: &IdApi) -> Option<&'static PortableCodeApiV2> {
        let object = unsafe { (api.import)(PORTABLE_CODE_API_V2_NAME.as_ptr().cast()) };
        let vtable = unsafe { (api.import)(PORTABLE_CODE_API_V2_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV2>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableCodeApiV2>() };
        (capability.abi_version == PORTABLE_CODE_API_V2_VERSION
            && capability.struct_size == size_of::<PortableCodeApiV2>() as u64)
            .then_some(capability)
    }

    unsafe fn copy_code_object_program(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        let vtable = unsafe { (api.vtableOf)(object) };
        if unsafe { c_string_bytes((api.exportedNameOf)(vtable)) }.as_deref()
            != Some(b"CodeObjectVt")
        {
            return None;
        }
        let capability = unsafe { portable_program_view_api(api) }?;
        let length = unsafe { (capability.copy_program)(object, ptr::null_mut(), 0) };
        if length == 0 || length > 64 << 20 {
            return None;
        }
        let mut bytes = vec![0; length];
        if unsafe { (capability.copy_program)(object, bytes.as_mut_ptr(), bytes.len()) } != length
            || birnam_artifact::portable_program::inspect(&bytes).is_err()
        {
            return None;
        }
        Some(bytes)
    }

    unsafe fn portable_block_code_object(api: &IdApi, vtable: Oop) -> Option<Oop> {
        if vtable.is_null() {
            return None;
        }
        let owner = unsafe { (api.vtableOwner)(vtable) };
        if owner.is_null() {
            return None;
        }
        let owner_vtable = unsafe { (api.vtableOf)(owner) };
        if unsafe { c_string_bytes((api.exportedNameOf)(owner_vtable)) }.as_deref()
            != Some(PORTABLE_BLOCK_CARRIER_VTABLE_NAME)
            || unsafe { (api.objectPayloadSize)(owner) } != 2 * size_of::<Oop>()
        {
            return None;
        }
        let code_object = unsafe { owner.cast::<Oop>().read() };
        unsafe { copy_code_object_program(api, code_object) }.map(|_| code_object)
    }

    unsafe fn has_global_recovery_source(api: &IdApi, store: Oop, attribute: Oop) -> bool {
        let source_entity = unsafe { intern(api, b"__globalsrc__\0") };
        (unsafe { send2(api, store, b"hasValue:attr:\0", source_entity, attribute) }) == api.trueObj
    }

    #[derive(Clone, Copy)]
    enum DeferredFactClass {
        Source,
        Code,
        Reject,
    }

    unsafe fn deferred_fact_class(
        api: &IdApi,
        store: Oop,
        entity: Oop,
        attribute: Oop,
    ) -> DeferredFactClass {
        let Some(entity_name) = (unsafe { symbol_bytes(api, entity) }) else {
            return DeferredFactClass::Reject;
        };
        let Some(attribute_name) = (unsafe { symbol_bytes(api, attribute) }) else {
            return DeferredFactClass::Reject;
        };
        if entity_name == b"__globals__" {
            if attribute_name.starts_with(b"__") {
                return DeferredFactClass::Code;
            }
            if unsafe { has_global_recovery_source(api, store, attribute) } {
                return DeferredFactClass::Source;
            }
            return DeferredFactClass::Reject;
        }
        if unsafe { (api.vtableOf)(entity) } == api.vtable_vt {
            return DeferredFactClass::Code;
        }
        DeferredFactClass::Reject
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
        if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
            heap.image_rejection.get_or_insert(kind);
            return false;
        }
        if let Some(index) = unsafe { heap.block_vtable_image_reference(api, object) } {
            encoder.resumable_ref_v5(index);
            return true;
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if vtable == api.symbol_vt {
            let Some(bytes) = (unsafe { c_string_bytes((api.symbolName)(object)) }) else {
                return false;
            };
            return encoder.symbol(&bytes).is_ok();
        }
        if let Some(name) = unsafe { c_string_bytes((api.exportedNameOf)(object)) } {
            return encoder.export_ref(&name).is_ok();
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

    fn record_fact_counts(
        kept: u32,
        recovered: u32,
        rejected: u32,
        recovered_slots: u32,
        rejected_slots: u32,
    ) {
        let mut report = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        report.saved_facts = kept;
        report.recovered_facts = recovered;
        report.rejected_facts = rejected;
        report.recovered_slots = recovered_slots;
        report.rejected_slots = rejected_slots;
    }

    /// Select the image writer's generation authority explicitly.
    ///
    /// Mixed hosts retain the canonical `Codegen` owner. The Rust owner is only
    /// admitted by an explicit test/selection environment value, and must expose
    /// the durable-generation bootstrap plus capture selectors before it can be
    /// used. An unknown explicit value fails closed instead of silently falling
    /// back to another authority.
    unsafe fn writer_codegen_authority(api: &IdApi) -> Option<Oop> {
        let requested = std::env::var("BIRNAM_RUST_IMAGE_WRITER_AUTHORITY").ok();
        if requested.as_deref() == Some("rust") {
            let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
            if codegen.is_null()
                || codegen == api.nilObj
                || !unsafe {
                    direct_method_present(api, codegen, RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION)
                        && direct_method_present(api, codegen, b"currentGenerationDigest\0")
                        && direct_method_present(api, codegen, b"compileImageCode:manifest:\0")
                }
            {
                return None;
            }
            let initialized =
                unsafe { send0(api, codegen, RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION) };
            return (initialized != api.nilObj && !initialized.is_null()).then_some(codegen);
        }
        if requested.is_some() {
            return None;
        }
        let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        if rust_image_canonical_selected() {
            // Match the native image driver's first-save behavior for script hosts:
            // close the initial dispatch generation before asking Codegen for its
            // digest. Explicit Rust Image fixtures do not set the canonical marker
            // and therefore retain their diagnostic-only no-publication contract.
            let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
            if !globals.is_null() && globals != api.nilObj {
                let key = unsafe { intern(api, b"DispatchGeneration\0") };
                let generations = [key];
                let generations =
                    unsafe { (api.send)(globals, intern(api, b"at:\0"), generations.as_ptr(), 1) };
                if !generations.is_null() && generations != api.nilObj {
                    let current = unsafe { send0(api, generations, b"current\0") };
                    if current.is_null() || current == api.nilObj {
                        let _ = unsafe { send0(api, generations, b"initializeCurrent\0") };
                    }
                }
            }
        }
        Some(codegen)
    }

    fn rust_image_publication_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_PUBLICATION")
            .ok()
            .as_deref()
            == Some("rust")
    }

    fn rust_image_writer_authority_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_WRITER_AUTHORITY")
            .ok()
            .as_deref()
            == Some("rust")
    }

    fn rust_image_canonical_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_CANONICAL").ok().as_deref() == Some("rust")
    }

    unsafe fn current_generation_digest(api: &IdApi) -> Option<[u8; AUTHORITY_DIGEST_LEN]> {
        let codegen = unsafe { writer_codegen_authority(api) }?;
        let digest = unsafe { send0(api, codegen, b"currentGenerationDigest\0") };
        if digest.is_null()
            || digest == api.nilObj
            || unsafe { (api.vtableOf)(digest) } != api.string_vt
        {
            return None;
        }
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(digest, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        let bytes = if length == 0 {
            &[]
        } else {
            // SAFETY: stringBytes supplies `length` readable bytes for this live String.
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        authority_digest_from_bytes(bytes)
    }

    unsafe fn compile_portable_image_code(
        api: &IdApi,
        records: &PortableSaveRecords,
    ) -> Option<Vec<u8>> {
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let programs = unsafe { (api.mkString)(records.programs.as_ptr(), records.programs.len()) };
        let bindings = unsafe { (api.mkString)(records.bindings.as_ptr(), records.bindings.len()) };
        let selection =
            unsafe { (api.mkString)(records.selection.as_ptr(), records.selection.len()) };
        let arguments = [programs, bindings, selection];
        let code = unsafe {
            (api.send)(
                codegen,
                intern(api, COMPILE_PORTABLE_IMAGE_CODE),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        if code.is_null() || code == api.nilObj || unsafe { (api.vtableOf)(code) } != api.string_vt
        {
            return None;
        }
        unsafe { oop_string_bytes(api, code) }.filter(|bytes| !bytes.is_empty())
    }

    #[cfg(test)]
    pub(crate) fn preflight_image_path(path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
            return false;
        }
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_IMAGE_BYTES {
            return false;
        }
        Image::preflight(&bytes, IMAGE_ARCH_TAG, runtime_abi_hash()).is_ok()
    }

    #[derive(Clone, Copy, Default)]
    struct ImageFeatures {
        has_facts: bool,
        has_heap: bool,
        has_code: bool,
        authority_digest: Option<[u8; AUTHORITY_DIGEST_LEN]>,
        composition_identity: Option<[u8; 64]>,
    }

    fn composition_identity(checksums: &[u64]) -> Option<[u8; 64]> {
        let mut digest = Sha256::new();
        digest.update(b"birnam.image-composition.v1\0");
        digest.update(u32::try_from(checksums.len()).ok()?.to_le_bytes());
        for checksum in checksums {
            digest.update(checksum.to_le_bytes());
        }
        let raw: [u8; 32] = digest.finalize().into();
        let mut spelling = [0; 64];
        for (index, byte) in raw.into_iter().enumerate() {
            spelling[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
            spelling[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
        }
        Some(spelling)
    }

    #[derive(Clone, Copy)]
    enum V7Value<'a> {
        Nil,
        True,
        False,
        Integer(i64),
        FloatBits(u64),
        Symbol(&'a [u8]),
        String(&'a [u8]),
        HeapRef(u32),
        Null,
        ExportRef(&'a [u8]),
        ProgramRef(u32),
        PortableBlockVtable(u32),
    }

    struct V7HeapObject<'a> {
        locator: V7Value<'a>,
        fields: Vec<V7Value<'a>>,
    }

    fn parse_v7_value<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<V7Value<'a>> {
        let tag = *bytes.get(*offset)?;
        *offset = offset.checked_add(1)?;
        match tag {
            0 => Some(V7Value::Nil),
            1 => Some(V7Value::True),
            2 => Some(V7Value::False),
            3 | 4 => {
                let end = offset.checked_add(8)?;
                let bits = u64::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
                *offset = end;
                if tag == 3 {
                    Some(V7Value::Integer(i64::from_le_bytes(bits.to_le_bytes())))
                } else {
                    Some(V7Value::FloatBits(bits))
                }
            }
            5 | 6 | 9 => {
                let end = offset.checked_add(4)?;
                let length = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?) as usize;
                *offset = end;
                let end = offset.checked_add(length)?;
                let value = bytes.get(*offset..end)?;
                *offset = end;
                match tag {
                    5 => Some(V7Value::Symbol(value)),
                    6 => Some(V7Value::String(value)),
                    9 => Some(V7Value::ExportRef(value)),
                    _ => None,
                }
            }
            7 | 11 | 12 => {
                let end = offset.checked_add(4)?;
                let index = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
                *offset = end;
                match tag {
                    7 => Some(V7Value::HeapRef(index)),
                    11 => Some(V7Value::ProgramRef(index)),
                    12 => Some(V7Value::PortableBlockVtable(index)),
                    _ => None,
                }
            }
            8 => Some(V7Value::Null),
            _ => None,
        }
    }

    fn parse_v7_heaps<'a>(payloads: &[&'a [u8]]) -> Option<Vec<V7HeapObject<'a>>> {
        let mut objects = Vec::new();
        for payload in payloads {
            let mut offset = 0usize;
            let end = offset.checked_add(4)?;
            let count = u32::from_le_bytes(payload.get(offset..end)?.try_into().ok()?);
            offset = end;
            for _ in 0..count {
                let locator = parse_v7_value(payload, &mut offset)?;
                let end = offset.checked_add(4)?;
                let words = u32::from_le_bytes(payload.get(offset..end)?.try_into().ok()?);
                offset = end;
                let fields = (0..words)
                    .map(|_| parse_v7_value(payload, &mut offset))
                    .collect::<Option<Vec<_>>>()?;
                objects.push(V7HeapObject { locator, fields });
            }
            if offset != payload.len() {
                return None;
            }
        }
        Some(objects)
    }

    unsafe fn import_bytes(api: &IdApi, name: &[u8]) -> Option<Oop> {
        let mut terminated = Vec::with_capacity(name.len().checked_add(1)?);
        terminated.extend_from_slice(name);
        terminated.push(0);
        let object = unsafe { (api.import)(terminated.as_ptr().cast()) };
        (!object.is_null()).then_some(object)
    }

    unsafe fn materialize_v7_value(
        api: &IdApi,
        value: V7Value<'_>,
        relocations: &[Oop],
    ) -> Option<Oop> {
        match value {
            V7Value::Nil => Some(api.nilObj),
            V7Value::True => Some(api.trueObj),
            V7Value::False => Some(api.falseObj),
            V7Value::Integer(value) => Some(unsafe { (api.mkSmallInt)(value) }),
            V7Value::FloatBits(bits) => Some(unsafe { (api.mkFloat)(f64::from_bits(bits)) }),
            V7Value::Symbol(bytes) => {
                let mut terminated = Vec::with_capacity(bytes.len().checked_add(1)?);
                terminated.extend_from_slice(bytes);
                terminated.push(0);
                Some(unsafe { (api.intern)(terminated.as_ptr().cast()) })
            }
            V7Value::String(bytes) => Some(unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }),
            V7Value::HeapRef(index) => relocations.get(index as usize).copied(),
            V7Value::Null => Some(ptr::null_mut()),
            V7Value::ExportRef(bytes) => unsafe { import_bytes(api, bytes) },
            V7Value::ProgramRef(_) | V7Value::PortableBlockVtable(_) => None,
        }
    }

    unsafe fn materialize_v7_heaps(
        api: &IdApi,
        decoded: &v7::DecodedImage<'_>,
        materialization_owner: Oop,
    ) -> Option<Vec<Oop>> {
        let objects = parse_v7_heaps(&decoded.heaps)?;
        let programs = birnam_artifact::portable_inventory::inspect(decoded.programs).ok()?;
        let mut relocations = vec![ptr::null_mut(); objects.len()];

        // Ordinary exported-layout shells have no execution authority and may
        // be allocated before portable meaning. This also resolves the usual
        // heap provenance references before immutable CodeObjects are admitted.
        for (index, object) in objects.iter().enumerate() {
            let V7Value::ExportRef(name) = object.locator else {
                continue;
            };
            if name == b"CodeObjectVt" {
                continue;
            }
            let vtable = unsafe { import_bytes(api, name) }?;
            let bytes = object.fields.len().checked_mul(size_of::<usize>())?;
            relocations[index] = unsafe { (api.alloc)(vtable, bytes) };
            if relocations[index].is_null() {
                return None;
            }
        }

        let portable_api = unsafe { portable_code_api_v3(api) }?;
        for (index, object) in objects.iter().enumerate() {
            let V7Value::ExportRef(name) = object.locator else {
                continue;
            };
            if name != b"CodeObjectVt" || object.fields.len() != 2 {
                continue;
            }
            let V7Value::ProgramRef(program_index) = object.fields[0] else {
                return None;
            };
            let provenance = unsafe { materialize_v7_value(api, object.fields[1], &relocations) }?;
            let program = programs.program(program_index as usize)?;
            relocations[index] = unsafe {
                (portable_api.admit_program_with_provenance)(
                    program.as_ptr(),
                    program.len(),
                    provenance,
                )
            };
            if relocations[index].is_null() || relocations[index] == api.nilObj {
                return None;
            }
        }

        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let block_api = unsafe { portable_code_api_v2(api) }?;
        for (index, object) in objects.iter().enumerate() {
            let V7Value::PortableBlockVtable(code_object_index) = object.locator else {
                continue;
            };
            let code_object = *relocations.get(code_object_index as usize)?;
            let code_object_record = objects.get(code_object_index as usize)?;
            let V7Value::ProgramRef(program_index) = *code_object_record.fields.first()? else {
                return None;
            };
            let capture_count = object.fields.len().checked_sub(1)?;
            let index_oop = unsafe { (api.mkSmallInt)(i64::from(program_index)) };
            let vtable = unsafe { send1(api, codegen, PORTABLE_IMAGE_BLOCK_VTABLE, index_oop) };
            if vtable.is_null() || vtable == api.nilObj {
                return None;
            }
            let captures = vec![api.nilObj; capture_count];
            relocations[index] = unsafe {
                (block_api.new_materialized_block)(
                    vtable,
                    code_object,
                    captures.as_ptr(),
                    captures.len(),
                    materialization_owner,
                )
            };
            if relocations[index].is_null() || relocations[index] == api.nilObj {
                return None;
            }
        }

        for (index, object) in objects.iter().enumerate() {
            if matches!(object.locator, V7Value::ExportRef(b"CodeObjectVt")) {
                continue;
            }
            let receiver = *relocations.get(index)?;
            if receiver.is_null() {
                return None;
            }
            let vtable = unsafe { (api.vtableOf)(receiver) };
            for (word, field) in object.fields.iter().copied().enumerate() {
                let address = unsafe { receiver.cast::<usize>().add(word) };
                match unsafe { (api.slotKind)(vtable, word) } {
                    birnam_abi::SLOT_OOP => {
                        let value = unsafe { materialize_v7_value(api, field, &relocations) }?;
                        unsafe { address.write(value as usize) };
                    }
                    birnam_abi::SLOT_RAW => {
                        let V7Value::Integer(value) = field else {
                            return None;
                        };
                        unsafe { address.write(value as usize) };
                    }
                    _ => return None,
                }
            }
        }
        Some(relocations)
    }

    struct StagedV7Facts {
        store: Oop,
        facts: Vec<[Oop; 6]>,
        basis: Oop,
        oldest: Oop,
        policy: Oop,
        argument: Oop,
    }

    unsafe fn prepare_v7_facts(
        api: &IdApi,
        payloads: &[&[u8]],
        relocations: &[Oop],
    ) -> Option<StagedV7Facts> {
        if payloads.is_empty() {
            return Some(StagedV7Facts {
                store: api.nilObj,
                facts: Vec::new(),
                basis: api.nilObj,
                oldest: api.nilObj,
                policy: api.nilObj,
                argument: api.nilObj,
            });
        }
        let store = unsafe { (api.import)(c"Store".as_ptr()) };
        if store.is_null() || store == api.nilObj {
            return None;
        }
        let mut facts = Vec::new();
        let mut final_metadata = None;
        for payload in payloads {
            if payload.len() < 36 {
                return None;
            }
            let basis = u64::from_le_bytes(payload[0..8].try_into().ok()?);
            let oldest = u64::from_le_bytes(payload[8..16].try_into().ok()?);
            let policy = payload[16];
            let argument = u64::from_le_bytes(payload[24..32].try_into().ok()?);
            let count = u32::from_le_bytes(payload[32..36].try_into().ok()?);
            let metadata = (basis, oldest, policy, argument);
            if final_metadata
                .replace(metadata)
                .is_some_and(|old| old != metadata)
            {
                return None;
            }
            let mut offset = 36usize;
            for _ in 0..count {
                let op = *payload.get(offset)?;
                let historied = *payload.get(offset.checked_add(1)?)?;
                let tx_end = offset.checked_add(10)?;
                let tx = u64::from_le_bytes(payload.get(offset + 2..tx_end)?.try_into().ok()?);
                offset = tx_end;
                let entity = parse_v7_value(payload, &mut offset)?;
                let attribute = parse_v7_value(payload, &mut offset)?;
                let value = parse_v7_value(payload, &mut offset)?;
                facts.push([
                    unsafe { materialize_v7_value(api, entity, relocations) }?,
                    unsafe { materialize_v7_value(api, attribute, relocations) }?,
                    unsafe { materialize_v7_value(api, value, relocations) }?,
                    unsafe { (api.mkSmallInt)(tx as i64) },
                    unsafe { (api.mkSmallInt)(i64::from(op)) },
                    if historied == 1 {
                        api.trueObj
                    } else {
                        api.falseObj
                    },
                ]);
            }
            if offset != payload.len() {
                return None;
            }
        }
        let (basis, oldest, policy, argument) = final_metadata?;
        Some(StagedV7Facts {
            store,
            facts,
            basis: unsafe { (api.mkSmallInt)(basis as i64) },
            oldest: unsafe { (api.mkSmallInt)(oldest as i64) },
            policy: unsafe { (api.mkSmallInt)(i64::from(policy)) },
            argument: unsafe { (api.mkSmallInt)(argument as i64) },
        })
    }

    unsafe fn replay_v7_facts(api: &IdApi, staged: &StagedV7Facts) -> Option<usize> {
        if staged.facts.is_empty() && staged.store == api.nilObj {
            return Some(0);
        }
        if unsafe { send0(api, staged.store, b"imageLoadBegin\0") } == api.nilObj {
            return None;
        }
        for arguments in &staged.facts {
            if unsafe {
                (api.send)(
                    staged.store,
                    intern(api, b"imageAddFact:attr:value:tx:op:historied:\0"),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } == api.nilObj
            {
                return None;
            }
        }
        let arguments = [
            staged.basis,
            api.falseObj,
            staged.oldest,
            staged.policy,
            staged.argument,
        ];
        (unsafe {
            (api.send)(
                staged.store,
                intern(api, b"imageLoadEnd:raw:oldest:policy:argument:\0"),
                arguments.as_ptr(),
                arguments.len(),
            )
        } != api.nilObj)
            .then_some(staged.facts.len())
    }

    unsafe fn materialize_binding_field(
        api: &IdApi,
        field: birnam_artifact::code_binding_inventory::Field<'_>,
        relocations: &[Oop],
    ) -> Option<Oop> {
        use birnam_artifact::code_binding_inventory::Field;
        match field {
            Field::Nil => Some(api.nilObj),
            Field::True => Some(api.trueObj),
            Field::False => Some(api.falseObj),
            Field::Integer(value) => Some(unsafe { (api.mkSmallInt)(value) }),
            Field::Symbol(bytes) => {
                let mut name = bytes.to_vec();
                name.push(0);
                Some(unsafe { (api.intern)(name.as_ptr().cast()) })
            }
            Field::String(bytes) => Some(unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }),
            Field::HeapRef(index) => relocations.get(index as usize).copied(),
            Field::ExportRef(bytes) => unsafe { import_bytes(api, bytes) },
        }
    }

    unsafe fn prepare_v7_bindings(api: &IdApi, bytes: &[u8], relocations: &[Oop]) -> Option<usize> {
        let inventory = birnam_artifact::code_binding_inventory::inspect(bytes).ok()?;
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        for index in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(index)?;
            let target = unsafe { materialize_binding_field(api, binding.target, relocations) }?;
            let implementation =
                unsafe { materialize_binding_field(api, binding.implementation, relocations) }?;
            let provenance =
                unsafe { materialize_binding_field(api, binding.provenance, relocations) }?;
            if target.is_null()
                || implementation.is_null()
                || provenance.is_null()
                || unsafe { (api.vtableOf)(target) }.is_null()
            {
                return None;
            }
            let index = unsafe { (api.mkSmallInt)(i64::try_from(index).ok()?) };
            let arguments = [index, implementation];
            if unsafe {
                (api.send)(
                    codegen,
                    intern(api, PREPARE_PORTABLE_IMAGE_BINDING),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } != api.trueObj
            {
                return None;
            }
        }
        Some(inventory.binding_count as usize)
    }

    unsafe fn install_v7_bindings(api: &IdApi, bytes: &[u8], relocations: &[Oop]) -> Option<usize> {
        let inventory = birnam_artifact::code_binding_inventory::inspect(bytes).ok()?;
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        for index in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(index)?;
            let target = unsafe { materialize_binding_field(api, binding.target, relocations) }?;
            let implementation =
                unsafe { materialize_binding_field(api, binding.implementation, relocations) }?;
            let provenance =
                unsafe { materialize_binding_field(api, binding.provenance, relocations) }?;
            let index_oop = unsafe { (api.mkSmallInt)(i64::try_from(index).ok()?) };
            let arguments = [index_oop, target, implementation, provenance];
            if unsafe {
                (api.send)(
                    codegen,
                    intern(api, INSTALL_PORTABLE_IMAGE_BINDING),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } == api.nilObj
            {
                return None;
            }
        }
        Some(inventory.binding_count as usize)
    }

    unsafe fn begin_v7_native_unit(
        api: &IdApi,
        decoded: &v7::DecodedImage<'_>,
        rebuild: bool,
    ) -> Option<(Oop, bool)> {
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let programs = unsafe { (api.mkString)(decoded.programs.as_ptr(), decoded.programs.len()) };
        let bindings = unsafe { (api.mkString)(decoded.bindings.as_ptr(), decoded.bindings.len()) };
        let selection =
            unsafe { (api.mkString)(decoded.selection.as_ptr(), decoded.selection.len()) };
        if !rebuild {
            if let Some(code) = decoded.code {
                let unit = code.get(72..)?;
                let unit = unsafe { (api.mkString)(unit.as_ptr(), unit.len()) };
                let arguments = [unit, programs, bindings, selection];
                let owner = unsafe {
                    (api.send)(
                        codegen,
                        intern(api, BEGIN_PORTABLE_IMAGE_MAPPED),
                        arguments.as_ptr(),
                        arguments.len(),
                    )
                };
                return (owner != api.nilObj && !owner.is_null()).then_some((owner, true));
            }
        }
        let arguments = [programs, bindings, selection];
        let owner = unsafe {
            (api.send)(
                codegen,
                intern(api, BEGIN_PORTABLE_IMAGE_REBUILD),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        (owner != api.nilObj && !owner.is_null()).then_some((owner, false))
    }

    unsafe fn load_v7(api: &IdApi, bytes: &[u8], rebuild: bool) -> Oop {
        let Ok(decoded) = v7::decode(bytes, IMAGE_ARCH_TAG, runtime_abi_hash()) else {
            return api.nilObj;
        };
        unsafe { (api.gcSetAuto)(0) };
        struct AutoGc<'a>(&'a IdApi);
        impl Drop for AutoGc<'_> {
            fn drop(&mut self) {
                unsafe { (self.0.gcSetAuto)(1) };
            }
        }
        let _auto_gc = AutoGc(api);
        let Some((owner, mapped)) = (unsafe { begin_v7_native_unit(api, &decoded, rebuild) })
        else {
            return api.nilObj;
        };
        struct StageGuard<'a> {
            api: &'a IdApi,
            armed: bool,
        }
        impl Drop for StageGuard<'_> {
            fn drop(&mut self) {
                if !self.armed {
                    return;
                }
                let codegen = unsafe { (self.api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
                if !codegen.is_null() && codegen != self.api.nilObj {
                    let _ = unsafe { send0(self.api, codegen, CANCEL_PORTABLE_IMAGE_UNIT) };
                }
            }
        }
        let mut stage = StageGuard { api, armed: true };
        let Some(relocations) = (unsafe { materialize_v7_heaps(api, &decoded, owner) }) else {
            return api.nilObj;
        };
        let Some(staged_facts) = (unsafe { prepare_v7_facts(api, &decoded.facts, &relocations) })
        else {
            return api.nilObj;
        };
        if unsafe { prepare_v7_bindings(api, decoded.bindings, &relocations) }.is_none() {
            return api.nilObj;
        }
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null()
            || codegen == api.nilObj
            || unsafe { send0(api, codegen, FINISH_PORTABLE_IMAGE_UNIT) } != api.trueObj
        {
            return api.nilObj;
        }
        stage.armed = false;
        if unsafe { replay_v7_facts(api, &staged_facts) }.is_none() {
            return api.nilObj;
        }
        if unsafe { install_v7_bindings(api, decoded.bindings, &relocations) }.is_none() {
            return api.nilObj;
        }
        if mapped { api.trueObj } else { api.falseObj }
    }

    unsafe fn image_v5_layout(api: &IdApi, name: &[u8], words: u32) -> Option<Vec<v5::SlotKind>> {
        let mut terminated = Vec::with_capacity(name.len().checked_add(1)?);
        terminated.extend_from_slice(name);
        terminated.push(0);
        let vtable = unsafe { (api.import)(terminated.as_ptr().cast()) };
        if vtable.is_null() || vtable == api.nilObj {
            return None;
        }
        let words = usize::try_from(words).ok()?;
        let mut layout = Vec::with_capacity(words);
        for word in 0..words {
            layout.push(match unsafe { (api.slotKind)(vtable, word) } {
                birnam_abi::SLOT_OOP => v5::SlotKind::Oop,
                birnam_abi::SLOT_RAW => v5::SlotKind::RawWord,
                birnam_abi::SLOT_CODE => v5::SlotKind::Code,
                _ => return None,
            });
        }
        Some(layout)
    }

    unsafe fn inspect_image_path(api: &IdApi, path_bytes: &[u8]) -> Option<ImageFeatures> {
        let path = Path::new(OsStr::from_bytes(path_bytes));
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        if u64::try_from(bytes.len()).ok()? > MAX_IMAGE_BYTES {
            return None;
        }
        let image = v5::Image::preflight_with_layout(
            &bytes,
            IMAGE_ARCH_TAG,
            runtime_abi_hash(),
            |name, words| unsafe { image_v5_layout(api, name, words) },
        )
        .ok()?;
        let mut features = ImageFeatures::default();
        features.authority_digest = Some(*image.authority.spelling);
        features.composition_identity = image
            .code()
            .and_then(|code| composition_identity(&[code.checksum]));
        let mut records = image.records();
        while let Some(record) = records.next_record().ok()? {
            match record.kind {
                v5::RecordKind::Facts => features.has_facts = true,
                v5::RecordKind::Heap => features.has_heap = true,
                v5::RecordKind::Code => features.has_code = true,
                v5::RecordKind::Authority | v5::RecordKind::Resumables => {}
            }
        }
        Some(features)
    }

    unsafe fn runtime_image_path_bytes(api: &IdApi, path: Oop) -> Option<Vec<u8>> {
        if path.is_null() || unsafe { (api.vtableOf)(path) } != api.string_vt {
            return None;
        }
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(path, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            // SAFETY: stringBytes supplies `length` readable bytes for this live String.
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        Some(bytes.to_vec())
    }

    unsafe fn inspect_runtime_image(api: &IdApi, path: Oop) -> Option<ImageFeatures> {
        let path_bytes = unsafe { runtime_image_path_bytes(api, path) }?;
        unsafe { inspect_image_path(api, &path_bytes) }
    }

    unsafe fn preflight_runtime_image(api: &IdApi, path: Oop) -> bool {
        let Some(path) = (unsafe { runtime_image_path_bytes(api, path) }) else {
            return false;
        };
        let Ok(bytes) = fs::read(Path::new(OsStr::from_bytes(&path))) else {
            return false;
        };
        if bytes.get(8..12) == Some(&7u32.to_le_bytes()) {
            return v7::decode(&bytes, IMAGE_ARCH_TAG, runtime_abi_hash()).is_ok();
        }
        unsafe { inspect_image_path(api, &path) }.is_some()
    }

    unsafe fn direct_method_present(api: &IdApi, target: Oop, selector: &[u8]) -> bool {
        let vtable = unsafe { (api.vtableOf)(target) };
        if vtable.is_null() {
            return false;
        }
        let selector = unsafe { intern(api, selector) };
        let mut index = 0usize;
        loop {
            let entry = unsafe { (api.vtableSelectorAt)(vtable, index) };
            if entry.is_null() {
                return false;
            }
            if entry == selector {
                return true;
            }
            index = index.saturating_add(1);
        }
    }

    unsafe fn rust_codegen_image_authority(api: &IdApi) -> Option<Oop> {
        let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        for selector in [
            RUST_CODEGEN_PREFLIGHT_IMAGE,
            RUST_CODEGEN_BEGIN_IMAGE_RECOVERY,
            RUST_CODEGEN_END_IMAGE_RECOVERY,
            RUST_CODEGEN_REPLAY_IMAGE_FACTS,
            RUST_CODEGEN_REPLAY_STALE_IMAGE_FACTS,
            RUST_CODEGEN_DERIVE_IMAGE_METHOD_TARGETS,
            RUST_CODEGEN_MAP_IMAGE_CODE,
        ] {
            if !unsafe { direct_method_present(api, codegen, selector) } {
                return None;
            }
        }
        Some(codegen)
    }

    unsafe fn send_image_unary(api: &IdApi, receiver: Oop, selector: &[u8], argument: Oop) -> Oop {
        let arguments = [argument];
        unsafe {
            (api.send)(
                receiver,
                intern(api, selector),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn load_with_rust_codegen(api: &IdApi, path: Oop, rebuild: bool) -> Oop {
        if let Ok(mut identity) = IMAGE_COMPOSITION_IDENTITY.lock() {
            *identity = None;
        }
        let Some(path_bytes) = (unsafe { runtime_image_path_bytes(api, path) }) else {
            return api.nilObj;
        };
        let Ok(image_bytes) = fs::read(Path::new(OsStr::from_bytes(&path_bytes))) else {
            return api.nilObj;
        };
        if image_bytes.get(8..12) == Some(&7u32.to_le_bytes()) {
            let composition = v7::decode(&image_bytes, IMAGE_ARCH_TAG, runtime_abi_hash())
                .ok()
                .and_then(|decoded| decoded.code)
                .and_then(|code| code.get(0..8))
                .and_then(|bytes| bytes.try_into().ok())
                .map(u64::from_le_bytes)
                .and_then(|checksum| composition_identity(&[checksum]));
            let codegen = unsafe { (api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) };
            if codegen.is_null()
                || codegen == api.nilObj
                || unsafe { send0(api, codegen, RUST_CODEGEN_BEGIN_IMAGE_RECOVERY) } != api.trueObj
            {
                return api.nilObj;
            }
            let loaded = unsafe { load_v7(api, &image_bytes, rebuild) };
            let ended = unsafe { send0(api, codegen, RUST_CODEGEN_END_IMAGE_RECOVERY) };
            if (loaded != api.trueObj && loaded != api.falseObj) || ended != api.trueObj {
                return api.nilObj;
            }
            if let Ok(mut identity) = IMAGE_COMPOSITION_IDENTITY.lock() {
                *identity = composition;
            }
            return loaded;
        }
        let Some(features) = (unsafe { inspect_runtime_image(api, path) }) else {
            return api.nilObj;
        };
        let Some(codegen) = (unsafe { rust_codegen_image_authority(api) }) else {
            // A complete Rust Codegen provider is required for image recovery.
            return api.falseObj;
        };
        if unsafe { send_image_unary(api, codegen, RUST_CODEGEN_PREFLIGHT_IMAGE, path) }
            != api.trueObj
        {
            // A structurally valid image can carry a generation from an older
            // authority. Rust Codegen rejects that authority for native mapping,
            // but the image loader hands valid bytes to the source-recovery path
            // than reporting malformed input. Other preflight failures remain
            // fail-closed as LoadRejected.
            if let Some(current) = unsafe { current_generation_digest(api) } {
                if let Some(image_authority) = features.authority_digest {
                    if current != image_authority {
                        if unsafe { send0(api, codegen, RUST_CODEGEN_BEGIN_IMAGE_RECOVERY) }
                            != api.trueObj
                        {
                            return api.nilObj;
                        }
                        let replayed = unsafe {
                            send_image_unary(
                                api,
                                codegen,
                                RUST_CODEGEN_REPLAY_STALE_IMAGE_FACTS,
                                path,
                            )
                        };
                        let ended = unsafe { send0(api, codegen, RUST_CODEGEN_END_IMAGE_RECOVERY) };
                        if replayed == api.nilObj || ended != api.trueObj {
                            return api.nilObj;
                        }
                        return api.falseObj;
                    }
                }
            }
            return api.nilObj;
        }
        if unsafe { send0(api, codegen, RUST_CODEGEN_BEGIN_IMAGE_RECOVERY) } != api.trueObj {
            return api.nilObj;
        }

        let outcome = (|| -> Result<Oop, ()> {
            if features.has_facts || features.has_heap {
                let replayed = unsafe {
                    send_image_unary(api, codegen, RUST_CODEGEN_REPLAY_IMAGE_FACTS, path)
                };
                if replayed == api.nilObj {
                    return Err(());
                }
            }
            if rebuild || !features.has_code {
                return Ok(api.falseObj);
            }
            let derived = unsafe { send0(api, codegen, RUST_CODEGEN_DERIVE_IMAGE_METHOD_TARGETS) };
            if derived == api.nilObj {
                return Err(());
            }
            let mapped =
                unsafe { send_image_unary(api, codegen, RUST_CODEGEN_MAP_IMAGE_CODE, path) };
            if mapped == api.nilObj {
                return Err(());
            }
            Ok(api.trueObj)
        })();
        let ended = unsafe { send0(api, codegen, RUST_CODEGEN_END_IMAGE_RECOVERY) };
        if ended != api.trueObj {
            return api.nilObj;
        }
        let Ok(status) = outcome else {
            return api.nilObj;
        };
        if let Some(composition_identity) = features.composition_identity
            && let Ok(mut identity) = IMAGE_COMPOSITION_IDENTITY.lock()
        {
            *identity = Some(composition_identity);
        }
        status
    }

    unsafe fn collect_store_facts(
        api: &IdApi,
        authority: Option<[u8; AUTHORITY_DIGEST_LEN]>,
    ) -> FactPreflight {
        if let Ok(mut records) = PORTABLE_SAVE_RECORDS.lock() {
            *records = None;
        }
        let store = unsafe { (api.import)(c"Store".as_ptr()) };
        if store.is_null() || store == api.nilObj {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: Some(Vec::new()),
                heap_ok: true,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        if unsafe { send0(api, store, b"compact\0") } != store {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        let portable_bindings = unsafe { portable_binding_snapshot(api) };
        let collector_vt = unsafe { (api.import)(COLLECTOR_VT_NAME.as_ptr().cast()) };
        if collector_vt.is_null() {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        let basis_object = unsafe { send0(api, store, b"basis\0") };
        let basis = if unsafe { (api.isSmallInt)(basis_object) } != 0 {
            unsafe { (api.smallIntValue)(basis_object) }.max(0) as u64
        } else {
            0
        };
        let oldest_object = unsafe { send0(api, store, b"oldestRetainedBasis\0") };
        let policy_object = unsafe { send0(api, store, b"retentionPolicyCode\0") };
        let argument_object = unsafe { send0(api, store, b"retentionPolicyArgument\0") };
        let metadata = if [oldest_object, policy_object, argument_object]
            .iter()
            .all(|value| unsafe { (api.isSmallInt)(*value) } != 0)
        {
            let oldest = unsafe { (api.smallIntValue)(oldest_object) };
            let policy = unsafe { (api.smallIntValue)(policy_object) };
            let argument = unsafe { (api.smallIntValue)(argument_object) };
            let retention = match (policy, argument) {
                (0, 0) => Some(v6::RetentionPolicy::CurrentOnly),
                (1, value) if value >= 0 => Some(v6::RetentionPolicy::KeepSince(value as u64)),
                (2, value) if value >= 0 => Some(v6::RetentionPolicy::KeepLast(value as u64)),
                _ => None,
            };
            (oldest >= 0)
                .then_some(oldest as u64)
                .zip(retention)
                .and_then(|(oldest, retention)| v6::FactsWriter::new(basis, oldest, retention).ok())
        } else {
            None
        };
        let Some(facts_writer) = metadata else {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        };
        let collector = unsafe { (api.alloc)(collector_vt, 0) };
        if collector.is_null() {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        {
            let mut collection = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *collection = Some(FactCollection {
                writer: facts_writer,
                heap: HeapPreflight::new(authority),
                kept: 0,
                recovered: 0,
                recoveries_needing_code: 0,
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
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        };
        let facts_payload = collection.writer.finish();
        if let Some(bindings) = portable_bindings.as_deref() {
            for object in bindings
                .iter()
                .flat_map(|binding| [binding.target, binding.implementation, binding.provenance])
            {
                if unsafe { runtime_oop_is_inline(api, object) }
                    || unsafe { c_string_bytes((api.exportedNameOf)(object)) }.is_some()
                {
                    continue;
                }
                if unsafe { collection.heap.intern(api, object) }.is_none() {
                    collection.heap.rejected_slots =
                        collection.heap.rejected_slots.saturating_add(1);
                }
            }
        }
        let portable_heap = unsafe { collection.heap.finish_v7(api) };
        let heap_payload = portable_heap.as_ref().map(|(heap, _)| heap.clone());
        let portable_records = portable_heap.and_then(|(_, programs)| {
            let (bindings, selection) = unsafe {
                portable_binding_records(
                    api,
                    portable_bindings.as_deref()?,
                    &collection.heap,
                    &programs,
                )
            }?;
            Some(PortableSaveRecords {
                programs,
                bindings,
                selection,
            })
        });
        if let Ok(mut records) = PORTABLE_SAVE_RECORDS.lock() {
            *records = portable_records;
        }
        let heap_ok = heap_payload.is_some();
        let writer_ok = heap_payload
            .as_deref()
            .zip(PORTABLE_SAVE_RECORDS.lock().ok())
            .and_then(|(heap, records)| {
                let records = records.as_ref()?;
                v7::compose(v7::ImageSpec {
                    architecture: IMAGE_ARCH_TAG,
                    runtime_abi: runtime_abi_hash(),
                    programs: &records.programs,
                    bindings: &records.bindings,
                    selection: &records.selection,
                    heaps: &[heap],
                    facts: &[&facts_payload],
                    code: None,
                })
                .ok()
            })
            .is_some();
        FactPreflight {
            kept: collection.kept,
            recovered: collection.recovered,
            rejected: collection.rejected,
            facts_payload,
            heap_payload,
            heap_ok,
            recovered_slots: collection.heap.recovered_slots,
            rejected_slots: collection.heap.rejected_slots,
            writer_ok,
            image_rejection: collection.heap.image_rejection,
        }
    }

    unsafe extern "C" fn system_save_image(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
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
        let authority_digest = unsafe { current_generation_digest(api) };
        let preflight = unsafe { collect_store_facts(api, authority_digest) };
        if let Some(kind) = preflight.image_rejection {
            reject_save(kind.detail());
            record_fact_counts(
                preflight.kept,
                preflight.recovered,
                preflight.rejected,
                preflight.recovered_slots,
                preflight.rejected_slots,
            );
            runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .code_units = 0;
            return unsafe { image_rejection_error(api, kind) };
        }
        // New saves make the accepted clean v7 cut. Code is an authority-bound
        // mapped acceleration; canonical inventories remain sufficient for a
        // forced source-free rebuild.
        let portable_records = PORTABLE_SAVE_RECORDS
            .lock()
            .ok()
            .and_then(|mut records| records.take());
        let portable_code = portable_records
            .as_ref()
            .and_then(|records| unsafe { compile_portable_image_code(api, records) });
        let composed = preflight
            .heap_payload
            .as_deref()
            .zip(portable_records.as_ref())
            .zip(portable_code.as_deref())
            .and_then(|((heap, records), code)| {
                (preflight.rejected == 0
                    && preflight.image_rejection.is_none()
                    && preflight.heap_ok
                    && preflight.writer_ok)
                    .then(|| {
                        v7::compose(v7::ImageSpec {
                            architecture: IMAGE_ARCH_TAG,
                            runtime_abi: runtime_abi_hash(),
                            programs: &records.programs,
                            bindings: &records.bindings,
                            selection: &records.selection,
                            heaps: &[heap],
                            facts: &[&preflight.facts_payload],
                            code: Some(code),
                        })
                        .ok()
                    })
                    .flatten()
            });
        let publication_requested = rust_image_publication_selected();
        let canonical_publication = rust_image_canonical_selected() && !publication_requested;
        let publication_authorized = if publication_requested {
            rust_image_writer_authority_selected()
        } else {
            canonical_publication
        };
        let published = if publication_authorized {
            composed.as_ref().is_some_and(|bytes| {
                let path_bytes = unsafe { runtime_image_path_bytes(api, path) };
                path_bytes.is_some_and(|path_bytes| {
                    publish_atomic(Path::new(OsStr::from_bytes(&path_bytes)), bytes).is_ok()
                })
            })
        } else {
            false
        };
        if published {
            record_fact_counts(
                preflight.kept,
                preflight.recovered,
                preflight.rejected,
                preflight.recovered_slots,
                preflight.rejected_slots,
            );
            let mut report = runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            report.status = ReportStatus::Saved;
            report.detail_len = 0;
            report.code_units = 1;
            return self_object;
        }
        reject_save(if let Some(kind) = preflight.image_rejection {
            kind.detail()
        } else if !preflight.heap_ok {
            "Rust Image heap preflight rejected unsupported layout"
        } else if preflight.rejected != 0 {
            "Rust Image fact preflight rejected unsupported values"
        } else if !preflight.writer_ok {
            "Rust Image writer rejected preflight payload"
        } else if publication_requested && !publication_authorized {
            "Rust Image publication requires explicit Rust writer authority"
        } else if portable_records.is_none() {
            "Rust Image portable inventories are unavailable"
        } else if portable_code.is_none() {
            "Rust Image portable native Code compilation failed"
        } else if composed.is_none() {
            "Rust Image v7 composition failed"
        } else if publication_requested {
            "Rust Image atomic publication failed"
        } else {
            "Rust Image code mapping/rebuild orchestration is not installed"
        });
        record_fact_counts(
            preflight.kept,
            preflight.recovered,
            preflight.rejected,
            preflight.recovered_slots,
            preflight.rejected_slots,
        );
        {
            let mut report = runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            report.code_units = 0;
        }
        if let Some(kind) = preflight.image_rejection {
            unsafe { image_rejection_error(api, kind) }
        } else {
            api.nilObj
        }
    }

    unsafe extern "C" fn system_load_image(
        _self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if count != 1 || !unsafe { preflight_runtime_image(api, arguments[0]) } {
            return api.nilObj;
        }
        unsafe { load_with_rust_codegen(api, arguments[0], false) }
    }

    unsafe extern "C" fn system_load_image_rebuild(
        _self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if count != 2 || !unsafe { preflight_runtime_image(api, arguments[0]) } {
            return api.nilObj;
        }
        unsafe { load_with_rust_codegen(api, arguments[0], arguments[1] == api.trueObj) }
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
        let identity = IMAGE_COMPOSITION_IDENTITY
            .lock()
            .ok()
            .and_then(|identity| *identity);
        let Some(identity) = identity else {
            return api.nilObj;
        };
        unsafe { (api.mkString)(identity.as_ptr(), identity.len()) }
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

    unsafe extern "C" fn report_saved_facts(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
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

    unsafe extern "C" fn report_code_units(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
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

    unsafe extern "C" fn collector_collect(
        _self_object: Oop,
        _args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        if count != 6 {
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
        let historied_object = arguments[5];
        if unsafe { (api.isSmallInt)(tx_object) } == 0
            || unsafe { (api.isSmallInt)(op_object) } == 0
            || (historied_object != api.trueObj && historied_object != api.falseObj)
        {
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
        // A selected projected global is reconstructed by the generation's
        // mapped or freshly rebuilt image unit. Serializing the closure object
        // as a Store value would require its artifact-local vtable before code
        // recovery has run and would duplicate the authoritative initializer.
        let value_vtable = unsafe { (api.vtableOf)(arguments[2]) };
        let projected_global = unsafe { symbol_bytes(api, arguments[0]) }.as_deref()
            == Some(b"__globals__".as_slice())
            && unsafe {
                collection
                    .heap
                    .block_vtable_image_reference(api, value_vtable)
                    .is_some()
            };
        if projected_global {
            collection.recovered = collection.recovered.saturating_add(1);
            return api.nilObj;
        }
        let mut fields = FieldEncoder::new();
        let encoded = unsafe {
            encode_runtime_oop(api, &mut fields, arguments[0], &mut collection.heap)
                && encode_runtime_oop(api, &mut fields, arguments[1], &mut collection.heap)
                && encode_runtime_oop(api, &mut fields, arguments[2], &mut collection.heap)
        };
        let fact_written = encoded
            && collection
                .writer
                .fact_encoded(
                    historied_object == api.trueObj,
                    op as u8,
                    tx as u64,
                    fields.as_bytes(),
                )
                .is_ok();
        if !fact_written {
            if collection.heap.image_rejection.is_some() || encoded {
                collection.rejected = collection.rejected.saturating_add(1);
            } else {
                let store = unsafe { (api.import)(c"Store".as_ptr()) };
                match if store.is_null() || store == api.nilObj {
                    DeferredFactClass::Reject
                } else {
                    unsafe { deferred_fact_class(api, store, arguments[0], arguments[1]) }
                } {
                    DeferredFactClass::Source => {
                        collection.recovered = collection.recovered.saturating_add(1);
                    }
                    DeferredFactClass::Code => {
                        collection.recovered = collection.recovered.saturating_add(1);
                        collection.recoveries_needing_code =
                            collection.recoveries_needing_code.saturating_add(1);
                    }
                    DeferredFactClass::Reject => {
                        collection.rejected = collection.rejected.saturating_add(1);
                    }
                }
            }
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
        let report_vt =
            if unsafe { (api_ref.isExported)(IMAGE_REPORT_VT_NAME.as_ptr().cast()) } != 0 {
                unsafe { (api_ref.import)(IMAGE_REPORT_VT_NAME.as_ptr().cast()) }
            } else {
                let vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
                unsafe { (api_ref.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW) };
                vt
            };
        let report_obj = if unsafe { (api_ref.isExported)(IMAGE_REPORT_NAME.as_ptr().cast()) } != 0
        {
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
                intern(api_ref, b"e:a:v:tx:op:historied:\0"),
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
        ROOT_REGISTERED.get_or_init(|| unsafe {
            (api_ref.gcRegisterRootProvider)(mark_runtime_roots);
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

    /// Declare that this module treats IdApi Symbol-name pointers as borrowed.
    #[unsafe(no_mangle)]
    pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
        1
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;

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
    fn runtime_abi_v3_matches_the_frozen_registration_tuple() {
        assert_eq!(hash_registration_abi_tuple_v1(0).1, 1_539);
        assert_eq!(runtime_abi_hash(), 0x6c3b_67f0_ef16_14b3);
    }

    fn supported_preflight_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut facts = FactsWriter::new(17);
        facts
            .fact(
                1,
                42,
                Field::Symbol(b"Point"),
                Field::Symbol(b"value"),
                Field::Integer(7),
            )
            .unwrap();

        let mut heap = Vec::new();
        heap.extend_from_slice(&1u32.to_le_bytes());
        heap.extend_from_slice(&5u32.to_le_bytes());
        heap.extend_from_slice(b"Point");
        heap.extend_from_slice(&2u32.to_le_bytes());
        let mut oop = FieldEncoder::new();
        oop.integer(9);
        heap.extend_from_slice(oop.as_bytes());
        heap.extend_from_slice(&0xfeed_face_cafe_beefu64.to_le_bytes());
        (facts.finish(), heap)
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
        assert_eq!(authority_digest_from_bytes(&digest), Some(digest));
        assert_eq!(authority_digest_from_bytes(&digest[..63]), None);
        let bytes = compose_complete_image(
            1,
            99,
            b"facts",
            b"heap",
            &digest,
            wyhash(0, b"unit"),
            b"unit",
        )
        .unwrap();

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
                RecordKind::Authority,
                RecordKind::Heap,
                RecordKind::Facts,
                RecordKind::Code
            ]
        );
    }

    #[test]
    fn composed_supported_preflight_round_trips_v4_payloads() {
        let (facts_payload, heap_payload) = supported_preflight_fixture();
        let digest = [b'g'; AUTHORITY_DIGEST_LEN];
        let code = b"compiled-unit";
        let bytes = compose_complete_image(
            IMAGE_ARCH_TAG,
            runtime_abi_hash(),
            &facts_payload,
            &heap_payload,
            &digest,
            wyhash(0, code),
            code,
        )
        .unwrap();

        let image = Image::preflight(&bytes, IMAGE_ARCH_TAG, runtime_abi_hash()).unwrap();
        assert_eq!(image.authority.digest, &digest);
        assert_eq!(image.code().unwrap().unit, code);

        let mut records = image.records();
        let authority = records.next_record().unwrap().unwrap();
        assert_eq!(authority.kind, RecordKind::Authority);
        assert_eq!(authority.payload, digest);

        let heap = records.next_record().unwrap().unwrap();
        assert_eq!(heap.kind, RecordKind::Heap);
        let mut objects = Heap::parse(heap.payload).unwrap();
        assert_eq!(
            objects.peek_header().unwrap(),
            Some(HeapHeader {
                vtable_name: b"Point",
                word_count: 2,
            })
        );
        let object = objects
            .next_object(&[SlotKind::Oop, SlotKind::RawWord])
            .unwrap()
            .unwrap();
        assert_eq!(object.header.vtable_name, b"Point");
        assert_eq!(object.header.word_count, 2);
        let mut fields = Fields::new(object.encoded_slots);
        assert_eq!(fields.next_field().unwrap(), Some(Field::Integer(9)));
        assert_eq!(fields.consumed(), 9);
        assert_eq!(
            objects
                .next_object(&[SlotKind::Oop, SlotKind::RawWord])
                .unwrap(),
            None
        );

        let facts = records.next_record().unwrap().unwrap();
        assert_eq!(facts.kind, RecordKind::Facts);
        let mut facts = Facts::parse(facts.payload).unwrap();
        assert_eq!(facts.basis, 17);
        assert_eq!(facts.next_fact().unwrap().unwrap().value, Field::Integer(7));
        assert_eq!(facts.next_fact().unwrap(), None);

        let code_record = records.next_record().unwrap().unwrap();
        assert_eq!(code_record.kind, RecordKind::Code);
        assert_eq!(records.next_record().unwrap(), None);
    }

    #[test]
    fn image_path_admission_is_bounded_and_read_only() {
        let (facts_payload, heap_payload) = supported_preflight_fixture();
        let digest = [b'a'; AUTHORITY_DIGEST_LEN];
        let bytes = compose_complete_image(
            IMAGE_ARCH_TAG,
            runtime_abi_hash(),
            &facts_payload,
            &heap_payload,
            &digest,
            wyhash(0, b"unit"),
            b"unit",
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!(
            "birnam-rust-image-admission-{}-{}",
            std::process::id(),
            codec::TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, &bytes).unwrap();
        assert!(runtime::preflight_image_path(&path));
        fs::write(&path, b"not an image").unwrap();
        assert!(!runtime::preflight_image_path(&path));
        fs::remove_file(path).unwrap();
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
            codec::TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
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
            codec::TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
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
    fn code_checksum_matches_the_wyhash_vectors() {
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
        let image = Image::preflight(&bytes, 1, 99).unwrap();
        assert_eq!(image.header.architecture, 1);
        assert_eq!(image.authority.digest, &digest);
        assert_eq!(image.code(), None);

        let mut code_payload = wyhash(0, b"unit").to_le_bytes().to_vec();
        code_payload.extend_from_slice(&digest);
        code_payload.extend_from_slice(b"unit");
        bytes.extend_from_slice(&record(RecordKind::Code, &code_payload));
        let image = Image::preflight(&bytes, 1, 99).unwrap();
        assert_eq!(image.code().unwrap().unit, b"unit");
        assert_eq!(
            image.records().next_record().unwrap().unwrap().kind,
            RecordKind::Authority
        );
        assert!(matches!(
            Image::preflight(&bytes, 2, 99),
            Err(Error::ArchitectureMismatch)
        ));
        assert!(matches!(
            Image::preflight(&bytes, 1, 100),
            Err(Error::AbiMismatch)
        ));
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
