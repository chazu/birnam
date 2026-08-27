//! Allocation-free parser for the pointer-free candidate sidecar envelope.
//!
//! Candidate mapping and publication are deliberately outside this module. It
//! only validates the detached bytes produced by the frozen Codegen contract,
//! so a later rung can inspect a candidate before opening its dylib.

pub const CANDIDATE_MAGIC: &[u8] = b"BIRNAM-CANDIDATE-2";
pub const CANDIDATE_ABI: u32 = 4;
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
    SelectionLengthMismatch,
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
    pub expected_predecessor_digest: &'a [u8],
    pub manifest: &'a [u8],
    pub artifact_selection: &'a [u8],
    pub source: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestVersion {
    V4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest<'a> {
    pub version: ManifestVersion,
    pub name: &'a [u8],
    pub source_digest: &'a [u8],
    pub non_method_digest: &'a [u8],
    pub method_digest: &'a [u8],
    pub revision_digest: &'a [u8],
    pub artifact_selection_digest: Option<&'a [u8]>,
    pub covered_revisions: Option<&'a [u8]>,
    pub dependencies: &'a [u8],
    pub operations: &'a [u8],
    dependency_count: usize,
    operation_count: usize,
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

/// Parse the candidate's pipe-delimited manifest without allocating.
pub fn inspect_manifest(bytes: &[u8]) -> Result<Manifest<'_>, Error> {
    if !bytes.starts_with(b"v4|") {
        return Err(Error::ManifestInvalid);
    }
    inspect_manifest_v4(bytes)
}

fn colon_decimal(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let rest = bytes.get(*cursor..)?;
    let end = rest.iter().position(|byte| *byte == b':')?;
    let value = decimal(&rest[..end])?;
    *cursor = cursor.checked_add(end + 1)?;
    Some(value)
}

fn length_value<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let length = usize::try_from(colon_decimal(bytes, cursor)?).ok()?;
    let end = cursor.checked_add(length)?;
    let value = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(value)
}

fn delimiter(bytes: &[u8], cursor: &mut usize, expected: u8) -> Option<()> {
    if bytes.get(*cursor) != Some(&expected) {
        return None;
    }
    *cursor = cursor.checked_add(1)?;
    Some(())
}

fn framed_token(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|byte| *byte >= 0x21)
}

fn parse_v4_operation<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<Operation<'a>> {
    let target = length_value(bytes, cursor)?;
    let selector = length_value(bytes, cursor)?;
    let install = match *bytes.get(*cursor)? {
        b'I' => true,
        b'R' => false,
        _ => return None,
    };
    *cursor = cursor.checked_add(1)?;
    let arity = colon_decimal(bytes, cursor)?;
    if !framed_token(target) || !framed_token(selector) {
        return None;
    }
    Some(Operation {
        target,
        selector,
        install,
        arity,
    })
}

fn inspect_manifest_v4(bytes: &[u8]) -> Result<Manifest<'_>, Error> {
    let mut cursor = b"v4|".len();
    let runtime = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    if runtime != crate::ABI {
        return Err(Error::ManifestInvalid);
    }
    let name = length_value(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    if !framed_token(name) {
        return Err(Error::ManifestInvalid);
    }
    delimiter(bytes, &mut cursor, b'|').ok_or(Error::ManifestInvalid)?;
    let source_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let non_method_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let method_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let revision_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    let selection_digest = segment(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    for digest_value in [
        source_digest,
        non_method_digest,
        method_digest,
        revision_digest,
        selection_digest,
    ] {
        if !lowercase_digest(digest_value) {
            return Err(Error::ManifestDigestInvalid);
        }
    }

    let revision_count =
        usize::try_from(colon_decimal(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?)
            .map_err(|_| Error::ManifestInvalid)?;
    let revisions_start = cursor;
    let revisions_length = revision_count
        .checked_mul(64)
        .ok_or(Error::ManifestInvalid)?;
    let revisions_end = revisions_start
        .checked_add(revisions_length)
        .ok_or(Error::ManifestInvalid)?;
    let covered_revisions = bytes
        .get(revisions_start..revisions_end)
        .ok_or(Error::ManifestInvalid)?;
    let mut previous = None;
    for revision in covered_revisions.chunks_exact(64) {
        if !lowercase_digest(revision) || previous.is_some_and(|value| value >= revision) {
            return Err(Error::ManifestDigestInvalid);
        }
        previous = Some(revision);
    }
    cursor = revisions_end;

    delimiter(bytes, &mut cursor, b'|').ok_or(Error::ManifestInvalid)?;
    let dependency_count =
        usize::try_from(colon_decimal(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?)
            .map_err(|_| Error::ManifestInvalid)?;
    let dependencies_start = cursor;
    for _ in 0..dependency_count {
        let dependency = length_value(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
        if !framed_token(dependency) {
            return Err(Error::ManifestInvalid);
        }
    }
    let dependencies = &bytes[dependencies_start..cursor];

    delimiter(bytes, &mut cursor, b'|').ok_or(Error::ManifestInvalid)?;
    let operation_count =
        usize::try_from(colon_decimal(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?)
            .map_err(|_| Error::ManifestInvalid)?;
    if operation_count == 0 {
        return Err(Error::ManifestInvalid);
    }
    let operations_start = cursor;
    for _ in 0..operation_count {
        parse_v4_operation(bytes, &mut cursor).ok_or(Error::ManifestInvalid)?;
    }
    if cursor != bytes.len() {
        return Err(Error::ManifestInvalid);
    }
    let operations = &bytes[operations_start..cursor];
    Ok(Manifest {
        version: ManifestVersion::V4,
        name,
        source_digest,
        non_method_digest,
        method_digest,
        revision_digest,
        artifact_selection_digest: Some(selection_digest),
        covered_revisions: Some(covered_revisions),
        dependencies,
        operations,
        dependency_count,
        operation_count,
    })
}

impl<'a> Manifest<'a> {
    pub fn operation_count(self) -> usize {
        self.operation_count
    }

    pub fn operation(self, index: usize) -> Option<Operation<'a>> {
        if index >= self.operation_count {
            return None;
        }
        let mut cursor = 0;
        let mut operation = None;
        for _ in 0..=index {
            operation = parse_v4_operation(self.operations, &mut cursor);
        }
        operation
    }

    pub fn dependency_count(self) -> usize {
        self.dependency_count
    }

    pub fn dependency(self, index: usize) -> Option<&'a [u8]> {
        if index >= self.dependency_count {
            return None;
        }
        let mut cursor = 0;
        let mut dependency = None;
        for _ in 0..=index {
            dependency = length_value(self.dependencies, &mut cursor);
        }
        dependency
    }

    pub fn covered_revision_count(self) -> usize {
        self.covered_revisions
            .map(|revisions| revisions.len() / 64)
            .unwrap_or(0)
    }

    pub fn covered_revision(self, index: usize) -> Option<&'a [u8]> {
        let revisions = self.covered_revisions?;
        revisions.chunks_exact(64).nth(index)
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

fn lowercase_digest(bytes: &[u8]) -> bool {
    bytes.len() == 64
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
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
    let expected_predecessor_digest =
        field(bytes, &mut cursor, b"predecessor=").ok_or(Error::MissingField)?;
    if !lowercase_digest(expected_predecessor_digest) {
        return Err(Error::InvalidDigest);
    }
    let manifest = field(bytes, &mut cursor, b"manifest=").ok_or(Error::MissingField)?;
    if manifest.is_empty() || manifest.contains(&b'\n') {
        return Err(Error::InvalidField);
    }
    let selection_length =
        decimal(field(bytes, &mut cursor, b"selection=").ok_or(Error::MissingField)?)
            .ok_or(Error::InvalidField)? as usize;
    if selection_length > MAX_CANDIDATE_BYTES {
        return Err(Error::SelectionLengthMismatch);
    }
    let selection_end = cursor
        .checked_add(selection_length)
        .ok_or(Error::SelectionLengthMismatch)?;
    let artifact_selection = bytes
        .get(cursor..selection_end)
        .ok_or(Error::SelectionLengthMismatch)?;
    if bytes.get(selection_end) != Some(&b'\n') {
        return Err(Error::SelectionLengthMismatch);
    }
    cursor = selection_end + 1;
    let source_length = decimal(field(bytes, &mut cursor, b"source=").ok_or(Error::MissingField)?)
        .ok_or(Error::InvalidField)? as usize;
    if source_length > MAX_CANDIDATE_BYTES {
        return Err(Error::SourceLengthMismatch);
    }
    let source = bytes.get(cursor..).ok_or(Error::SourceLengthMismatch)?;
    if source.len() != source_length {
        return Err(Error::SourceLengthMismatch);
    }
    inspect_manifest(manifest)?;
    Ok(Envelope {
        artifact_digest,
        runtime_abi,
        candidate_abi,
        platform,
        expected_predecessor_digest,
        manifest,
        artifact_selection,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(source: &[u8]) -> Vec<u8> {
        let digest = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
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
        bytes.extend_from_slice(b"candidate=4\nplatform=aarch64-macos\npredecessor=");
        bytes.extend_from_slice(digest);
        bytes.extend_from_slice(b"\nmanifest=v4|birnam-artifact-generation-v1|9:candidate|");
        for _ in 0..5 {
            bytes.extend_from_slice(digest);
            bytes.push(b'|');
        }
        bytes.extend_from_slice(b"0:|0:|1:6:Target8:selectorI1:\nselection=0\n\nsource=");
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
        assert_eq!(
            envelope.expected_predecessor_digest,
            b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert!(envelope.artifact_selection.is_empty());
        assert!(
            envelope
                .manifest
                .starts_with(b"v4|birnam-artifact-generation-v1|9:candidate|")
        );
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
            .windows(b"candidate=4".len())
            .position(|window| window == b"candidate=4")
            .unwrap();
        bytes[candidate + b"candidate=".len()] = b'5';
        assert_eq!(inspect(&bytes), Err(Error::CandidateAbiMismatch));

        let mut bytes = fixture(b"source");
        let predecessor = bytes
            .windows(b"predecessor=".len())
            .position(|window| window == b"predecessor=")
            .unwrap()
            + b"predecessor=".len();
        bytes[predecessor] = b'A';
        assert_eq!(inspect(&bytes), Err(Error::InvalidDigest));
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
    fn obsolete_manifest_versions_are_rejected_before_parsing() {
        assert_eq!(inspect_manifest(b"v2|garbage"), Err(Error::ManifestInvalid));
        assert_eq!(inspect_manifest(b"v3|garbage"), Err(Error::ManifestInvalid));
    }

    #[test]
    fn manifest_v4_length_frames_separator_selectors() {
        let digest = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut manifest = b"v4|birnam-artifact-generation-v1|6:stdlib|".to_vec();
        for _ in 0..5 {
            manifest.extend_from_slice(digest);
            manifest.push(b'|');
        }
        manifest.extend_from_slice(b"1:");
        manifest.extend_from_slice(digest);
        manifest.extend_from_slice(b"|1:4:core|2:6:Object2:|>I1:7:Integer1:,I1:");
        let parsed = inspect_manifest(&manifest).unwrap();
        assert_eq!(parsed.version, ManifestVersion::V4);
        assert_eq!(parsed.name, b"stdlib");
        assert_eq!(parsed.covered_revision(0), Some(digest.as_slice()));
        assert_eq!(parsed.dependency(0), Some(b"core".as_slice()));
        assert_eq!(parsed.operation_count(), 2);
        assert_eq!(parsed.operation(0).unwrap().selector, b"|>");
        assert_eq!(parsed.operation(1).unwrap().selector, b",");
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
