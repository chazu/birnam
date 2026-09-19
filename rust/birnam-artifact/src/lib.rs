//! Allocation-free parser for the pointer-free Birnam artifact descriptor.
//!
//! This crate deliberately knows nothing about `IdApi`, `dlopen`, object
//! allocation, or activation policy. It returns slices borrowed from the input
//! artifact and rejects malformed framing before a higher rung can materialize
//! any Birnam value.

pub mod candidate;
pub mod code_binding_inventory;
pub mod digest;
pub mod frame_descriptor;
pub mod materialization_selection;
#[cfg(feature = "module")]
mod module;
pub mod portable_inventory;
pub mod portable_program;
pub mod portable_resumable_manifest;
pub mod resumable_manifest;
pub mod stackless_selection;
pub mod selector;
mod wire;

pub const ABI: &[u8] = b"birnam-artifact-generation-v1";
pub const TARGET: &[u8] = b"aarch64-macos";
pub const DESCRIPTOR_MAGIC: &[u8] = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1";
pub const MAX_ARTIFACT_BYTES: usize = 1 << 30;
pub const MAX_LIST_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    ArtifactTooLarge,
    MissingDescriptor,
    MultipleDescriptors,
    TruncatedDescriptor,
    InvalidDescriptor,
    ListTooLarge,
    AbiMismatch,
    TargetMismatch,
    EmptyIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct List<'a> {
    encoded: &'a [u8],
    count: usize,
}

impl<'a> List<'a> {
    pub const fn count(self) -> usize {
        self.count
    }

    /// Return the length-delimited field bytes after the list count.
    ///
    /// The inspector uses this borrowed wire projection when it materializes
    /// the existing Birnam descriptor object. The parser still owns no state
    /// and performs no allocation.
    pub const fn encoded_bytes(self) -> &'a [u8] {
        self.encoded
    }

    pub fn get(self, index: usize) -> Option<&'a [u8]> {
        if index >= self.count {
            return None;
        }
        let mut reader = Reader::new(self.encoded);
        for current in 0..=index {
            let field = reader.field()?;
            if current == index {
                return Some(field);
            }
        }
        None
    }

    pub fn iter(self) -> ListIter<'a> {
        ListIter {
            reader: Reader::new(self.encoded),
            remaining: self.count,
        }
    }
}

pub struct ListIter<'a> {
    reader: Reader<'a>,
    remaining: usize,
}

impl<'a> Iterator for ListIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let field = self.reader.field()?;
        self.remaining -= 1;
        Some(field)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor<'a> {
    pub kind: &'a [u8],
    pub identity: &'a [u8],
    pub artifact_abi: &'a [u8],
    pub target: &'a [u8],
    pub source_digest: &'a [u8],
    pub revision_digest: &'a [u8],
    pub requires: List<'a>,
    pub native_requires: List<'a>,
    pub capabilities: List<'a>,
}

impl Descriptor<'_> {
    pub fn validate_compatibility(self) -> Result<Self, Error> {
        if self.artifact_abi != ABI {
            return Err(Error::AbiMismatch);
        }
        if self.target != TARGET {
            return Err(Error::TargetMismatch);
        }
        if self.identity.is_empty() {
            return Err(Error::EmptyIdentity);
        }
        Ok(self)
    }
}

/// Parse one complete inert descriptor and validate its ABI/target contract.
pub fn inspect(bytes: &[u8]) -> Result<Descriptor<'_>, Error> {
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::ArtifactTooLarge);
    }

    let mut position = 0;
    let mut found = None;
    while let Some(relative) = bytes[position..]
        .windows(DESCRIPTOR_MAGIC.len())
        .position(|window| window == DESCRIPTOR_MAGIC)
    {
        let start = position + relative;
        if let Ok(descriptor) = parse_at(bytes, start) {
            if found.is_some() {
                return Err(Error::MultipleDescriptors);
            }
            found = Some(descriptor);
        }
        position = start.saturating_add(DESCRIPTOR_MAGIC.len());
        if position >= bytes.len() {
            break;
        }
    }

    found
        .ok_or(Error::MissingDescriptor)?
        .validate_compatibility()
}

fn parse_at(bytes: &[u8], start: usize) -> Result<Descriptor<'_>, Error> {
    let header_end = start
        .checked_add(DESCRIPTOR_MAGIC.len())
        .and_then(|value| value.checked_add(4))
        .ok_or(Error::TruncatedDescriptor)?;
    if header_end > bytes.len() {
        return Err(Error::TruncatedDescriptor);
    }
    let payload_len = u32::from_le_bytes(
        bytes[start + DESCRIPTOR_MAGIC.len()..header_end]
            .try_into()
            .map_err(|_| Error::InvalidDescriptor)?,
    ) as usize;
    let payload_end = header_end
        .checked_add(payload_len)
        .ok_or(Error::TruncatedDescriptor)?;
    if payload_end > bytes.len() {
        return Err(Error::TruncatedDescriptor);
    }

    let mut reader = Reader::new(&bytes[header_end..payload_end]);
    let kind = reader.field().ok_or(Error::InvalidDescriptor)?;
    let identity = reader.field().ok_or(Error::InvalidDescriptor)?;
    let artifact_abi = reader.field().ok_or(Error::InvalidDescriptor)?;
    let target = reader.field().ok_or(Error::InvalidDescriptor)?;
    let source_digest = reader.field().ok_or(Error::InvalidDescriptor)?;
    let revision_digest = reader.field().ok_or(Error::InvalidDescriptor)?;
    let requires = reader.list()?;
    let native_requires = reader.list()?;
    let capabilities = reader.list()?;
    if !reader.is_empty() {
        return Err(Error::InvalidDescriptor);
    }
    Ok(Descriptor {
        kind,
        identity,
        artifact_abi,
        target,
        source_digest,
        revision_digest,
        requires,
        native_requires,
        capabilities,
    })
}

#[derive(Clone, Copy)]
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn field(&mut self) -> Option<&'a [u8]> {
        let length = self.u32()?;
        let end = self.position.checked_add(length)?;
        let value = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(value)
    }

    fn list(&mut self) -> Result<List<'a>, Error> {
        let count = self.u32().ok_or(Error::InvalidDescriptor)?;
        if count > MAX_LIST_ENTRIES {
            return Err(Error::ListTooLarge);
        }
        let start = self.position;
        for _ in 0..count {
            self.field().ok_or(Error::InvalidDescriptor)?;
        }
        Ok(List {
            encoded: &self.bytes[start..self.position],
            count,
        })
    }

    fn u32(&mut self) -> Option<usize> {
        let end = self.position.checked_add(4)?;
        let value = u32::from_le_bytes(self.bytes.get(self.position..end)?.try_into().ok()?);
        self.position = end;
        Some(value as usize)
    }

    const fn is_empty(self) -> bool {
        self.position == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(value: &[u8], output: &mut Vec<u8>) {
        output.extend_from_slice(&(value.len() as u32).to_le_bytes());
        output.extend_from_slice(value);
    }

    fn list(values: &[&[u8]], output: &mut Vec<u8>) {
        output.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for value in values {
            field(value, output);
        }
    }

    fn fixture(abi: &[u8], target: &[u8], native: &[&[u8]]) -> Vec<u8> {
        let mut payload = Vec::new();
        field(b"module", &mut payload);
        field(b"candidate", &mut payload);
        field(abi, &mut payload);
        field(target, &mut payload);
        field(b"source-digest", &mut payload);
        field(b"revision-digest", &mut payload);
        list(&[], &mut payload);
        list(native, &mut payload);
        list(&[b"activate"], &mut payload);

        let mut bytes = b"sentinel initializer: MUST NOT EXECUTE\n".to_vec();
        bytes.extend_from_slice(DESCRIPTOR_MAGIC);
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }

    #[test]
    fn valid_descriptor_is_borrowed_and_lists_are_deterministic() {
        let bytes = fixture(ABI, TARGET, &[b"libdep.dylib"]);
        let descriptor = inspect(&bytes).unwrap();
        assert_eq!(descriptor.kind, b"module");
        assert_eq!(descriptor.identity, b"candidate");
        assert_eq!(descriptor.native_requires.count(), 1);
        assert_eq!(
            descriptor.native_requires.get(0),
            Some(&b"libdep.dylib"[..])
        );
        assert_eq!(
            descriptor.capabilities.iter().collect::<Vec<_>>(),
            vec![&b"activate"[..]]
        );
    }

    #[test]
    fn compatibility_mismatches_are_categorized() {
        assert_eq!(
            inspect(&fixture(b"wrong", TARGET, &[])),
            Err(Error::AbiMismatch)
        );
        assert_eq!(
            inspect(&fixture(ABI, b"wrong-target", &[])),
            Err(Error::TargetMismatch)
        );
    }

    #[test]
    fn malformed_and_oversized_framing_fails_closed() {
        assert_eq!(inspect(b"garbage"), Err(Error::MissingDescriptor));
        // The host-visible category for an unparseable magic candidate is the
        // existing artifactDescriptionMissing result; malformed candidates do
        // not become usable merely because their magic is present.
        assert_eq!(inspect(DESCRIPTOR_MAGIC), Err(Error::MissingDescriptor));

        let mut too_many = fixture(ABI, TARGET, &[]);
        let count_offset = too_many
            .windows(DESCRIPTOR_MAGIC.len())
            .position(|window| window == DESCRIPTOR_MAGIC)
            .unwrap()
            + DESCRIPTOR_MAGIC.len()
            + 4;
        // Skip six fields, then the requires-list count.
        let mut cursor = count_offset;
        for _ in 0..6 {
            let length =
                u32::from_le_bytes(too_many[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4 + length;
        }
        too_many[cursor..cursor + 4]
            .copy_from_slice(&((MAX_LIST_ENTRIES + 1) as u32).to_le_bytes());
        // The wire-compatible host category remains description-missing for
        // any structurally unusable candidate, including an oversized list.
        assert_eq!(inspect(&too_many), Err(Error::MissingDescriptor));
    }

    #[test]
    fn duplicate_valid_descriptors_are_rejected() {
        let first = fixture(ABI, TARGET, &[]);
        let mut bytes = first.clone();
        bytes.extend_from_slice(&first);
        assert_eq!(inspect(&bytes), Err(Error::MultipleDescriptors));
    }

    #[test]
    fn adversarial_inputs_never_panic_or_allocate() {
        let mut bytes = [0u8; 257];
        for seed in 0u32..10_000 {
            let mut value = seed.wrapping_mul(0x9e37_79b9);
            for byte in &mut bytes {
                value ^= value << 13;
                value ^= value >> 17;
                value ^= value << 5;
                *byte = value as u8;
            }
            let _ = inspect(&bytes);
        }
    }
}

#[cfg(feature = "module")]
birnam_abi::integer_dependency!();
