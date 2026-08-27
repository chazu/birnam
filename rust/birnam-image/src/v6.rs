//! Image-v6 framing for prospective Store history and durable retention.
//!
//! The outer authority, resumable, heap, and code records are inherited from
//! image-v5. Only the header version and Facts payload advance: v6 adds the
//! retention floor/policy and one commit-time history bit per fact.

pub use super::v5::{
    AUTHORITY_BYTES, Authority, Code, Field, Fields, Heap, Image, Record, RecordKind, Records,
    Resumables, SlotKind, VtableLocator,
};

pub const FORMAT_VERSION: u32 = 6;
pub const FACTS_HEADER_BYTES: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionPolicy {
    CurrentOnly,
    KeepSince(u64),
    KeepLast(u64),
}

impl RetentionPolicy {
    pub fn code(self) -> u8 {
        match self {
            Self::CurrentOnly => 0,
            Self::KeepSince(_) => 1,
            Self::KeepLast(_) => 2,
        }
    }

    pub fn argument(self) -> u64 {
        match self {
            Self::CurrentOnly => 0,
            Self::KeepSince(value) | Self::KeepLast(value) => value,
        }
    }

    fn parse(code: u8, argument: u64, basis: u64) -> Result<Self, super::v5::Error> {
        match code {
            0 if argument == 0 => Ok(Self::CurrentOnly),
            1 if argument <= basis => Ok(Self::KeepSince(argument)),
            2 => Ok(Self::KeepLast(argument)),
            _ => Err(super::v5::Error::RecordLength),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact<'a> {
    pub historied: bool,
    pub operation: u8,
    pub transaction: u64,
    pub entity: Field<'a>,
    pub attribute: Field<'a>,
    pub value: Field<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub basis: u64,
    pub oldest_retained_basis: u64,
    pub retention: RetentionPolicy,
    remaining: u32,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Facts<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, super::v5::Error> {
        if payload.len() < FACTS_HEADER_BYTES {
            return Err(super::v5::Error::RecordLength);
        }
        let basis = u64::from_le_bytes(payload[0..8].try_into().expect("fixed basis"));
        let oldest_retained_basis =
            u64::from_le_bytes(payload[8..16].try_into().expect("fixed floor"));
        if oldest_retained_basis > basis || payload[17..24].iter().any(|byte| *byte != 0) {
            return Err(super::v5::Error::RecordLength);
        }
        let argument = u64::from_le_bytes(payload[24..32].try_into().expect("fixed argument"));
        let retention = RetentionPolicy::parse(payload[16], argument, basis)?;
        let remaining = u32::from_le_bytes(payload[32..36].try_into().expect("fixed count"));
        Ok(Self {
            basis,
            oldest_retained_basis,
            retention,
            remaining,
            bytes: &payload[FACTS_HEADER_BYTES..],
            offset: 0,
        })
    }

    pub fn next_fact(&mut self) -> Result<Option<Fact<'a>>, super::v5::Error> {
        if self.remaining == 0 {
            return if self.offset == self.bytes.len() {
                Ok(None)
            } else {
                Err(super::v5::Error::RecordLength)
            };
        }
        let prefix_end = self
            .offset
            .checked_add(10)
            .ok_or(super::v5::Error::Arithmetic)?;
        let prefix = self
            .bytes
            .get(self.offset..prefix_end)
            .ok_or(super::v5::Error::RecordLength)?;
        let historied = match prefix[0] {
            0 => false,
            1 => true,
            _ => return Err(super::v5::Error::RecordLength),
        };
        let operation = prefix[1];
        if operation > 1 {
            return Err(super::v5::Error::RecordLength);
        }
        let transaction = u64::from_le_bytes(prefix[2..10].try_into().expect("fixed tx"));
        let mut fields = Fields::new(&self.bytes[prefix_end..]);
        let entity = fields.next_field()?.ok_or(super::v5::Error::RecordLength)?;
        let attribute = fields.next_field()?.ok_or(super::v5::Error::RecordLength)?;
        let value = fields.next_field()?.ok_or(super::v5::Error::RecordLength)?;
        self.offset = prefix_end
            .checked_add(fields.consumed())
            .ok_or(super::v5::Error::Arithmetic)?;
        self.remaining -= 1;
        Ok(Some(Fact {
            historied,
            operation,
            transaction,
            entity,
            attribute,
            value,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactsEncodeError {
    InvalidFields,
    PayloadTooLarge,
    InvalidMetadata,
}

pub struct FactsWriter {
    bytes: Vec<u8>,
    count: u32,
}

impl FactsWriter {
    pub fn new(
        basis: u64,
        oldest_retained_basis: u64,
        retention: RetentionPolicy,
    ) -> Result<Self, FactsEncodeError> {
        if oldest_retained_basis > basis
            || matches!(retention, RetentionPolicy::KeepSince(value) if value > basis)
        {
            return Err(FactsEncodeError::InvalidMetadata);
        }
        let mut bytes = Vec::with_capacity(FACTS_HEADER_BYTES);
        bytes.extend_from_slice(&basis.to_le_bytes());
        bytes.extend_from_slice(&oldest_retained_basis.to_le_bytes());
        bytes.push(retention.code());
        bytes.extend_from_slice(&[0; 7]);
        bytes.extend_from_slice(&retention.argument().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        Ok(Self { bytes, count: 0 })
    }

    pub fn fact_encoded(
        &mut self,
        historied: bool,
        operation: u8,
        transaction: u64,
        encoded_fields: &[u8],
    ) -> Result<(), FactsEncodeError> {
        if operation > 1 {
            return Err(FactsEncodeError::InvalidFields);
        }
        let mut fields = Fields::new(encoded_fields);
        for _ in 0..3 {
            if fields
                .next_field()
                .map_err(|_| FactsEncodeError::InvalidFields)?
                .is_none()
            {
                return Err(FactsEncodeError::InvalidFields);
            }
        }
        if fields
            .next_field()
            .map_err(|_| FactsEncodeError::InvalidFields)?
            .is_some()
        {
            return Err(FactsEncodeError::InvalidFields);
        }
        let count = self
            .count
            .checked_add(1)
            .ok_or(FactsEncodeError::PayloadTooLarge)?;
        self.bytes.push(u8::from(historied));
        self.bytes.push(operation);
        self.bytes.extend_from_slice(&transaction.to_le_bytes());
        self.bytes.extend_from_slice(encoded_fields);
        self.count = count;
        self.bytes[32..36].copy_from_slice(&count.to_le_bytes());
        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

pub fn compose_complete_image(
    architecture: u32,
    runtime_abi: u64,
    facts: &[u8],
    heap: &[u8],
    authority: &[u8; AUTHORITY_BYTES],
    manifest: &[u8],
    code_checksum: u64,
    code_unit: &[u8],
) -> Result<Vec<u8>, super::v5::EncodeError> {
    super::v5::compose_complete_image_version(
        architecture,
        runtime_abi,
        FORMAT_VERSION,
        facts,
        heap,
        authority,
        manifest,
        code_checksum,
        code_unit,
    )
}

#[cfg(test)]
mod tests {
    use super::super::codec::FieldEncoder;
    use super::{Facts, FactsWriter, RetentionPolicy};

    #[test]
    fn facts_round_trip_history_and_retention_metadata() {
        let mut writer = FactsWriter::new(41, 17, RetentionPolicy::KeepLast(24)).unwrap();
        let mut fields = FieldEncoder::new();
        fields.integer(1);
        fields.integer(2);
        fields.integer(3);
        writer.fact_encoded(true, 0, 40, fields.as_bytes()).unwrap();
        let payload = writer.finish();

        let mut facts = Facts::parse(&payload).unwrap();
        assert_eq!(facts.basis, 41);
        assert_eq!(facts.oldest_retained_basis, 17);
        assert_eq!(facts.retention, RetentionPolicy::KeepLast(24));
        let fact = facts.next_fact().unwrap().unwrap();
        assert!(fact.historied);
        assert_eq!(fact.transaction, 40);
        assert_eq!(facts.next_fact().unwrap(), None);
    }
}
