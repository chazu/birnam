//! Canonical ResumableManifest v1 codec and artifact-selection agreement.

use core::cmp::Ordering;

use sha2::{Digest, Sha256};

use crate::frame_descriptor;
use crate::stackless_selection::{
    Disposition, ImplementationKind, MAX_IMPLEMENTATION_ROWS, MAX_PATH_DEPTH, MAX_PATH_ORDINALS,
    RootKind, Selection, SelectionKind, SelectionSpec, portable_implementation_identity,
};

pub const DOMAIN: [u8; 32] = *b"birnam.resumable-manifest.v1\0\0\0\0";
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SELECTOR_ARENA_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LANGUAGE_ARITY: u32 = 4096;

const PREFIX_BYTES: usize = 64;
const DIRECTORY_STRIDE: usize = 32;
const SECTION_COUNT: usize = 4;
const DATA_OFFSET: usize = PREFIX_BYTES + SECTION_COUNT * DIRECTORY_STRIDE;
const AUTHORITY_BYTES: usize = 32;
const ENTRY_BYTES: usize = 96;
const ORDINAL_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Truncated,
    Domain,
    Prefix,
    RequiredFlags,
    Directory,
    Padding,
    Arithmetic,
    Limit,
    Authority,
    Entry,
    EntryOrder,
    PathStorage,
    SelectorStorage,
    Arity,
    Descriptor,
    SelectionAgreement,
}

#[derive(Clone, Copy)]
pub struct ManifestEntrySpec<'a> {
    pub selection: SelectionSpec<'a>,
    pub descriptor: &'a [u8],
    pub selector: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path<'a> {
    bytes: &'a [u8],
}

impl<'a> Path<'a> {
    pub const fn len(self) -> usize {
        self.bytes.len() / ORDINAL_BYTES
    }

    pub fn get(self, index: usize) -> Option<u32> {
        read_u32(self.bytes, index.checked_mul(ORDINAL_BYTES)?)
    }

    pub fn iter(self) -> impl Iterator<Item = u32> + 'a {
        self.bytes
            .chunks_exact(ORDINAL_BYTES)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte chunk")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub source_revision: [u8; 32],
    pub root_kind: RootKind,
    pub root_index: u32,
    pub path: Path<'a>,
    pub implementation_kind: ImplementationKind,
    pub portable_implementation_identity: u64,
    pub descriptor_sha256: [u8; 32],
    pub selector: &'a [u8],
    pub language_arity: u32,
}

#[derive(Clone)]
pub struct Manifest<'a> {
    bytes: &'a [u8],
    authority: [u8; 32],
    entries: &'a [u8],
    ordinals: &'a [u8],
    selectors: &'a [u8],
    path_offsets: Vec<(usize, usize)>,
    selector_offsets: Vec<(usize, usize)>,
}

impl<'a> Manifest<'a> {
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub const fn authority(&self) -> [u8; 32] {
        self.authority
    }

    pub const fn entry_count(&self) -> usize {
        self.path_offsets.len()
    }

    pub fn entry(&self, index: usize) -> Option<Entry<'a>> {
        let record = self
            .entries
            .get(index.checked_mul(ENTRY_BYTES)?..)?
            .get(..ENTRY_BYTES)?;
        let (path_start, path_length) = *self.path_offsets.get(index)?;
        let (selector_start, selector_length) = *self.selector_offsets.get(index)?;
        decode_entry(
            record,
            self.ordinals
                .get(path_start..path_start.checked_add(path_length)?)?,
            self.selectors
                .get(selector_start..selector_start.checked_add(selector_length)?)?,
        )
    }
}

/// Inspect canonical manifest bytes after the caller has established the
/// staged DispatchGeneration authority.
pub fn inspect<'a>(bytes: &'a [u8], expected_authority: [u8; 32]) -> Result<Manifest<'a>, Error> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::TooLarge);
    }
    if bytes.len() < DATA_OFFSET + AUTHORITY_BYTES {
        return Err(Error::Truncated);
    }
    if bytes.get(..32) != Some(&DOMAIN) {
        return Err(Error::Domain);
    }
    if read_u32(bytes, 0x20) != Some(1)
        || read_u32(bytes, 0x24) != Some(PREFIX_BYTES as u32)
        || read_u64(bytes, 0x28) != Some(bytes.len() as u64)
        || read_u32(bytes, 0x34) != Some(SECTION_COUNT as u32)
        || read_u32(bytes, 0x38) != Some(DIRECTORY_STRIDE as u32)
        || read_u32(bytes, 0x3c) != Some(0)
    {
        return Err(Error::Prefix);
    }
    if read_u32(bytes, 0x30) != Some(0) {
        return Err(Error::RequiredFlags);
    }
    let strides = [AUTHORITY_BYTES, ENTRY_BYTES, ORDINAL_BYTES, 1];
    let mut cursor = DATA_OFFSET;
    let mut sections = [(&[][..], 0usize); SECTION_COUNT];
    for (index, stride) in strides.into_iter().enumerate() {
        let base = PREFIX_BYTES + index * DIRECTORY_STRIDE;
        let kind = read_u32(bytes, base).ok_or(Error::Truncated)?;
        let actual_stride = read_u32(bytes, base + 4).ok_or(Error::Truncated)? as usize;
        let count = read_u32(bytes, base + 8).ok_or(Error::Truncated)? as usize;
        let reserved = read_u32(bytes, base + 12).ok_or(Error::Truncated)?;
        let offset = usize::try_from(read_u64(bytes, base + 16).ok_or(Error::Truncated)?)
            .map_err(|_| Error::Arithmetic)?;
        let length = usize::try_from(read_u64(bytes, base + 24).ok_or(Error::Truncated)?)
            .map_err(|_| Error::Arithmetic)?;
        let expected_offset = if index == 3 { align8(cursor)? } else { cursor };
        if kind != index as u32 + 1
            || actual_stride != stride
            || reserved != 0
            || offset != expected_offset
            || length != count.checked_mul(stride).ok_or(Error::Arithmetic)?
        {
            return Err(Error::Directory);
        }
        if bytes
            .get(cursor..offset)
            .ok_or(Error::Truncated)?
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(Error::Padding);
        }
        let end = offset.checked_add(length).ok_or(Error::Arithmetic)?;
        sections[index] = (bytes.get(offset..end).ok_or(Error::Truncated)?, count);
        cursor = end;
    }
    if cursor != bytes.len()
        || sections[0].1 != 1
        || sections[1].1 > MAX_IMPLEMENTATION_ROWS
        || sections[2].1 > MAX_PATH_ORDINALS
        || sections[3].0.len() > MAX_SELECTOR_ARENA_BYTES
    {
        return Err(Error::Limit);
    }
    let authority: [u8; 32] = sections[0].0.try_into().map_err(|_| Error::Authority)?;
    if authority != expected_authority {
        return Err(Error::Authority);
    }

    let entries = sections[1].0;
    let ordinals = sections[2].0;
    let selectors = sections[3].0;
    let mut path_offsets = Vec::with_capacity(sections[1].1);
    let mut selector_offsets = Vec::with_capacity(sections[1].1);
    let mut path_cursor = 0usize;
    let mut selector_cursor = 0usize;
    let mut previous: Option<Entry<'_>> = None;
    for index in 0..sections[1].1 {
        let record = entries
            .get(index * ENTRY_BYTES..(index + 1) * ENTRY_BYTES)
            .ok_or(Error::Entry)?;
        let path_count = read_u32(record, 0x28).ok_or(Error::Entry)? as usize;
        if path_count > MAX_PATH_DEPTH {
            return Err(Error::Limit);
        }
        let path_length = path_count
            .checked_mul(ORDINAL_BYTES)
            .ok_or(Error::Arithmetic)?;
        let path_end = path_cursor
            .checked_add(path_length)
            .ok_or(Error::Arithmetic)?;
        if path_end > ordinals.len() {
            return Err(Error::PathStorage);
        }
        let selector_length = read_u32(record, 0x58).ok_or(Error::Entry)? as usize;
        let selector_end = selector_cursor
            .checked_add(selector_length)
            .ok_or(Error::Arithmetic)?;
        if selector_length == 0
            || selector_length > frame_descriptor::MAX_SELECTOR_BYTES
            || selector_end > selectors.len()
        {
            return Err(Error::SelectorStorage);
        }
        path_offsets.push((path_cursor, path_length));
        selector_offsets.push((selector_cursor, selector_length));
        let entry = decode_entry(
            record,
            &ordinals[path_cursor..path_end],
            &selectors[selector_cursor..selector_end],
        )
        .ok_or(Error::Entry)?;
        validate_entry_shape(entry)?;
        if selector_arity(entry.selector) != entry.language_arity
            || entry.language_arity > MAX_LANGUAGE_ARITY
        {
            return Err(Error::Arity);
        }
        if previous.is_some_and(|prior| compare_manifest_entries(prior, entry) != Ordering::Less) {
            return Err(Error::EntryOrder);
        }
        previous = Some(entry);
        path_cursor = path_end;
        selector_cursor = selector_end;
    }
    if path_cursor != ordinals.len() {
        return Err(Error::PathStorage);
    }
    if selector_cursor != selectors.len() {
        return Err(Error::SelectorStorage);
    }
    Ok(Manifest {
        bytes,
        authority,
        entries,
        ordinals,
        selectors,
        path_offsets,
        selector_offsets,
    })
}

struct PreparedEntry<'a> {
    selection: SelectionSpec<'a>,
    descriptor_sha256: [u8; 32],
    portable_identity: u64,
    selector: &'a [u8],
    language_arity: u32,
}

/// Materialize a manifest total over exactly the ResumableRequired rows of one
/// admitted ArtifactSelection.
pub fn materialize(
    authority: [u8; 32],
    selection: Selection<'_>,
    entries: &[ManifestEntrySpec<'_>],
) -> Result<Vec<u8>, Error> {
    if selection.kind() != SelectionKind::Artifact || entries.len() > MAX_IMPLEMENTATION_ROWS {
        return Err(Error::SelectionAgreement);
    }
    let mut prepared = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.selection.disposition != Disposition::ResumableRequired
            || entry.selector.is_empty()
            || entry.selector.len() > frame_descriptor::MAX_SELECTOR_BYTES
        {
            return Err(Error::Entry);
        }
        let descriptor = frame_descriptor::inspect(entry.descriptor, entry.selection)
            .map_err(|_| Error::Descriptor)?;
        let metadata = descriptor.metadata();
        let language_arity = selector_arity(entry.selector);
        if language_arity > MAX_LANGUAGE_ARITY
            || metadata.entry_argument_count != language_arity
            || metadata.implementation_kind != entry.selection.implementation_kind
        {
            return Err(Error::Arity);
        }
        let portable_identity =
            portable_implementation_identity(entry.selection, metadata.lowering_version)
                .map_err(|_| Error::Descriptor)?;
        if portable_identity != metadata.portable_implementation_identity {
            return Err(Error::Descriptor);
        }
        prepared.push(PreparedEntry {
            selection: entry.selection,
            descriptor_sha256: Sha256::digest(entry.descriptor).into(),
            portable_identity,
            selector: entry.selector,
            language_arity,
        });
    }
    prepared.sort_by(compare_prepared_manifest);
    if prepared
        .windows(2)
        .any(|pair| same_stable_key(pair[0].selection, pair[1].selection))
    {
        return Err(Error::EntryOrder);
    }

    let mut required = selection
        .rows()
        .map(|row| row.map_err(|_| Error::SelectionAgreement))
        .collect::<Result<Vec<_>, _>>()?;
    required.retain(|row| row.disposition == Disposition::ResumableRequired);
    let mut prepared_by_selection: Vec<_> = prepared.iter().collect();
    prepared_by_selection.sort_by(|left, right| compare_selection(left.selection, right.selection));
    if required.len() != prepared_by_selection.len()
        || required
            .iter()
            .zip(&prepared_by_selection)
            .any(|(row, entry)| {
                row.source_revision != entry.selection.source_revision
                    || row.root_kind != entry.selection.root_kind
                    || row.root_index != entry.selection.root_index
                    || row.implementation_kind != entry.selection.implementation_kind
                    || !row.path.iter().eq(entry.selection.path.iter().copied())
            })
    {
        return Err(Error::SelectionAgreement);
    }

    let path_count = prepared
        .iter()
        .try_fold(0usize, |total, entry| {
            total.checked_add(entry.selection.path.len())
        })
        .ok_or(Error::Arithmetic)?;
    let selector_bytes = prepared
        .iter()
        .try_fold(0usize, |total, entry| {
            total.checked_add(entry.selector.len())
        })
        .ok_or(Error::Arithmetic)?;
    if path_count > MAX_PATH_ORDINALS || selector_bytes > MAX_SELECTOR_ARENA_BYTES {
        return Err(Error::Limit);
    }
    let entry_offset = DATA_OFFSET + AUTHORITY_BYTES;
    let ordinal_offset = entry_offset
        .checked_add(
            prepared
                .len()
                .checked_mul(ENTRY_BYTES)
                .ok_or(Error::Arithmetic)?,
        )
        .ok_or(Error::Arithmetic)?;
    let selector_offset = align8(
        ordinal_offset
            .checked_add(
                path_count
                    .checked_mul(ORDINAL_BYTES)
                    .ok_or(Error::Arithmetic)?,
            )
            .ok_or(Error::Arithmetic)?,
    )?;
    let total = selector_offset
        .checked_add(selector_bytes)
        .ok_or(Error::Arithmetic)?;
    if total > MAX_MANIFEST_BYTES {
        return Err(Error::TooLarge);
    }

    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&DOMAIN);
    push_u32(&mut output, 1);
    push_u32(&mut output, PREFIX_BYTES as u32);
    push_u64(&mut output, total as u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, SECTION_COUNT as u32);
    push_u32(&mut output, DIRECTORY_STRIDE as u32);
    push_u32(&mut output, 0);
    push_directory(&mut output, 1, 32, 1, DATA_OFFSET, 32)?;
    push_directory(
        &mut output,
        2,
        ENTRY_BYTES,
        prepared.len(),
        entry_offset,
        prepared.len() * ENTRY_BYTES,
    )?;
    push_directory(
        &mut output,
        3,
        ORDINAL_BYTES,
        path_count,
        ordinal_offset,
        path_count * ORDINAL_BYTES,
    )?;
    push_directory(
        &mut output,
        4,
        1,
        selector_bytes,
        selector_offset,
        selector_bytes,
    )?;
    output.extend_from_slice(&authority);
    for entry in &prepared {
        output.extend_from_slice(&entry.selection.source_revision);
        push_u32(&mut output, entry.selection.root_kind as u32);
        push_u32(&mut output, entry.selection.root_index);
        push_u32(
            &mut output,
            u32::try_from(entry.selection.path.len()).map_err(|_| Error::Arithmetic)?,
        );
        push_u32(&mut output, entry.selection.implementation_kind as u32);
        push_u64(&mut output, entry.portable_identity);
        output.extend_from_slice(&entry.descriptor_sha256);
        push_u32(
            &mut output,
            u32::try_from(entry.selector.len()).map_err(|_| Error::Arithmetic)?,
        );
        push_u32(&mut output, entry.language_arity);
    }
    for entry in &prepared {
        for ordinal in entry.selection.path {
            push_u32(&mut output, *ordinal);
        }
    }
    output.resize(selector_offset, 0);
    for entry in &prepared {
        output.extend_from_slice(entry.selector);
    }
    if output.len() != total {
        return Err(Error::Arithmetic);
    }
    inspect(&output, authority)?;
    Ok(output)
}

/// Recompute the canonical manifest from admitted selection and descriptor
/// ownership, then require byte identity with a mapped artifact export.
pub fn require_artifact<'a>(
    manifest: Manifest<'a>,
    selection: Selection<'_>,
    entries: &[ManifestEntrySpec<'_>],
) -> Result<Manifest<'a>, Error> {
    let expected = materialize(manifest.authority, selection, entries)?;
    if expected != manifest.bytes {
        return Err(Error::Descriptor);
    }
    Ok(manifest)
}

fn validate_entry_shape(entry: Entry<'_>) -> Result<(), Error> {
    match entry.implementation_kind {
        ImplementationKind::NamedAdapter
            if entry.root_kind == RootKind::MethodOperation && entry.path.len() == 0 => {}
        ImplementationKind::BlockBody if entry.path.len() != 0 => {}
        _ => return Err(Error::Entry),
    }
    if entry.descriptor_sha256 == [0; 32] {
        return Err(Error::Entry);
    }
    Ok(())
}

fn decode_entry<'a>(record: &[u8], path: &'a [u8], selector: &'a [u8]) -> Option<Entry<'a>> {
    Some(Entry {
        source_revision: record.get(..32)?.try_into().ok()?,
        root_kind: decode_root(read_u32(record, 0x20)?)?,
        root_index: read_u32(record, 0x24)?,
        path: Path { bytes: path },
        implementation_kind: decode_implementation(read_u32(record, 0x2c)?)?,
        portable_implementation_identity: read_u64(record, 0x30)?,
        descriptor_sha256: record.get(0x38..0x58)?.try_into().ok()?,
        selector,
        language_arity: read_u32(record, 0x5c)?,
    })
}

fn compare_manifest_entries(left: Entry<'_>, right: Entry<'_>) -> Ordering {
    (left.implementation_kind as u32)
        .cmp(&(right.implementation_kind as u32))
        .then_with(|| left.source_revision.cmp(&right.source_revision))
        .then_with(|| (left.root_kind as u32).cmp(&(right.root_kind as u32)))
        .then_with(|| left.root_index.cmp(&right.root_index))
        .then_with(|| left.path.iter().cmp(right.path.iter()))
        .then_with(|| {
            left.portable_implementation_identity
                .cmp(&right.portable_implementation_identity)
        })
}

fn compare_prepared_manifest(left: &PreparedEntry<'_>, right: &PreparedEntry<'_>) -> Ordering {
    (left.selection.implementation_kind as u32)
        .cmp(&(right.selection.implementation_kind as u32))
        .then_with(|| compare_selection(left.selection, right.selection))
        .then_with(|| left.portable_identity.cmp(&right.portable_identity))
}

fn compare_selection(left: SelectionSpec<'_>, right: SelectionSpec<'_>) -> Ordering {
    left.source_revision
        .cmp(&right.source_revision)
        .then_with(|| (left.root_kind as u32).cmp(&(right.root_kind as u32)))
        .then_with(|| left.root_index.cmp(&right.root_index))
        .then_with(|| left.path.cmp(right.path))
        .then_with(|| (left.implementation_kind as u32).cmp(&(right.implementation_kind as u32)))
}

fn same_stable_key(left: SelectionSpec<'_>, right: SelectionSpec<'_>) -> bool {
    compare_selection(left, right) == Ordering::Equal
}

fn selector_arity(selector: &[u8]) -> u32 {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        colons as u32
    } else if selector
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        0
    } else {
        1
    }
}

fn decode_root(value: u32) -> Option<RootKind> {
    match value {
        1 => Some(RootKind::NonMethodProgram),
        2 => Some(RootKind::MethodOperation),
        _ => None,
    }
}

fn decode_implementation(value: u32) -> Option<ImplementationKind> {
    match value {
        1 => Some(ImplementationKind::NamedAdapter),
        2 => Some(ImplementationKind::BlockBody),
        _ => None,
    }
}

fn push_directory(
    output: &mut Vec<u8>,
    kind: u32,
    stride: usize,
    count: usize,
    offset: usize,
    length: usize,
) -> Result<(), Error> {
    push_u32(output, kind);
    push_u32(
        output,
        u32::try_from(stride).map_err(|_| Error::Arithmetic)?,
    );
    push_u32(output, u32::try_from(count).map_err(|_| Error::Arithmetic)?);
    push_u32(output, 0);
    push_u64(
        output,
        u64::try_from(offset).map_err(|_| Error::Arithmetic)?,
    );
    push_u64(
        output,
        u64::try_from(length).map_err(|_| Error::Arithmetic)?,
    );
    Ok(())
}

fn align8(value: usize) -> Result<usize, Error> {
    value
        .checked_add(7)
        .map(|value| value & !7)
        .ok_or(Error::Arithmetic)
}

use super::wire::{push_u32, push_u64, read_u32, read_u64};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stackless_selection::inspect_artifact_selection;

    fn bytes(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/stackless-s2")
                .join(name),
        )
        .unwrap()
    }

    fn authority(bytes: &[u8]) -> [u8; 32] {
        bytes[0xc0..0xe0].try_into().unwrap()
    }

    fn spec<'a>(
        revision: [u8; 32],
        kind: ImplementationKind,
        path: &'a [u32],
        descriptor: &'a [u8],
        selector: &'a [u8],
    ) -> ManifestEntrySpec<'a> {
        ManifestEntrySpec {
            selection: SelectionSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path,
                implementation_kind: kind,
                disposition: Disposition::ResumableRequired,
            },
            descriptor,
            selector,
        }
    }

    #[test]
    fn frozen_manifests_parse_and_serialize_byte_identically() {
        let named_descriptor = bytes("frame-descriptor-minimal-return.bin");
        let block_descriptor = bytes("frame-descriptor-source-map-block.bin");
        let cases = [
            (
                "resumable-manifest-empty.bin",
                "artifact-selection-all-synchronous.bin",
                0,
            ),
            (
                "resumable-manifest-named-only.bin",
                "artifact-selection-named-resumable.bin",
                1,
            ),
            (
                "resumable-manifest-block-only.bin",
                "artifact-selection-mixed.bin",
                2,
            ),
            (
                "resumable-manifest-mixed.bin",
                "artifact-selection-both-resumable.bin",
                3,
            ),
        ];
        for (manifest_name, selection_name, entry_mask) in cases {
            let manifest_bytes = bytes(manifest_name);
            let selection_bytes = bytes(selection_name);
            let selection = inspect_artifact_selection(&selection_bytes).unwrap();
            let revision = selection.revision(0).unwrap();
            let named = spec(
                revision,
                ImplementationKind::NamedAdapter,
                &[],
                &named_descriptor,
                b"run",
            );
            let block = spec(
                revision,
                ImplementationKind::BlockBody,
                &[2, 1],
                &block_descriptor,
                b"value",
            );
            let entries = match entry_mask {
                0 => Vec::new(),
                1 => vec![named],
                2 => vec![block],
                3 => vec![block, named],
                _ => unreachable!(),
            };
            let actual = materialize(authority(&manifest_bytes), selection, &entries)
                .unwrap_or_else(|error| panic!("{manifest_name}: {error:?}"));
            assert_eq!(actual, manifest_bytes, "{manifest_name}");
            let parsed = inspect(&manifest_bytes, authority(&manifest_bytes)).unwrap();
            assert_eq!(parsed.entry_count(), entries.len());
            require_artifact(parsed, selection, &entries).unwrap();
        }
    }

    #[test]
    fn truncation_authority_and_forged_identity_fail_closed() {
        let manifest_bytes = bytes("resumable-manifest-named-only.bin");
        let expected_authority = authority(&manifest_bytes);
        for length in 0..manifest_bytes.len() {
            assert!(inspect(&manifest_bytes[..length], expected_authority).is_err());
        }
        let mut wrong_authority = expected_authority;
        wrong_authority[0] ^= 1;
        assert_eq!(
            inspect(&manifest_bytes, wrong_authority).err(),
            Some(Error::Authority)
        );

        let forged = bytes("resumable-manifest-forged-portable-id.bin");
        let parsed = inspect(&forged, expected_authority).unwrap();
        let selection_bytes = bytes("artifact-selection-named-resumable.bin");
        let selection = inspect_artifact_selection(&selection_bytes).unwrap();
        let descriptor = bytes("frame-descriptor-minimal-return.bin");
        let entry = spec(
            selection.revision(0).unwrap(),
            ImplementationKind::NamedAdapter,
            &[],
            &descriptor,
            b"run",
        );
        assert_eq!(
            require_artifact(parsed, selection, &[entry]).err(),
            Some(Error::Descriptor)
        );
    }
}
