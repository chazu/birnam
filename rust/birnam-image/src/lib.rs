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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Truncated,
    MagicMismatch,
    UnsupportedVersion,
    EndianMismatch,
    ArchitectureMismatch,
    AbiMismatch,
    InvalidRecordKind,
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
}
