//! Canonical pointer-free PortableProgramInventory v1 bytes.

use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::portable_program::{self, Error};

const DOMAIN: &[u8] = b"birnam.portable-inventory.v1\0";
const HEADER_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inventory<'a> {
    pub program_count: u32,
    rows: &'a [u8],
    programs: &'a [u8],
}

impl Inventory<'_> {
    pub fn program(&self, index: usize) -> Option<&[u8]> {
        if index >= self.program_count as usize {
            return None;
        }
        let mut start = 0usize;
        for current in 0..=index {
            let length = read_u32(self.rows, current.checked_mul(48)?.checked_add(32)?)? as usize;
            let end = start.checked_add(length)?;
            let program = self.programs.get(start..end)?;
            if current == index {
                return Some(program);
            }
            start = end;
        }
        None
    }
}

struct Row<'a> {
    digest: [u8; 32],
    bytes: &'a [u8],
    parameter_count: u32,
    capture_count: u32,
}

pub fn encode_closure<'a>(roots: &'a [&'a [u8]]) -> Result<Vec<u8>, Error> {
    let mut pending = roots.to_vec();
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    while let Some(bytes) = pending.pop() {
        if !seen.insert(bytes) {
            continue;
        }
        if rows.len() >= 262_144 {
            return Err(Error::Quantity);
        }
        let program = portable_program::inspect(bytes)?;
        for index in 0..program.node_count as usize {
            let node = program.node(index).ok_or(Error::Format)?;
            if node.kind == 11 {
                pending.push(node.data);
            }
        }
        rows.push(Row {
            digest: Sha256::digest(bytes).into(),
            bytes,
            parameter_count: program.parameter_count,
            capture_count: program.capture_count,
        });
    }
    rows.sort_by(|left, right| {
        compare_digest_bytes(&left.digest, left.bytes, &right.digest, right.bytes)
    });

    let rows_bytes = rows.len().checked_mul(48).ok_or(Error::Quantity)?;
    let programs_offset = align8(
        HEADER_BYTES
            .checked_add(rows_bytes)
            .ok_or(Error::Quantity)?,
    )
    .ok_or(Error::Quantity)?;
    let programs_bytes = rows.iter().try_fold(0usize, |total, row| {
        total.checked_add(row.bytes.len()).ok_or(Error::Quantity)
    })?;
    let total_bytes = programs_offset
        .checked_add(programs_bytes)
        .ok_or(Error::Quantity)?;
    if total_bytes > 64 << 20 {
        return Err(Error::Quantity);
    }
    let row_count = u32::try_from(rows.len()).map_err(|_| Error::Quantity)?;
    let programs_bytes = u32::try_from(programs_bytes).map_err(|_| Error::Quantity)?;

    let mut output = Vec::with_capacity(total_bytes);
    output.extend_from_slice(DOMAIN);
    output.resize(32, 0);
    push_u32(&mut output, 1);
    push_u32(&mut output, 64);
    push_u64(&mut output, total_bytes as u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, 2);
    push_u32(&mut output, 32);
    push_u32(&mut output, 0);
    for (kind, stride, count, offset) in [
        (1, 48, row_count, HEADER_BYTES as u64),
        (2, 1, programs_bytes, programs_offset as u64),
    ] {
        push_u32(&mut output, kind);
        push_u32(&mut output, stride);
        push_u32(&mut output, count);
        push_u32(&mut output, 0);
        push_u64(&mut output, offset);
        push_u64(&mut output, u64::from(stride) * u64::from(count));
    }
    for row in &rows {
        output.extend_from_slice(&row.digest);
        push_u32(
            &mut output,
            u32::try_from(row.bytes.len()).map_err(|_| Error::Quantity)?,
        );
        push_u32(&mut output, 0);
        push_u32(&mut output, row.parameter_count);
        push_u32(&mut output, row.capture_count);
    }
    output.resize(programs_offset, 0);
    for row in rows {
        output.extend_from_slice(row.bytes);
    }
    debug_assert_eq!(output.len(), total_bytes);
    Ok(output)
}

fn compare_digest_bytes(
    left_digest: &[u8; 32],
    left_bytes: &[u8],
    right_digest: &[u8; 32],
    right_bytes: &[u8],
) -> Ordering {
    left_digest
        .cmp(right_digest)
        .then_with(|| left_bytes.cmp(right_bytes))
}

pub fn inspect(bytes: &[u8]) -> Result<Inventory<'_>, Error> {
    if bytes.len() < HEADER_BYTES
        || bytes.len() > 64 << 20
        || bytes.get(..DOMAIN.len()) != Some(DOMAIN)
        || bytes
            .get(DOMAIN.len()..32)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        || read_u32(bytes, 32) != Some(1)
        || read_u32(bytes, 36) != Some(64)
        || read_u64(bytes, 40) != Some(bytes.len() as u64)
        || read_u32(bytes, 48) != Some(0)
        || read_u32(bytes, 52) != Some(2)
        || read_u32(bytes, 56) != Some(32)
        || read_u32(bytes, 60) != Some(0)
    {
        return Err(Error::Format);
    }
    let row_count = read_u32(bytes, 72).ok_or(Error::Format)?;
    let programs_bytes = read_u32(bytes, 104).ok_or(Error::Format)?;
    if row_count > 262_144 || programs_bytes > 64 << 20 {
        return Err(Error::Quantity);
    }
    let rows_end = HEADER_BYTES
        .checked_add(row_count as usize * 48)
        .ok_or(Error::Quantity)?;
    let programs_offset = align8(rows_end).ok_or(Error::Quantity)?;
    let total = programs_offset
        .checked_add(programs_bytes as usize)
        .ok_or(Error::Quantity)?;
    if read_u32(bytes, 64) != Some(1)
        || read_u32(bytes, 68) != Some(48)
        || read_u32(bytes, 76) != Some(0)
        || read_u64(bytes, 80) != Some(HEADER_BYTES as u64)
        || read_u64(bytes, 88) != Some(row_count as u64 * 48)
        || read_u32(bytes, 96) != Some(2)
        || read_u32(bytes, 100) != Some(1)
        || read_u32(bytes, 108) != Some(0)
        || read_u64(bytes, 112) != Some(programs_offset as u64)
        || read_u64(bytes, 120) != Some(programs_bytes as u64)
        || total != bytes.len()
        || bytes
            .get(rows_end..programs_offset)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
    {
        return Err(Error::Format);
    }
    let rows = bytes.get(HEADER_BYTES..rows_end).ok_or(Error::Format)?;
    let programs = bytes.get(programs_offset..total).ok_or(Error::Format)?;
    let mut cursor = 0usize;
    let mut previous: Option<([u8; 32], &[u8])> = None;
    for index in 0..row_count as usize {
        let base = index * 48;
        let digest: [u8; 32] = rows[base..base + 32]
            .try_into()
            .map_err(|_| Error::Format)?;
        let length = read_u32(rows, base + 32).ok_or(Error::Format)? as usize;
        let end = cursor.checked_add(length).ok_or(Error::Quantity)?;
        let program_bytes = programs.get(cursor..end).ok_or(Error::Format)?;
        let program = portable_program::inspect(program_bytes)?;
        if length == 0
            || read_u32(rows, base + 36) != Some(0)
            || read_u32(rows, base + 40) != Some(program.parameter_count)
            || read_u32(rows, base + 44) != Some(program.capture_count)
            || <[u8; 32]>::from(Sha256::digest(program_bytes)) != digest
            || previous.is_some_and(|(prior_digest, prior_bytes)| {
                (prior_digest, prior_bytes) >= (digest, program_bytes)
            })
        {
            return Err(Error::Format);
        }
        previous = Some((digest, program_bytes));
        cursor = end;
    }
    if cursor != programs.len() {
        return Err(Error::Format);
    }
    Ok(Inventory {
        program_count: row_count,
        rows,
        programs,
    })
}

pub fn inspect_exact_closure<'a>(
    bytes: &'a [u8],
    roots: &'a [&'a [u8]],
) -> Result<Inventory<'a>, Error> {
    let inventory = inspect(bytes)?;
    if encode_closure(roots)? != bytes {
        return Err(Error::Format);
    }
    Ok(inventory)
}

use super::wire::{push_u32, push_u64, read_u32_checked as read_u32, read_u64_checked as read_u64};

fn align8(value: usize) -> Option<usize> {
    value.checked_add(7).map(|aligned| aligned & !7)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn empty_inventory_has_the_frozen_128_byte_encoding() {
        let bytes = super::encode_closure(&[]).unwrap();
        assert_eq!(bytes.len(), 128);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x0f, 0xe1, 0xf4, 0x05, 0x6d, 0xbc, 0xd5, 0x37, 0x9a, 0xb2, 0xb2, 0xee, 0x62, 0xbc,
                0x28, 0x93, 0x09, 0x40, 0xe6, 0xd1, 0x9b, 0x25, 0xed, 0x2c, 0xb6, 0x9e, 0xee, 0x06,
                0x15, 0xd7, 0x73, 0x51,
            ]
        );
        assert_eq!(super::inspect(&bytes).unwrap().program_count, 0);
    }

    #[test]
    fn nested_program_closure_is_deduplicated_and_digest_sorted() {
        let outer = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "program-nested-captures.bin"
        ));
        let bytes = super::encode_closure(&[outer.as_slice()]).unwrap();
        assert_eq!(bytes.len(), 1277);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xf9, 0x35, 0x5f, 0xb5, 0xa3, 0x38, 0x08, 0x1f, 0x2a, 0x41, 0xb8, 0x13, 0xf8, 0x21,
                0xf2, 0xd6, 0x69, 0xd3, 0xac, 0xed, 0xbf, 0xcf, 0x3b, 0xd5, 0x02, 0x59, 0x07, 0x84,
                0x1a, 0x28, 0x39, 0x48,
            ]
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-nested-closure.bin"
            ))
        );
        let roots = [outer.as_slice()];
        let inventory = super::inspect_exact_closure(&bytes, &roots).unwrap();
        assert_eq!(inventory.program_count, 2);
        assert_eq!(inventory.program(0).unwrap().len(), 338);
        assert_eq!(inventory.program(1), Some(outer.as_slice()));
    }

    #[test]
    fn repeated_root_is_elided_to_the_frozen_one_row_inventory() {
        let program = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "program-nested-inner.bin"
        ));
        let bytes = super::encode_closure(&[program.as_slice(), program.as_slice()]).unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-one.bin"
            ))
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-duplicate-elided.bin"
            ))
        );
        assert_eq!(super::inspect(&bytes).unwrap().program_count, 1);
    }

    #[test]
    fn nonempty_nested_closure_matches_the_shared_cyclic_inventory() {
        let outer = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "program-nonutf8-names-block.bin"
        ));
        let bytes = super::encode_closure(&[outer.as_slice()]).unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-shared-cyclic.bin"
            ))
        );
        assert_eq!(super::inspect(&bytes).unwrap().program_count, 2);
    }

    #[test]
    fn injected_equal_digests_order_by_exact_program_bytes() {
        let digest = [0x5a; 32];
        assert_eq!(
            super::compare_digest_bytes(&digest, b"a", &digest, b"b"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            super::compare_digest_bytes(&digest, b"b", &digest, b"a"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            super::compare_digest_bytes(&digest, b"same", &digest, b"same"),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn every_stored_inventory_truncation_rejects_and_mutations_do_not_panic() {
        let fixtures: &[&[u8]] = &[
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-empty.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-one.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-duplicate-elided.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-nested-closure.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "inventory-shared-cyclic.bin"
            )),
        ];
        for fixture in fixtures {
            for end in 0..fixture.len() {
                assert!(
                    super::inspect(&fixture[..end]).is_err(),
                    "accepted truncation {end} of {}",
                    fixture.len()
                );
            }
            for offset in 0..fixture.len() {
                let mut mutated = fixture.to_vec();
                mutated[offset] ^= 1;
                assert!(std::panic::catch_unwind(|| super::inspect(&mutated)).is_ok());
            }
        }
    }
}
