//! Canonical pointer-free CodeMaterializationSelection v1 bytes.

use sha2::{Digest, Sha256};

use crate::{code_binding_inventory, portable_inventory, portable_program::Error};

const DOMAIN: &[u8] = b"birnam.code-selection.v1\0";
const AUTHORITY_DOMAIN: &[u8] = b"birnam.portable-image-authority.v1\0";
const METADATA_OFFSET: usize = 160;
const ROWS_OFFSET: usize = 224;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Disposition {
    Synchronous = 1,
    Resumable = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SubjectKind {
    NamedAdapter = 1,
    BlockBody = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<'a> {
    pub kind: SubjectKind,
    pub disposition: Disposition,
    pub subject_index: u32,
    pub language_arity: u32,
    pub capture_count: u32,
    pub semantic_digest: [u8; 32],
    pub selector: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<'a> {
    bytes: &'a [u8],
    pub row_count: u32,
    selector_offsets: Vec<(usize, usize)>,
}

impl<'a> Selection<'a> {
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn row(&self, index: usize) -> Option<Row<'a>> {
        let base = ROWS_OFFSET.checked_add(index.checked_mul(64)?)?;
        let record = self.bytes.get(base..base + 64)?;
        let (selector_start, selector_length) = *self.selector_offsets.get(index)?;
        let selector = self
            .bytes
            .get(selector_start..selector_start.checked_add(selector_length)?)?;
        Some(Row {
            kind: match read_u32(record, 0)? {
                1 => SubjectKind::NamedAdapter,
                2 => SubjectKind::BlockBody,
                _ => return None,
            },
            disposition: match read_u32(record, 4)? {
                1 => Disposition::Synchronous,
                2 => Disposition::Resumable,
                _ => return None,
            },
            subject_index: read_u32(record, 8)?,
            language_arity: read_u32(record, 12)?,
            capture_count: read_u32(record, 16)?,
            semantic_digest: record.get(32..64)?.try_into().ok()?,
            selector,
        })
    }
}

struct RowSpec {
    kind: u32,
    disposition: Disposition,
    subject: u32,
    arity: u32,
    captures: u32,
    selector_length: usize,
    digest: [u8; 32],
}

pub fn encode(
    programs_bytes: &[u8],
    bindings_bytes: &[u8],
    named: &[Disposition],
    blocks: &[Disposition],
) -> Result<Vec<u8>, Error> {
    let programs = portable_inventory::inspect(programs_bytes)?;
    let bindings = code_binding_inventory::inspect(bindings_bytes)?;
    if named.len() != bindings.binding_count as usize
        || blocks.len() != programs.program_count as usize
    {
        return Err(Error::Format);
    }
    let row_count = named
        .len()
        .checked_add(blocks.len())
        .ok_or(Error::Quantity)?;
    if row_count > 524_288 {
        return Err(Error::Quantity);
    }
    let mut selectors_bytes = 0usize;
    for index in 0..bindings.binding_count as usize {
        selectors_bytes = selectors_bytes
            .checked_add(bindings.binding(index).ok_or(Error::Format)?.selector.len())
            .ok_or(Error::Quantity)?;
    }
    for index in 0..programs.program_count as usize {
        let program =
            crate::portable_program::inspect(programs.program(index).ok_or(Error::Format)?)?;
        selectors_bytes = selectors_bytes
            .checked_add(block_selector_len(program.parameter_count)?)
            .ok_or(Error::Quantity)?;
    }
    let selectors_offset = ROWS_OFFSET
        .checked_add(row_count.checked_mul(64).ok_or(Error::Quantity)?)
        .ok_or(Error::Quantity)?;
    let total_bytes = selectors_offset
        .checked_add(selectors_bytes)
        .ok_or(Error::Quantity)?;
    if total_bytes > 64 << 20 {
        return Err(Error::Quantity);
    }
    let row_count_u32 = u32::try_from(row_count).map_err(|_| Error::Quantity)?;
    let selectors_u32 = u32::try_from(selectors_bytes).map_err(|_| Error::Quantity)?;

    let mut output = Vec::with_capacity(total_bytes);
    output.extend_from_slice(DOMAIN);
    output.resize(32, 0);
    push_u32(&mut output, 1);
    push_u32(&mut output, 64);
    push_u64(&mut output, total_bytes as u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, 3);
    push_u32(&mut output, 32);
    push_u32(&mut output, 0);
    for (kind, stride, count, offset) in [
        (1, 64, 1, METADATA_OFFSET as u64),
        (2, 64, row_count_u32, ROWS_OFFSET as u64),
        (3, 1, selectors_u32, selectors_offset as u64),
    ] {
        push_u32(&mut output, kind);
        push_u32(&mut output, stride);
        push_u32(&mut output, count);
        push_u32(&mut output, 0);
        push_u64(&mut output, offset);
        push_u64(&mut output, u64::from(stride) * u64::from(count));
    }
    output.extend_from_slice(&Sha256::digest(programs_bytes));
    output.extend_from_slice(&bindings.semantic_digest);

    for (index, disposition) in named.iter().enumerate() {
        let binding = bindings.binding(index).ok_or(Error::Format)?;
        push_row(
            &mut output,
            RowSpec {
                kind: 1,
                disposition: *disposition,
                subject: index as u32,
                arity: binding.arity,
                captures: 0,
                selector_length: binding.selector.len(),
                digest: binding.semantic_digest,
            },
        )?;
    }
    for (index, disposition) in blocks.iter().enumerate() {
        let program_bytes = programs.program(index).ok_or(Error::Format)?;
        let program = crate::portable_program::inspect(program_bytes)?;
        push_row(
            &mut output,
            RowSpec {
                kind: 2,
                disposition: *disposition,
                subject: index as u32,
                arity: program.parameter_count,
                captures: program.capture_count,
                selector_length: block_selector_len(program.parameter_count)?,
                digest: Sha256::digest(program_bytes).into(),
            },
        )?;
    }
    for index in 0..bindings.binding_count as usize {
        output.extend_from_slice(bindings.binding(index).ok_or(Error::Format)?.selector);
    }
    for index in 0..programs.program_count as usize {
        let program =
            crate::portable_program::inspect(programs.program(index).ok_or(Error::Format)?)?;
        push_block_selector(&mut output, program.parameter_count)?;
    }
    debug_assert_eq!(output.len(), total_bytes);
    Ok(output)
}

pub fn inspect<'a>(
    bytes: &'a [u8],
    programs_bytes: &[u8],
    bindings_bytes: &[u8],
) -> Result<Selection<'a>, Error> {
    let programs = portable_inventory::inspect(programs_bytes)?;
    let bindings = code_binding_inventory::inspect(bindings_bytes)?;
    if bytes.len() < ROWS_OFFSET
        || bytes.len() > 64 << 20
        || bytes.get(..DOMAIN.len()) != Some(DOMAIN)
        || bytes
            .get(DOMAIN.len()..32)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        || read_u32(bytes, 32) != Some(1)
        || read_u32(bytes, 36) != Some(64)
        || read_u64(bytes, 40) != Some(bytes.len() as u64)
        || read_u32(bytes, 48) != Some(0)
        || read_u32(bytes, 52) != Some(3)
        || read_u32(bytes, 56) != Some(32)
        || read_u32(bytes, 60) != Some(0)
    {
        return Err(Error::Format);
    }
    let expected_rows = bindings
        .binding_count
        .checked_add(programs.program_count)
        .ok_or(Error::Quantity)?;
    let row_count = read_u32(bytes, 104).ok_or(Error::Format)?;
    let selector_count = read_u32(bytes, 136).ok_or(Error::Format)?;
    let selector_offset = ROWS_OFFSET
        .checked_add(row_count as usize * 64)
        .ok_or(Error::Quantity)?;
    if row_count != expected_rows
        || row_count > 524_288
        || read_u32(bytes, 64) != Some(1)
        || read_u32(bytes, 68) != Some(64)
        || read_u32(bytes, 72) != Some(1)
        || read_u32(bytes, 76) != Some(0)
        || read_u64(bytes, 80) != Some(METADATA_OFFSET as u64)
        || read_u64(bytes, 88) != Some(64)
        || read_u32(bytes, 96) != Some(2)
        || read_u32(bytes, 100) != Some(64)
        || read_u32(bytes, 108) != Some(0)
        || read_u64(bytes, 112) != Some(ROWS_OFFSET as u64)
        || read_u64(bytes, 120) != Some(row_count as u64 * 64)
        || read_u32(bytes, 128) != Some(3)
        || read_u32(bytes, 132) != Some(1)
        || read_u32(bytes, 140) != Some(0)
        || read_u64(bytes, 144) != Some(selector_offset as u64)
        || read_u64(bytes, 152) != Some(selector_count as u64)
        || selector_offset + selector_count as usize != bytes.len()
        || bytes.get(160..192) != Some(Sha256::digest(programs_bytes).as_slice())
        || bytes.get(192..224) != Some(bindings.semantic_digest.as_slice())
    {
        return Err(Error::Format);
    }
    let selectors = bytes.get(selector_offset..).ok_or(Error::Format)?;
    let mut selector_cursor = 0usize;
    let mut selector_offsets = Vec::with_capacity(row_count as usize);
    for index in 0..row_count as usize {
        let base = ROWS_OFFSET + index * 64;
        let expected_kind = if index < bindings.binding_count as usize {
            1
        } else {
            2
        };
        let subject = if expected_kind == 1 {
            index
        } else {
            index - bindings.binding_count as usize
        };
        let (arity, captures, expected_selector, digest) = if expected_kind == 1 {
            let binding = bindings.binding(subject).ok_or(Error::Format)?;
            (
                binding.arity,
                0,
                binding.selector.to_vec(),
                binding.semantic_digest,
            )
        } else {
            let program_bytes = programs.program(subject).ok_or(Error::Format)?;
            let program = crate::portable_program::inspect(program_bytes)?;
            let mut selector = Vec::new();
            push_block_selector(&mut selector, program.parameter_count)?;
            (
                program.parameter_count,
                program.capture_count,
                selector,
                Sha256::digest(program_bytes).into(),
            )
        };
        let length = read_u32(bytes, base + 20).ok_or(Error::Format)? as usize;
        let end = selector_cursor.checked_add(length).ok_or(Error::Quantity)?;
        if read_u32(bytes, base) != Some(expected_kind)
            || !matches!(read_u32(bytes, base + 4), Some(1 | 2))
            || read_u32(bytes, base + 8) != u32::try_from(subject).ok()
            || read_u32(bytes, base + 12) != Some(arity)
            || read_u32(bytes, base + 16) != Some(captures)
            || read_u32(bytes, base + 24) != Some(0)
            || read_u32(bytes, base + 28) != Some(0)
            || bytes.get(base + 32..base + 64) != Some(digest.as_slice())
            || selectors.get(selector_cursor..end) != Some(expected_selector.as_slice())
        {
            return Err(Error::Format);
        }
        selector_offsets.push((selector_offset + selector_cursor, length));
        selector_cursor = end;
    }
    if selector_cursor != selectors.len() {
        return Err(Error::Format);
    }
    Ok(Selection {
        bytes,
        row_count,
        selector_offsets,
    })
}

/// Validate the three frozen 60L inputs and derive their raw portable
/// image-code authority without requiring image framing.
pub fn authority(
    programs_bytes: &[u8],
    bindings_bytes: &[u8],
    selection_bytes: &[u8],
) -> Result<[u8; 32], Error> {
    let bindings = code_binding_inventory::inspect(bindings_bytes)?;
    inspect(selection_bytes, programs_bytes, bindings_bytes)?;
    let mut hash = Sha256::new();
    hash.update(AUTHORITY_DOMAIN);
    hash.update(Sha256::digest(programs_bytes));
    hash.update(bindings.semantic_digest);
    hash.update(Sha256::digest(selection_bytes));
    Ok(hash.finalize().into())
}

fn push_row(output: &mut Vec<u8>, row: RowSpec) -> Result<(), Error> {
    push_u32(output, row.kind);
    push_u32(output, row.disposition as u32);
    push_u32(output, row.subject);
    push_u32(output, row.arity);
    push_u32(output, row.captures);
    push_u32(
        output,
        u32::try_from(row.selector_length).map_err(|_| Error::Quantity)?,
    );
    push_u32(output, 0);
    push_u32(output, 0);
    output.extend_from_slice(&row.digest);
    Ok(())
}

fn block_selector_len(arity: u32) -> Result<usize, Error> {
    if arity > 4096 {
        return Err(Error::Quantity);
    }
    if arity == 0 {
        Ok(5)
    } else {
        (arity as usize).checked_mul(6).ok_or(Error::Quantity)
    }
}

fn push_block_selector(output: &mut Vec<u8>, arity: u32) -> Result<(), Error> {
    if arity == 0 {
        output.extend_from_slice(b"value");
        return Ok(());
    }
    if arity > 4096 {
        return Err(Error::Quantity);
    }
    for _ in 0..arity {
        output.extend_from_slice(b"value:");
    }
    Ok(())
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn empty_selection_has_frozen_metadata_and_bytes() {
        let programs = crate::portable_inventory::encode_closure(&[]).unwrap();
        let bindings = crate::code_binding_inventory::encode(&[]).unwrap();
        let bytes = super::encode(&programs, &bindings, &[], &[]).unwrap();
        assert_eq!(bytes.len(), 224);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x32, 0x4d, 0x0a, 0x48, 0x41, 0xad, 0x19, 0x4a, 0x13, 0xe9, 0xb4, 0x26, 0x5f, 0x80,
                0xe5, 0x48, 0xcd, 0x3c, 0x9a, 0xf9, 0x40, 0x85, 0x63, 0x51, 0xca, 0xd1, 0x1b, 0xe0,
                0xad, 0x0c, 0xda, 0x5f,
            ]
        );
        assert_eq!(
            super::inspect(&bytes, &programs, &bindings)
                .unwrap()
                .row_count,
            0
        );
    }

    #[test]
    fn mixed_selection_covers_every_binding_and_program_in_order() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let bytes = super::encode(
            programs,
            bindings,
            &[
                super::Disposition::Synchronous,
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
            &[
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
        )
        .unwrap();
        assert_eq!(bytes.len(), 566);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xed, 0xf4, 0x88, 0x69, 0x53, 0x00, 0x4e, 0x7f, 0x1b, 0xab, 0xa0, 0x7d, 0x08, 0xdb,
                0xe3, 0x51, 0xa2, 0xbe, 0xca, 0x25, 0x79, 0x4f, 0x40, 0x5d, 0x4c, 0xb1, 0xb9, 0xc3,
                0x1e, 0xb1, 0x93, 0x0c,
            ]
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "selection-mixed.bin"
            ))
        );
        assert_eq!(
            super::inspect(&bytes, programs, bindings)
                .unwrap()
                .row_count,
            5
        );
    }

    #[test]
    fn shared_cyclic_selection_matches_independent_bytes() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-shared-cyclic.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-empty.bin"
        ));
        let bytes = super::encode(
            programs,
            bindings,
            &[],
            &[
                super::Disposition::Synchronous,
                super::Disposition::Resumable,
            ],
        )
        .unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "selection-shared-cyclic.bin"
            ))
        );
    }

    #[test]
    fn block_implementation_selection_matches_independent_bytes() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-block-implementation.bin"
        ));
        let bytes = super::encode(
            programs,
            bindings,
            &[
                super::Disposition::Synchronous,
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
            &[
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
        )
        .unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "selection-block-implementation.bin"
            ))
        );
    }

    #[test]
    fn physical_descriptor_hashes_are_outside_portable_selection_authority() {
        let first = include_bytes!(concat!(
            "../../../abi/fixtures/stackless-s2/",
            "frame-descriptor-call.bin"
        ));
        let rebuilt = include_bytes!(concat!(
            "../../../abi/fixtures/stackless-s2/",
            "frame-descriptor-colored-reuse.bin"
        ));
        assert_ne!(Sha256::digest(first), Sha256::digest(rebuilt));

        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let selection = super::encode(
            programs,
            bindings,
            &[
                super::Disposition::Synchronous,
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
            &[
                super::Disposition::Resumable,
                super::Disposition::Synchronous,
            ],
        )
        .unwrap();
        assert_eq!(
            selection,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "selection-mixed.bin"
            ))
        );
        assert!(super::inspect(&selection, programs, bindings).is_ok());
    }

    #[test]
    fn every_stored_selection_truncation_rejects_and_mutations_do_not_panic() {
        let empty_programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-empty.bin"
        ));
        let empty_bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-empty.bin"
        ));
        let mixed_programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let mixed_bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let shared_programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-shared-cyclic.bin"
        ));
        let fixtures: &[(&[u8], &[u8], &[u8])] = &[
            (
                include_bytes!(concat!(
                    "../../../abi/fixtures/portable-code-v1/",
                    "selection-block-implementation.bin"
                )),
                mixed_programs,
                include_bytes!(concat!(
                    "../../../abi/fixtures/portable-code-v1/",
                    "bindings-block-implementation.bin"
                )),
            ),
            (
                include_bytes!(concat!(
                    "../../../abi/fixtures/portable-code-v1/",
                    "selection-empty.bin"
                )),
                empty_programs,
                empty_bindings,
            ),
            (
                include_bytes!(concat!(
                    "../../../abi/fixtures/portable-code-v1/",
                    "selection-mixed.bin"
                )),
                mixed_programs,
                mixed_bindings,
            ),
            (
                include_bytes!(concat!(
                    "../../../abi/fixtures/portable-code-v1/",
                    "selection-shared-cyclic.bin"
                )),
                shared_programs,
                empty_bindings,
            ),
        ];
        for (fixture, programs, bindings) in fixtures {
            for end in 0..fixture.len() {
                assert!(
                    super::inspect(&fixture[..end], programs, bindings).is_err(),
                    "accepted truncation {end} of {}",
                    fixture.len()
                );
            }
            for offset in 0..fixture.len() {
                let mut mutated = fixture.to_vec();
                mutated[offset] ^= 1;
                assert!(
                    std::panic::catch_unwind(|| { super::inspect(&mutated, programs, bindings) })
                        .is_ok()
                );
            }
        }
    }
}
