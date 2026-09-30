//! Source-independent private native manifest for portable resumable units.

use sha2::{Digest, Sha256};

use crate::materialization_selection::{Disposition, Row, Selection};

const DOMAIN: &[u8] = b"birnam.portable-resumable.v1\0";
const PREFIX_BYTES: usize = 64;
const DIRECTORY_STRIDE: usize = 32;
const SECTION_COUNT: usize = 2;
const DATA_OFFSET: usize = PREFIX_BYTES + DIRECTORY_STRIDE * SECTION_COUNT;
const METADATA_BYTES: usize = 64;
const ENTRY_BYTES: usize = 64;
const IDENTITY_DOMAIN: &[u8] = b"birnam.portable-materialization-identity.v1\0";

pub const MAX_MANIFEST_BYTES: usize = 64 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Quantity,
    Authority,
    Selection,
    Entry,
}

#[derive(Clone, Copy)]
pub struct EntrySpec<'a> {
    pub selection_row_index: u32,
    pub driver_argument_count: u32,
    pub lowering_version: u32,
    pub descriptor: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub selection_row_index: u32,
    pub driver_argument_count: u32,
    pub lowering_version: u32,
    pub portable_implementation_identity: u64,
    pub descriptor_sha256: [u8; 32],
}

#[derive(Clone, Copy)]
pub struct Manifest<'a> {
    bytes: &'a [u8],
    authority: [u8; 32],
    selection_sha256: [u8; 32],
    entries: &'a [u8],
}

impl<'a> Manifest<'a> {
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn authority(self) -> [u8; 32] {
        self.authority
    }

    pub const fn selection_sha256(self) -> [u8; 32] {
        self.selection_sha256
    }

    pub const fn entry_count(self) -> usize {
        self.entries.len() / ENTRY_BYTES
    }

    pub fn entry(self, index: usize) -> Option<Entry> {
        let start = index.checked_mul(ENTRY_BYTES)?;
        decode_entry(self.entries.get(start..start + ENTRY_BYTES)?)
    }
}

pub fn portable_implementation_identity(row: Row<'_>, lowering_version: u32) -> Result<u64, Error> {
    if lowering_version == 0 {
        return Err(Error::Entry);
    }
    let mut hash = Sha256::new();
    hash.update(IDENTITY_DOMAIN);
    hash.update((row.kind as u32).to_le_bytes());
    hash.update(row.semantic_digest);
    hash.update(lowering_version.to_le_bytes());
    Ok(u64::from_le_bytes(
        hash.finalize()[..8].try_into().expect("eight-byte prefix"),
    ))
}

pub fn materialize(
    authority: [u8; 32],
    selection: &Selection<'_>,
    entries: &[EntrySpec<'_>],
) -> Result<Vec<u8>, Error> {
    if authority == [0; 32] {
        return Err(Error::Authority);
    }
    let resumable_rows = (0..selection.row_count as usize)
        .filter(|index| {
            selection
                .row(*index)
                .is_some_and(|row| row.disposition == Disposition::Resumable)
        })
        .collect::<Vec<_>>();
    if entries.len() != resumable_rows.len() || entries.len() > 524_288 {
        return Err(Error::Selection);
    }
    let mut encoded_entries = Vec::with_capacity(entries.len() * ENTRY_BYTES);
    for (position, (spec, expected_index)) in entries.iter().zip(resumable_rows).enumerate() {
        if usize::try_from(spec.selection_row_index).ok() != Some(expected_index)
            || position != 0
                && entries[position - 1].selection_row_index >= spec.selection_row_index
            || spec.driver_argument_count > 1
            || spec.lowering_version == 0
            || spec.descriptor.is_empty()
        {
            return Err(Error::Entry);
        }
        let row = selection.row(expected_index).ok_or(Error::Selection)?;
        let identity = portable_implementation_identity(row, spec.lowering_version)?;
        push_u32(&mut encoded_entries, spec.selection_row_index);
        push_u32(&mut encoded_entries, spec.driver_argument_count);
        push_u32(&mut encoded_entries, spec.lowering_version);
        push_u32(&mut encoded_entries, 0);
        push_u64(&mut encoded_entries, identity);
        push_u64(&mut encoded_entries, 0);
        encoded_entries.extend_from_slice(&Sha256::digest(spec.descriptor));
    }
    let entries_offset = DATA_OFFSET + METADATA_BYTES;
    let total = entries_offset
        .checked_add(encoded_entries.len())
        .ok_or(Error::Quantity)?;
    if total > MAX_MANIFEST_BYTES {
        return Err(Error::Quantity);
    }
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(DOMAIN);
    output.resize(32, 0);
    push_u32(&mut output, 1);
    push_u32(&mut output, PREFIX_BYTES as u32);
    push_u64(&mut output, total as u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, SECTION_COUNT as u32);
    push_u32(&mut output, DIRECTORY_STRIDE as u32);
    push_u32(&mut output, 0);
    for (kind, stride, count, offset) in [
        (1, METADATA_BYTES, 1usize, DATA_OFFSET),
        (2, ENTRY_BYTES, entries.len(), entries_offset),
    ] {
        push_u32(&mut output, kind);
        push_u32(&mut output, stride as u32);
        push_u32(
            &mut output,
            u32::try_from(count).map_err(|_| Error::Quantity)?,
        );
        push_u32(&mut output, 0);
        push_u64(&mut output, offset as u64);
        push_u64(
            &mut output,
            u64::try_from(count.checked_mul(stride).ok_or(Error::Quantity)?)
                .map_err(|_| Error::Quantity)?,
        );
    }
    output.extend_from_slice(&authority);
    output.extend_from_slice(&Sha256::digest(selection.bytes()));
    output.extend_from_slice(&encoded_entries);
    inspect(&output, authority, selection)?;
    Ok(output)
}

pub fn inspect<'a>(
    bytes: &'a [u8],
    expected_authority: [u8; 32],
    selection: &Selection<'_>,
) -> Result<Manifest<'a>, Error> {
    if bytes.len() < DATA_OFFSET + METADATA_BYTES || bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Format);
    }
    if bytes.get(..DOMAIN.len()) != Some(DOMAIN)
        || bytes
            .get(DOMAIN.len()..32)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        || read_u32(bytes, 32) != Some(1)
        || read_u32(bytes, 36) != Some(PREFIX_BYTES as u32)
        || read_u64(bytes, 40) != Some(bytes.len() as u64)
        || read_u32(bytes, 48) != Some(0)
        || read_u32(bytes, 52) != Some(SECTION_COUNT as u32)
        || read_u32(bytes, 56) != Some(DIRECTORY_STRIDE as u32)
        || read_u32(bytes, 60) != Some(0)
    {
        return Err(Error::Format);
    }
    let entry_count = read_u32(bytes, 104).ok_or(Error::Format)? as usize;
    let entries_length = entry_count
        .checked_mul(ENTRY_BYTES)
        .ok_or(Error::Quantity)?;
    let entries_offset = DATA_OFFSET + METADATA_BYTES;
    if read_u32(bytes, 64) != Some(1)
        || read_u32(bytes, 68) != Some(METADATA_BYTES as u32)
        || read_u32(bytes, 72) != Some(1)
        || read_u32(bytes, 76) != Some(0)
        || read_u64(bytes, 80) != Some(DATA_OFFSET as u64)
        || read_u64(bytes, 88) != Some(METADATA_BYTES as u64)
        || read_u32(bytes, 96) != Some(2)
        || read_u32(bytes, 100) != Some(ENTRY_BYTES as u32)
        || read_u32(bytes, 108) != Some(0)
        || read_u64(bytes, 112) != Some(entries_offset as u64)
        || read_u64(bytes, 120) != Some(entries_length as u64)
        || entries_offset.checked_add(entries_length) != Some(bytes.len())
    {
        return Err(Error::Format);
    }
    let authority: [u8; 32] = bytes
        .get(DATA_OFFSET..DATA_OFFSET + 32)
        .ok_or(Error::Format)?
        .try_into()
        .map_err(|_| Error::Format)?;
    let selection_sha256: [u8; 32] = bytes
        .get(DATA_OFFSET + 32..DATA_OFFSET + 64)
        .ok_or(Error::Format)?
        .try_into()
        .map_err(|_| Error::Format)?;
    if authority != expected_authority
        || authority == [0; 32]
        || selection_sha256 != <[u8; 32]>::from(Sha256::digest(selection.bytes()))
    {
        return Err(Error::Authority);
    }
    let entries = bytes.get(entries_offset..).ok_or(Error::Format)?;
    let expected_rows = (0..selection.row_count as usize)
        .filter(|index| {
            selection
                .row(*index)
                .is_some_and(|row| row.disposition == Disposition::Resumable)
        })
        .collect::<Vec<_>>();
    if expected_rows.len() != entry_count {
        return Err(Error::Selection);
    }
    for (position, expected_index) in expected_rows.into_iter().enumerate() {
        let entry = decode_entry(
            entries
                .get(position * ENTRY_BYTES..(position + 1) * ENTRY_BYTES)
                .ok_or(Error::Format)?,
        )
        .ok_or(Error::Entry)?;
        let row = selection.row(expected_index).ok_or(Error::Selection)?;
        if entry.selection_row_index as usize != expected_index
            || entry.driver_argument_count > 1
            || entry.lowering_version == 0
            || entry.descriptor_sha256 == [0; 32]
            || entry.portable_implementation_identity
                != portable_implementation_identity(row, entry.lowering_version)?
        {
            return Err(Error::Entry);
        }
    }
    Ok(Manifest {
        bytes,
        authority,
        selection_sha256,
        entries,
    })
}

fn decode_entry(bytes: &[u8]) -> Option<Entry> {
    if bytes.len() != ENTRY_BYTES || read_u32(bytes, 12)? != 0 || read_u64(bytes, 24)? != 0 {
        return None;
    }
    Some(Entry {
        selection_row_index: read_u32(bytes, 0)?,
        driver_argument_count: read_u32(bytes, 4)?,
        lowering_version: read_u32(bytes, 8)?,
        portable_implementation_identity: read_u64(bytes, 16)?,
        descriptor_sha256: bytes.get(32..64)?.try_into().ok()?,
    })
}

use super::wire::{push_u32, push_u64, read_u32_checked as read_u32, read_u64_checked as read_u64};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{code_binding_inventory, materialization_selection, portable_inventory};

    #[test]
    fn manifest_covers_exactly_resumable_rows_and_derives_identity() {
        let program = include_bytes!(concat!(
            "../../../tests/fixtures/portable-code-v1/",
            "program-empty.bin"
        ));
        let programs = portable_inventory::encode_closure(&[program.as_slice()]).unwrap();
        let bindings = code_binding_inventory::encode(&[]).unwrap();
        let selection_bytes =
            materialization_selection::encode(&programs, &bindings, &[], &[Disposition::Resumable])
                .unwrap();
        let selection =
            materialization_selection::inspect(&selection_bytes, &programs, &bindings).unwrap();
        let authority =
            materialization_selection::authority(&programs, &bindings, &selection_bytes).unwrap();
        let bytes = materialize(
            authority,
            &selection,
            &[EntrySpec {
                selection_row_index: 0,
                driver_argument_count: 0,
                lowering_version: 1,
                descriptor: b"descriptor",
            }],
        )
        .unwrap();
        let manifest = inspect(&bytes, authority, &selection).unwrap();
        assert_eq!(manifest.entry_count(), 1);
        let entry = manifest.entry(0).unwrap();
        assert_eq!(entry.selection_row_index, 0);
        assert_eq!(
            entry.portable_implementation_identity,
            portable_implementation_identity(selection.row(0).unwrap(), 1).unwrap()
        );
    }

    #[test]
    fn synchronous_rows_cannot_appear_in_manifest() {
        let program = include_bytes!(concat!(
            "../../../tests/fixtures/portable-code-v1/",
            "program-empty.bin"
        ));
        let programs = portable_inventory::encode_closure(&[program.as_slice()]).unwrap();
        let bindings = code_binding_inventory::encode(&[]).unwrap();
        let selection_bytes = materialization_selection::encode(
            &programs,
            &bindings,
            &[],
            &[Disposition::Synchronous],
        )
        .unwrap();
        let selection =
            materialization_selection::inspect(&selection_bytes, &programs, &bindings).unwrap();
        let authority =
            materialization_selection::authority(&programs, &bindings, &selection_bytes).unwrap();
        assert_eq!(materialize(authority, &selection, &[]).unwrap().len(), 192);
        assert_eq!(
            materialize(
                authority,
                &selection,
                &[EntrySpec {
                    selection_row_index: 0,
                    driver_argument_count: 0,
                    lowering_version: 1,
                    descriptor: b"descriptor",
                }]
            ),
            Err(Error::Selection)
        );
    }
}
