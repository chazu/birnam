//! Current retained-fact encoding shared by image-v8 save and load.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionPolicy {
    CurrentOnly,
    KeepSince(u64),
    KeepLast(u64),
}

impl RetentionPolicy {
    fn code(self) -> u8 {
        match self {
            Self::CurrentOnly => 0,
            Self::KeepSince(_) => 1,
            Self::KeepLast(_) => 2,
        }
    }

    fn argument(self) -> u64 {
        match self {
            Self::CurrentOnly => 0,
            Self::KeepSince(value) | Self::KeepLast(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
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
    ) -> Result<Self, Error> {
        if oldest_retained_basis > basis
            || matches!(retention, RetentionPolicy::KeepSince(value) if value > basis)
        {
            return Err(Error::InvalidMetadata);
        }
        let mut bytes = Vec::with_capacity(36);
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
    ) -> Result<(), Error> {
        if operation > 1 || encoded_value_count(encoded_fields) != Some(3) {
            return Err(Error::InvalidFields);
        }
        let count = self.count.checked_add(1).ok_or(Error::PayloadTooLarge)?;
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

fn encoded_value_count(bytes: &[u8]) -> Option<usize> {
    let mut offset = 0usize;
    let mut count = 0usize;
    while offset < bytes.len() {
        let tag = *bytes.get(offset)?;
        offset = offset.checked_add(1)?;
        match tag {
            0 | 1 | 2 | 8 => {}
            3 | 4 => offset = offset.checked_add(8)?,
            5 | 6 | 9 => {
                let length_end = offset.checked_add(4)?;
                let length =
                    u32::from_le_bytes(bytes.get(offset..length_end)?.try_into().ok()?) as usize;
                offset = length_end.checked_add(length)?;
            }
            7 | 11 | 12 => offset = offset.checked_add(4)?,
            _ => return None,
        }
        if offset > bytes.len() {
            return None;
        }
        count = count.checked_add(1)?;
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exactly_three_current_values() {
        let mut writer = FactsWriter::new(41, 17, RetentionPolicy::KeepLast(24)).unwrap();
        let fields = [0, 3, 1, 0, 0, 0, 0, 0, 0, 0, 12, 7, 0, 0, 0];
        writer.fact_encoded(true, 0, 40, &fields).unwrap();
        assert_eq!(
            u32::from_le_bytes(writer.finish()[32..36].try_into().unwrap()),
            1
        );
    }

    #[test]
    fn rejects_obsolete_or_malformed_value_tags() {
        let mut writer = FactsWriter::new(0, 0, RetentionPolicy::CurrentOnly).unwrap();
        assert_eq!(
            writer.fact_encoded(false, 0, 0, &[10, 0, 0, 0, 0]),
            Err(Error::InvalidFields)
        );
    }
}
