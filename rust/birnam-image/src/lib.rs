//! Borrowed framing primitives for the portable Birnam v4 image.
//!
//! This first image slice deliberately stops at the header and record envelope.
//! It owns no `IdApi` values, performs no allocation, and does not interpret
//! heap or Store payloads. Higher layers can therefore reject malformed bytes
//! before materializing any runtime state.

pub const MAGIC: &[u8; 8] = b"BIRNAMIM";
pub const FORMAT_VERSION: u32 = 4;
pub const ENDIAN_TAG: u32 = 0x0102_0304;
pub const HEADER_LEN: usize = 28;
pub const RECORD_HEADER_LEN: usize = 12;
pub const AUTHORITY_DIGEST_LEN: usize = 64;
pub const CODE_PREFIX_LEN: usize = 8 + AUTHORITY_DIGEST_LEN;
pub const FIELD_LENGTH_PREFIX: usize = 4;

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

#[derive(Debug, Clone, Copy)]
pub struct Records<'a> {
    bytes: &'a [u8],
    offset: usize,
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

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
