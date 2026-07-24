//! Allocation-free parser for the pointer-free candidate sidecar envelope.
//!
//! Candidate mapping and publication are deliberately outside this module. It
//! only validates the detached bytes produced by the frozen Codegen contract,
//! so a later rung can inspect a candidate before opening its dylib.

pub const CANDIDATE_MAGIC: &[u8] = b"BIRNAM-CANDIDATE-2";
pub const CANDIDATE_ABI: u32 = 3;
pub const CANDIDATE_PLATFORM: &[u8] = b"aarch64-macos";
pub const MAX_CANDIDATE_BYTES: usize = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    CandidateTooLarge,
    MissingField,
    InvalidField,
    AbiMismatch,
    PlatformMismatch,
    CandidateAbiMismatch,
    InvalidDigest,
    SourceLengthMismatch,
    ManifestInvalid,
    ManifestDigestInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope<'a> {
    pub artifact_digest: &'a [u8],
    pub runtime_abi: &'a [u8],
    pub candidate_abi: u32,
    pub platform: &'a [u8],
    pub manifest: &'a [u8],
    pub source: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest<'a> {
    pub name: &'a [u8],
    pub source_digest: &'a [u8],
    pub non_method_digest: &'a [u8],
    pub method_digest: &'a [u8],
    pub revision_digest: &'a [u8],
    pub dependencies: &'a [u8],
    pub operations: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation<'a> {
    pub target: &'a [u8],
    pub selector: &'a [u8],
    pub install: bool,
    pub arity: u32,
}

fn segment<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let rest = bytes.get(*cursor..)?;
    let end = rest.iter().position(|byte| *byte == b'|')?;
    let value = &rest[..end];
    *cursor = cursor.checked_add(end + 1)?;
    Some(value)
}

fn token(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| *byte >= 0x21 && *byte != b'|' && *byte != b',' && *byte != b';')
}

/// Parse the candidate's pipe-delimited manifest without allocating.
pub fn inspect_manifest(bytes: &[u8]) -> Result<Manifest<'_>, Error> {
    let mut cursor = 0;
    let version = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let runtime = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let name = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let source_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let non_method_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let method_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let revision_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let dependencies = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let operations = bytes.get(cursor..).ok_or(Error::ManifestInvalid)?;
    if version != b"v2" || runtime != crate::ABI || !token(name) {
        return Err(Error::ManifestInvalid);
    }
    for digest_value in [
        source_digest,
        non_method_digest,
        method_digest,
        revision_digest,
    ] {
        if !digest(digest_value) {
            return Err(Error::ManifestDigestInvalid);
        }
    }
    if !list_tokens(dependencies, b',') || !operation_rows(operations) {
        return Err(Error::ManifestInvalid);
    }
    Ok(Manifest {
        name,
        source_digest,
        non_method_digest,
        method_digest,
        revision_digest,
        dependencies,
        operations,
    })
}

fn list_tokens(bytes: &[u8], separator: u8) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == separator {
            if !token(&bytes[start..index]) {
                return false;
            }
            start = index + 1;
        }
    }
    token(&bytes[start..])
}

fn operation_rows(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b';' {
            if parse_operation(&bytes[start..index]).is_none() {
                return false;
            }
            start = index + 1;
        }
    }
    parse_operation(&bytes[start..]).is_some()
}

fn parse_operation(bytes: &[u8]) -> Option<Operation<'_>> {
    let mut fields = bytes.split(|byte| *byte == b',');
    let target = fields.next()?;
    let selector = fields.next()?;
    let kind = fields.next()?;
    let arity = decimal(fields.next()?)?;
    if fields.next().is_some() || !token(target) || !token(selector) || kind.len() != 1 {
        return None;
    }
    let install = match kind[0] {
        b'I' => true,
        b'R' => false,
        _ => return None,
    };
    Some(Operation {
        target,
        selector,
        install,
        arity,
    })
}

impl<'a> Manifest<'a> {
    pub fn operation_count(self) -> usize {
        self.operations.split(|byte| *byte == b';').count()
    }

    pub fn operation(self, index: usize) -> Option<Operation<'a>> {
        self.operations
            .split(|byte| *byte == b';')
            .nth(index)
            .and_then(parse_operation)
    }
}

fn line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let rest = bytes.get(*cursor..)?;
    let end = rest.iter().position(|byte| *byte == b'\n')?;
    let value = &rest[..end];
    *cursor = cursor.checked_add(end + 1)?;
    Some(value)
}

fn field<'a>(bytes: &'a [u8], cursor: &mut usize, name: &[u8]) -> Option<&'a [u8]> {
    line(bytes, cursor)?.strip_prefix(name)
}

fn decimal(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(*byte - b'0'))?;
    }
    Some(value)
}

fn digest(bytes: &[u8]) -> bool {
    bytes.len() == 64 && bytes.iter().all(u8::is_ascii_hexdigit)
}

/// Parse one complete candidate envelope without allocating or opening code.
pub fn inspect(bytes: &[u8]) -> Result<Envelope<'_>, Error> {
    if bytes.len() > MAX_CANDIDATE_BYTES {
        return Err(Error::CandidateTooLarge);
    }
    let mut cursor = 0;
    if line(bytes, &mut cursor) != Some(CANDIDATE_MAGIC) {
        return Err(Error::InvalidField);
    }
    let artifact_digest = field(bytes, &mut cursor, b"artifact=").ok_or(Error::MissingField)?;
    if !digest(artifact_digest) {
        return Err(Error::InvalidDigest);
    }
    let runtime_abi = field(bytes, &mut cursor, b"runtime=").ok_or(Error::MissingField)?;
    if runtime_abi != crate::ABI {
        return Err(Error::AbiMismatch);
    }
    let candidate_abi =
        decimal(field(bytes, &mut cursor, b"candidate=").ok_or(Error::MissingField)?)
            .ok_or(Error::InvalidField)?;
    if candidate_abi != CANDIDATE_ABI {
        return Err(Error::CandidateAbiMismatch);
    }
    let platform = field(bytes, &mut cursor, b"platform=").ok_or(Error::MissingField)?;
    if platform != CANDIDATE_PLATFORM {
        return Err(Error::PlatformMismatch);
    }
    let manifest = field(bytes, &mut cursor, b"manifest=").ok_or(Error::MissingField)?;
    if manifest.is_empty() || manifest.contains(&b'\n') {
        return Err(Error::InvalidField);
    }
    let source_length = decimal(field(bytes, &mut cursor, b"source=").ok_or(Error::MissingField)?)
        .ok_or(Error::InvalidField)? as usize;
    if source_length > MAX_CANDIDATE_BYTES {
        return Err(Error::SourceLengthMismatch);
    }
    let source = bytes.get(cursor..).ok_or(Error::SourceLengthMismatch)?;
    if source.len() != source_length {
        return Err(Error::SourceLengthMismatch);
    }
    Ok(Envelope {
        artifact_digest,
        runtime_abi,
        candidate_abi,
        platform,
        manifest,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(source: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(CANDIDATE_MAGIC);
        bytes.push(b'\n');
        bytes.extend_from_slice(b"artifact=");
        bytes
            .extend_from_slice(b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        bytes.push(b'\n');
        bytes.extend_from_slice(b"runtime=");
        bytes.extend_from_slice(crate::ABI);
        bytes.push(b'\n');
        bytes.extend_from_slice(
            b"candidate=3\nplatform=aarch64-macos\nmanifest=v2|runtime|candidate|digest\nsource=",
        );
        bytes.extend_from_slice(source.len().to_string().as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(source);
        bytes
    }

    #[test]
    fn valid_envelope_is_borrowed() {
        let bytes = fixture(b"module source");
        let envelope = inspect(&bytes).unwrap();
        assert_eq!(envelope.artifact_digest.len(), 64);
        assert_eq!(envelope.runtime_abi, crate::ABI);
        assert_eq!(envelope.candidate_abi, CANDIDATE_ABI);
        assert_eq!(envelope.platform, CANDIDATE_PLATFORM);
        assert_eq!(envelope.manifest, b"v2|runtime|candidate|digest");
        assert_eq!(envelope.source, b"module source");
    }

    #[test]
    fn contract_mismatches_fail_closed() {
        let mut bytes = fixture(b"source");
        let runtime = bytes
            .windows(crate::ABI.len())
            .position(|window| window == crate::ABI)
            .unwrap();
        bytes[runtime] = b'X';
        assert_eq!(inspect(&bytes), Err(Error::AbiMismatch));

        let mut bytes = fixture(b"source");
        let platform = bytes
            .windows(CANDIDATE_PLATFORM.len())
            .position(|window| window == CANDIDATE_PLATFORM)
            .unwrap();
        bytes[platform] = b'X';
        assert_eq!(inspect(&bytes), Err(Error::PlatformMismatch));

        let mut bytes = fixture(b"source");
        let candidate = bytes
            .windows(b"candidate=3".len())
            .position(|window| window == b"candidate=3")
            .unwrap();
        bytes[candidate + b"candidate=".len()] = b'4';
        assert_eq!(inspect(&bytes), Err(Error::CandidateAbiMismatch));
    }

    #[test]
    fn framing_and_digest_fail_closed() {
        assert_eq!(inspect(b"garbage"), Err(Error::InvalidField));
        let mut bytes = fixture(b"source");
        let digest = bytes
            .windows(b"artifact=".len())
            .position(|window| window == b"artifact=")
            .unwrap()
            + b"artifact=".len();
        bytes[digest] = b'X';
        assert_eq!(inspect(&bytes), Err(Error::InvalidDigest));

        let mut bytes = fixture(b"source");
        let source = bytes
            .windows(b"source=6\n".len())
            .position(|window| window == b"source=6\n")
            .unwrap();
        bytes[source + b"source=".len()] = b'7';
        assert_eq!(inspect(&bytes), Err(Error::SourceLengthMismatch));
    }

    #[test]
    fn manifest_is_borrowed_and_operations_are_structural() {
        let digest = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut manifest = b"v2|birnam-artifact-generation-v1|candidate|".to_vec();
        for index in 0..4 {
            if index != 0 {
                manifest.push(b'|');
            }
            manifest.extend_from_slice(digest);
        }
        manifest.extend_from_slice(b"|depA,depB|Target,selector:,I,2;Other,remove,R,1");
        let parsed = inspect_manifest(&manifest).unwrap();
        assert_eq!(parsed.name, b"candidate");
        assert_eq!(parsed.dependencies, b"depA,depB");
        assert_eq!(parsed.operation_count(), 2);
        assert_eq!(parsed.operation(0).unwrap().target, b"Target");
        assert!(parsed.operation(0).unwrap().install);
        assert_eq!(parsed.operation(0).unwrap().arity, 2);
        assert!(!parsed.operation(1).unwrap().install);
    }

    #[test]
    fn manifest_rejects_malformed_rows_and_digests() {
        let valid = b"v2|birnam-artifact-generation-v1|candidate|0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef|0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef|0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef|0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef||Target,selector,I,1";
        assert!(inspect_manifest(valid).is_ok());
        let mut bad = valid.to_vec();
        let offset = bad
            .windows(b"Target,selector,I,1".len())
            .position(|window| window == b"Target,selector,I,1")
            .unwrap();
        bad[offset + b"Target,selector,".len()] = b'X';
        assert_eq!(inspect_manifest(&bad), Err(Error::ManifestInvalid));
        let mut bad_digest = valid.to_vec();
        let digest = bad_digest
            .windows(64)
            .position(|window| window.iter().all(u8::is_ascii_hexdigit))
            .unwrap();
        bad_digest[digest] = b'g';
        assert_eq!(
            inspect_manifest(&bad_digest),
            Err(Error::ManifestDigestInvalid)
        );
    }

    #[test]
    fn adversarial_inputs_do_not_panic() {
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
