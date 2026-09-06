//! Canonical pointer-free CodeBindingInventory v2 bytes.

use sha2::{Digest, Sha256};

use crate::portable_program::Error;

const DOMAIN: &[u8] = b"birnam.code-bindings.v2\0";
const ROW_DOMAIN: &[u8] = b"birnam.code-binding-row.v2\0";
const SEMANTICS_DOMAIN: &[u8] = b"birnam.code-binding-semantics.v2\0";
const HEADER_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field<'a> {
    Nil,
    True,
    False,
    Symbol(&'a [u8]),
    HeapRef(u32),
    ExportRef(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingSpec<'a> {
    pub target: Field<'a>,
    pub selector: &'a [u8],
    pub implementation: Field<'a>,
    pub provenance: Field<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inventory<'a> {
    pub binding_count: u32,
    pub semantic_digest: [u8; 32],
    rows: &'a [u8],
    fields: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding<'a> {
    pub selector: &'a [u8],
    pub arity: u32,
    pub semantic_digest: [u8; 32],
}

impl<'a> Inventory<'a> {
    pub fn binding(&self, index: usize) -> Option<Binding<'a>> {
        if index >= self.binding_count as usize {
            return None;
        }
        let mut cursor = 0usize;
        for current in 0..=index {
            let base = current.checked_mul(32)?;
            let lengths = [
                read_u32(self.rows, base)? as usize,
                read_u32(self.rows, base.checked_add(4)?)? as usize,
                read_u32(self.rows, base.checked_add(8)?)? as usize,
                read_u32(self.rows, base.checked_add(12)?)? as usize,
            ];
            let target_start = cursor;
            let selector_start = target_start.checked_add(lengths[0])?;
            let implementation_start = selector_start.checked_add(lengths[1])?;
            let provenance_start = implementation_start.checked_add(lengths[2])?;
            let end = provenance_start.checked_add(lengths[3])?;
            if current == index {
                let target = self.fields.get(target_start..selector_start)?;
                let selector_field = self.fields.get(selector_start..implementation_start)?;
                let implementation = self.fields.get(implementation_start..provenance_start)?;
                let selector = selector_bytes(selector_field);
                let arity = read_u32(self.rows, base.checked_add(16)?)?;
                return Some(Binding {
                    selector,
                    arity,
                    semantic_digest: row_semantic_digest(
                        u32::try_from(index).ok()?,
                        target,
                        selector_field,
                        implementation,
                        arity,
                    )
                    .ok()?,
                });
            }
            cursor = end;
        }
        None
    }

    pub fn binding_spec(&self, index: usize) -> Option<BindingSpec<'a>> {
        if index >= self.binding_count as usize {
            return None;
        }
        let mut cursor = 0usize;
        for current in 0..=index {
            let base = current.checked_mul(32)?;
            let lengths = [
                read_u32(self.rows, base)? as usize,
                read_u32(self.rows, base.checked_add(4)?)? as usize,
                read_u32(self.rows, base.checked_add(8)?)? as usize,
                read_u32(self.rows, base.checked_add(12)?)? as usize,
            ];
            let target_start = cursor;
            let selector_start = target_start.checked_add(lengths[0])?;
            let implementation_start = selector_start.checked_add(lengths[1])?;
            let provenance_start = implementation_start.checked_add(lengths[2])?;
            let end = provenance_start.checked_add(lengths[3])?;
            if current == index {
                return Some(BindingSpec {
                    target: decode_field(self.fields.get(target_start..selector_start)?)?,
                    selector: selector_bytes(
                        self.fields.get(selector_start..implementation_start)?,
                    ),
                    implementation: decode_field(
                        self.fields.get(implementation_start..provenance_start)?,
                    )?,
                    provenance: decode_field(self.fields.get(provenance_start..end)?)?,
                });
            }
            cursor = end;
        }
        None
    }

    pub fn validate_heap_references(&self, heap_count: u32) -> Result<(), Error> {
        let mut cursor = 0usize;
        for index in 0..self.binding_count as usize {
            let base = index.checked_mul(32).ok_or(Error::Quantity)?;
            for field_index in 0..4 {
                let length =
                    read_u32(self.rows, base + field_index * 4).ok_or(Error::Format)? as usize;
                let end = cursor.checked_add(length).ok_or(Error::Quantity)?;
                let field = self.fields.get(cursor..end).ok_or(Error::Format)?;
                if field.first() == Some(&7)
                    && read_u32(field, 1).is_none_or(|reference| reference >= heap_count)
                {
                    return Err(Error::Format);
                }
                cursor = end;
            }
        }
        Ok(())
    }
}

fn decode_field(bytes: &[u8]) -> Option<Field<'_>> {
    match *bytes.first()? {
        0 if bytes.len() == 1 => Some(Field::Nil),
        1 if bytes.len() == 1 => Some(Field::True),
        2 if bytes.len() == 1 => Some(Field::False),
        5 | 9 => {
            let length = read_u32(bytes, 1)? as usize;
            let value = bytes.get(5..)?.get(..length)?;
            if bytes.len() != 5usize.checked_add(length)? {
                return None;
            }
            match bytes[0] {
                5 => Some(Field::Symbol(value)),
                9 => Some(Field::ExportRef(value)),
                _ => None,
            }
        }
        7 if bytes.len() == 5 => Some(Field::HeapRef(read_u32(bytes, 1)?)),
        _ => None,
    }
}

struct EncodedBinding {
    target: Vec<u8>,
    selector: Vec<u8>,
    implementation: Vec<u8>,
    provenance: Vec<u8>,
    arity: u32,
}

pub fn encode(bindings: &[BindingSpec<'_>]) -> Result<Vec<u8>, Error> {
    if bindings.len() > 262_144 {
        return Err(Error::Quantity);
    }
    let mut encoded = Vec::with_capacity(bindings.len());
    for binding in bindings {
        validate_name(binding.selector)?;
        if !matches!(
            binding.target,
            Field::Nil
                | Field::True
                | Field::False
                | Field::Symbol(_)
                | Field::HeapRef(_)
                | Field::ExportRef(_)
        ) || !matches!(
            binding.implementation,
            Field::Nil
                | Field::True
                | Field::False
                | Field::Symbol(_)
                | Field::HeapRef(_)
                | Field::ExportRef(_)
        ) || !matches!(
            binding.provenance,
            Field::Nil | Field::HeapRef(_) | Field::ExportRef(_)
        ) {
            return Err(Error::Format);
        }
        encoded.push(EncodedBinding {
            target: encode_field(binding.target)?,
            selector: encode_field(Field::Symbol(binding.selector))?,
            implementation: encode_field(binding.implementation)?,
            provenance: encode_field(binding.provenance)?,
            arity: selector_arity(binding.selector)?,
        });
    }
    encoded.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then_with(|| selector_bytes(&left.selector).cmp(selector_bytes(&right.selector)))
    });
    for pair in encoded.windows(2) {
        if pair[0].target == pair[1].target
            && selector_bytes(&pair[0].selector) == selector_bytes(&pair[1].selector)
        {
            return Err(Error::Format);
        }
    }

    let rows_bytes = encoded.len().checked_mul(32).ok_or(Error::Quantity)?;
    let fields_offset = align8(
        HEADER_BYTES
            .checked_add(rows_bytes)
            .ok_or(Error::Quantity)?,
    )
    .ok_or(Error::Quantity)?;
    let fields_bytes = encoded.iter().try_fold(0usize, |total, binding| {
        total
            .checked_add(binding.target.len())
            .and_then(|value| value.checked_add(binding.selector.len()))
            .and_then(|value| value.checked_add(binding.implementation.len()))
            .and_then(|value| value.checked_add(binding.provenance.len()))
            .ok_or(Error::Quantity)
    })?;
    let total_bytes = fields_offset
        .checked_add(fields_bytes)
        .ok_or(Error::Quantity)?;
    if total_bytes > 64 << 20 {
        return Err(Error::Quantity);
    }
    let row_count = u32::try_from(encoded.len()).map_err(|_| Error::Quantity)?;
    let fields_bytes = u32::try_from(fields_bytes).map_err(|_| Error::Quantity)?;

    let mut output = common_prefix(total_bytes);
    for (kind, stride, count, offset) in [
        (1, 32, row_count, HEADER_BYTES as u64),
        (2, 1, fields_bytes, fields_offset as u64),
    ] {
        push_u32(&mut output, kind);
        push_u32(&mut output, stride);
        push_u32(&mut output, count);
        push_u32(&mut output, 0);
        push_u64(&mut output, offset);
        push_u64(&mut output, u64::from(stride) * u64::from(count));
    }
    for binding in &encoded {
        for field in [
            &binding.target,
            &binding.selector,
            &binding.implementation,
            &binding.provenance,
        ] {
            push_u32(
                &mut output,
                u32::try_from(field.len()).map_err(|_| Error::Quantity)?,
            );
        }
        push_u32(&mut output, binding.arity);
        push_u32(&mut output, 0);
        push_u64(&mut output, 0);
    }
    output.resize(fields_offset, 0);
    for binding in encoded {
        output.extend_from_slice(&binding.target);
        output.extend_from_slice(&binding.selector);
        output.extend_from_slice(&binding.implementation);
        output.extend_from_slice(&binding.provenance);
    }
    debug_assert_eq!(output.len(), total_bytes);
    Ok(output)
}

pub fn inspect(bytes: &[u8]) -> Result<Inventory<'_>, Error> {
    validate_common(bytes)?;
    let row_count = read_u32(bytes, 72).ok_or(Error::Format)?;
    let fields_bytes = read_u32(bytes, 104).ok_or(Error::Format)?;
    if row_count > 262_144 {
        return Err(Error::Quantity);
    }
    let rows_end = HEADER_BYTES
        .checked_add(row_count as usize * 32)
        .ok_or(Error::Quantity)?;
    let fields_offset = align8(rows_end).ok_or(Error::Quantity)?;
    if read_u32(bytes, 64) != Some(1)
        || read_u32(bytes, 68) != Some(32)
        || read_u32(bytes, 76) != Some(0)
        || read_u64(bytes, 80) != Some(128)
        || read_u64(bytes, 88) != Some(row_count as u64 * 32)
        || read_u32(bytes, 96) != Some(2)
        || read_u32(bytes, 100) != Some(1)
        || read_u32(bytes, 108) != Some(0)
        || read_u64(bytes, 112) != Some(fields_offset as u64)
        || read_u64(bytes, 120) != Some(fields_bytes as u64)
        || fields_offset + fields_bytes as usize != bytes.len()
        || bytes
            .get(rows_end..fields_offset)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
    {
        return Err(Error::Format);
    }
    let rows = bytes.get(HEADER_BYTES..rows_end).ok_or(Error::Format)?;
    let fields = bytes.get(fields_offset..).ok_or(Error::Format)?;
    let mut cursor = 0usize;
    let mut previous: Option<(&[u8], &[u8])> = None;
    let mut row_digests = Vec::with_capacity(row_count as usize);
    for index in 0..row_count as usize {
        let base = index * 32;
        let lengths = [
            read_u32(rows, base).ok_or(Error::Format)? as usize,
            read_u32(rows, base + 4).ok_or(Error::Format)? as usize,
            read_u32(rows, base + 8).ok_or(Error::Format)? as usize,
            read_u32(rows, base + 12).ok_or(Error::Format)? as usize,
        ];
        let mut values = [&fields[0..0]; 4];
        for (slot, length) in lengths.into_iter().enumerate() {
            let end = cursor.checked_add(length).ok_or(Error::Quantity)?;
            values[slot] = fields.get(cursor..end).ok_or(Error::Format)?;
            cursor = end;
        }
        let target = parse_field(values[0])?;
        let selector = parse_field(values[1])?;
        let implementation = parse_field(values[2])?;
        let provenance = parse_field(values[3])?;
        let selector_name = selector.data.ok_or(Error::Format)?;
        let arity = selector_arity(selector_name)?;
        if !matches!(target.tag, 0 | 1 | 2 | 5 | 7 | 9)
            || selector.tag != 5
            || !matches!(implementation.tag, 0 | 1 | 2 | 5 | 7 | 9)
            || !matches!(provenance.tag, 0 | 7 | 9)
            || read_u32(rows, base + 16) != Some(arity)
            || read_u32(rows, base + 20) != Some(0)
            || read_u64(rows, base + 24) != Some(0)
            || previous.is_some_and(|prior| prior >= (values[0], selector_name))
        {
            return Err(Error::Format);
        }
        previous = Some((values[0], selector_name));
        row_digests.push(row_semantic_digest(
            index as u32,
            values[0],
            values[1],
            values[2],
            arity,
        )?);
    }
    if cursor != fields.len() {
        return Err(Error::Format);
    }
    let mut semantics = Sha256::new();
    semantics.update(SEMANTICS_DOMAIN);
    semantics.update(row_count.to_le_bytes());
    for digest in row_digests {
        semantics.update(digest);
    }
    Ok(Inventory {
        binding_count: row_count,
        semantic_digest: semantics.finalize().into(),
        rows,
        fields,
    })
}

fn row_semantic_digest(
    index: u32,
    target: &[u8],
    selector: &[u8],
    implementation: &[u8],
    arity: u32,
) -> Result<[u8; 32], Error> {
    let mut hash = Sha256::new();
    hash.update(ROW_DOMAIN);
    hash.update(index.to_le_bytes());
    for length in [target.len(), selector.len(), implementation.len()] {
        hash.update(
            u32::try_from(length)
                .map_err(|_| Error::Quantity)?
                .to_le_bytes(),
        );
    }
    hash.update(arity.to_le_bytes());
    hash.update(target);
    hash.update(selector);
    hash.update(implementation);
    Ok(hash.finalize().into())
}

#[derive(Clone, Copy)]
struct ParsedField<'a> {
    tag: u8,
    data: Option<&'a [u8]>,
}

fn parse_field(bytes: &[u8]) -> Result<ParsedField<'_>, Error> {
    let tag = *bytes.first().ok_or(Error::Format)?;
    let data = match tag {
        0..=2 if bytes.len() == 1 => None,
        7 if bytes.len() == 5 => None,
        5 | 9 => {
            let length = read_u32(bytes, 1).ok_or(Error::Format)? as usize;
            let data = bytes
                .get(5..)
                .filter(|data| data.len() == length)
                .ok_or(Error::Format)?;
            validate_name(data)?;
            Some(data)
        }
        _ => return Err(Error::Format),
    };
    Ok(ParsedField { tag, data })
}

fn encode_field(field: Field<'_>) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    match field {
        Field::Nil => output.push(0),
        Field::True => output.push(1),
        Field::False => output.push(2),
        Field::Symbol(bytes) | Field::ExportRef(bytes) => {
            validate_name(bytes)?;
            output.push(match field {
                Field::Symbol(_) => 5,
                Field::ExportRef(_) => 9,
                _ => unreachable!(),
            });
            push_u32(
                &mut output,
                u32::try_from(bytes.len()).map_err(|_| Error::Quantity)?,
            );
            output.extend_from_slice(bytes);
        }
        Field::HeapRef(index) => {
            output.push(7);
            push_u32(&mut output, index);
        }
    }
    Ok(output)
}

fn selector_bytes(field: &[u8]) -> &[u8] {
    &field[5..]
}

fn selector_arity(selector: &[u8]) -> Result<u32, Error> {
    validate_name(selector)?;
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return u32::try_from(colons).map_err(|_| Error::Quantity);
    }
    Ok(
        if selector[0].is_ascii_alphabetic() || selector[0] == b'_' {
            0
        } else {
            1
        },
    )
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty() || name.contains(&0) || name.len() > 1 << 20 {
        return Err(Error::Name);
    }
    Ok(())
}

fn common_prefix(total_bytes: usize) -> Vec<u8> {
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
    output
}

fn validate_common(bytes: &[u8]) -> Result<(), Error> {
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
    Ok(())
}

use super::wire::{push_u32, push_u64, read_u32_checked as read_u32, read_u64_checked as read_u64};

fn align8(value: usize) -> Option<usize> {
    value.checked_add(7).map(|aligned| aligned & !7)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn empty_bindings_have_frozen_bytes_and_semantic_digest() {
        let bytes = super::encode(&[]).unwrap();
        assert_eq!(bytes.len(), 128);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xa9, 0xf3, 0x42, 0x70, 0xec, 0x76, 0xee, 0xf4, 0x4c, 0xcd, 0xe2, 0x46, 0x37, 0x50,
                0x35, 0x8b, 0xcc, 0xf1, 0x49, 0xfa, 0x6e, 0x0d, 0x13, 0x24, 0x32, 0x97, 0xeb, 0xb3,
                0x6c, 0x12, 0x75, 0x59,
            ]
        );
        assert_eq!(
            super::inspect(&bytes).unwrap().semantic_digest,
            [
                0x15, 0x15, 0xd2, 0xdb, 0xa1, 0xda, 0x6e, 0x7e, 0x81, 0xa5, 0xc9, 0xcf, 0xb6, 0x0b,
                0x5b, 0xbe, 0x63, 0xbf, 0x57, 0x42, 0x2c, 0xe6, 0xea, 0x65, 0xce, 0x64, 0x7e, 0xe8,
                0xb7, 0xd4, 0x8b, 0x4d,
            ]
        );
    }

    #[test]
    fn mixed_bindings_sort_canonically_and_exclude_provenance_from_semantics() {
        use super::{BindingSpec, Field};

        let bindings = [
            BindingSpec {
                target: Field::Symbol(b"Type"),
                selector: b"run",
                implementation: Field::HeapRef(7),
                provenance: Field::HeapRef(11),
            },
            BindingSpec {
                target: Field::HeapRef(8),
                selector: b"+",
                implementation: Field::ExportRef(b"Plus"),
                provenance: Field::Nil,
            },
            BindingSpec {
                target: Field::Nil,
                selector: b"at:put:",
                implementation: Field::Symbol(b"method"),
                provenance: Field::HeapRef(3),
            },
        ];
        let bytes = super::encode(&bindings).unwrap();
        assert_eq!(bytes.len(), 301);
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-mixed.bin"
            ))
        );
        let inventory = super::inspect(&bytes).unwrap();
        assert_eq!(inventory.binding_count, 3);
        assert_eq!(
            inventory.semantic_digest,
            [
                0x05, 0x8a, 0xb6, 0x70, 0x04, 0x52, 0x47, 0xf6, 0x67, 0xce, 0x84, 0x2e, 0x92, 0xf3,
                0xe7, 0xfb, 0x4e, 0xd3, 0xf4, 0xdb, 0xae, 0x49, 0xf2, 0x16, 0x53, 0x94, 0x6e, 0x2c,
                0x45, 0x07, 0x0f, 0xb8,
            ]
        );

        let mut changed = bindings;
        changed[0].provenance = Field::HeapRef(10);
        let changed_bytes = super::encode(&changed).unwrap();
        assert_ne!(bytes, changed_bytes);
        assert_eq!(
            changed_bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-provenance-only-change.bin"
            ))
        );
        assert_eq!(
            super::inspect(&changed_bytes).unwrap().semantic_digest,
            inventory.semantic_digest
        );

        let mut block_implementation = bindings;
        block_implementation[0].implementation = Field::HeapRef(1);
        let block_bytes = super::encode(&block_implementation).unwrap();
        assert_eq!(
            block_bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-block-implementation.bin"
            ))
        );
        assert_ne!(
            super::inspect(&block_bytes).unwrap().semantic_digest,
            inventory.semantic_digest
        );
    }

    #[test]
    fn every_stored_binding_inventory_truncation_rejects_and_mutations_do_not_panic() {
        let fixtures: &[&[u8]] = &[
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-block-implementation.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-empty.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-mixed.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "bindings-provenance-only-change.bin"
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
