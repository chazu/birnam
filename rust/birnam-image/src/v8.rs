//! Inert image-v8 format proof. This module performs structural preflight only;
//! it performs no native admission or runtime publication.

use birnam_artifact::{
    code_binding_inventory, materialization_selection, portable_inventory, portable_program,
};
use sha2::{Digest, Sha256};

pub const FORMAT_VERSION: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    CanonicalValue,
    Quantity,
    Header,
    Record,
    Authority,
    Heap,
    Facts,
    Code,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageSpec<'a> {
    pub architecture: u32,
    pub runtime_abi: u64,
    pub programs: &'a [u8],
    pub bindings: &'a [u8],
    pub selection: &'a [u8],
    pub heaps: &'a [&'a [u8]],
    pub facts: &'a [&'a [u8]],
    pub code: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    pub architecture: u32,
    pub runtime_abi: u64,
    pub heap_objects: u32,
    pub has_code: bool,
}

#[allow(dead_code)]
pub(crate) struct DecodedImage<'a> {
    pub programs: &'a [u8],
    pub bindings: &'a [u8],
    pub selection: &'a [u8],
    pub heaps: Vec<&'a [u8]>,
    pub facts: Vec<&'a [u8]>,
    pub code: Option<&'a [u8]>,
}

#[allow(dead_code)]
pub(crate) fn decode(
    bytes: &[u8],
    architecture: u32,
    runtime_abi: u64,
) -> Result<DecodedImage<'_>, Error> {
    structural_preflight(bytes, architecture, runtime_abi)?;
    let mut offset = 28usize;
    let _authority = next_record(bytes, &mut offset, 5)?;
    let (_, programs) = hashed_record(next_record(bytes, &mut offset, 7)?)?;
    let (_, bindings) = hashed_record(next_record(bytes, &mut offset, 8)?)?;
    let (_, selection) = hashed_record(next_record(bytes, &mut offset, 9)?)?;
    let mut heaps = Vec::new();
    let mut facts = Vec::new();
    let mut code = None;
    while offset < bytes.len() {
        match read_u32(bytes, offset).ok_or(Error::Record)? {
            3 if facts.is_empty() && code.is_none() => {
                heaps.push(next_record(bytes, &mut offset, 3)?);
            }
            2 if code.is_none() => facts.push(next_record(bytes, &mut offset, 2)?),
            4 if code.is_none() => code = Some(next_record(bytes, &mut offset, 4)?),
            _ => return Err(Error::Record),
        }
    }
    Ok(DecodedImage {
        programs,
        bindings,
        selection,
        heaps,
        facts,
        code,
    })
}

pub fn compose_stripped(
    architecture: u32,
    runtime_abi: u64,
    programs: &[u8],
    bindings: &[u8],
    selection: &[u8],
) -> Result<Vec<u8>, Error> {
    compose(ImageSpec {
        architecture,
        runtime_abi,
        programs,
        bindings,
        selection,
        heaps: &[],
        facts: &[],
        code: None,
    })
}

pub fn compose(spec: ImageSpec<'_>) -> Result<Vec<u8>, Error> {
    portable_inventory::inspect(spec.programs).map_err(|_| Error::CanonicalValue)?;
    let binding_inventory =
        code_binding_inventory::inspect(spec.bindings).map_err(|_| Error::CanonicalValue)?;
    materialization_selection::inspect(spec.selection, spec.programs, spec.bindings)
        .map_err(|_| Error::CanonicalValue)?;
    let programs_hash: [u8; 32] = Sha256::digest(spec.programs).into();
    let selection_hash: [u8; 32] = Sha256::digest(spec.selection).into();
    let authority = authority(
        programs_hash,
        binding_inventory.semantic_digest,
        selection_hash,
    );

    let mut total = 28usize
        .checked_add(12 + 64)
        .and_then(|value| value.checked_add(12 + 40 + spec.programs.len()))
        .and_then(|value| value.checked_add(12 + 40 + spec.bindings.len()))
        .and_then(|value| value.checked_add(12 + 40 + spec.selection.len()))
        .ok_or(Error::Quantity)?;
    for payload in spec.heaps.iter().chain(spec.facts.iter()) {
        total = total
            .checked_add(12)
            .and_then(|value| value.checked_add(payload.len()))
            .ok_or(Error::Quantity)?;
    }
    if let Some(unit) = spec.code {
        if unit.is_empty() {
            return Err(Error::Code);
        }
        total = total
            .checked_add(12 + 8 + 64)
            .and_then(|value| value.checked_add(unit.len()))
            .ok_or(Error::Quantity)?;
    }
    if total > 1 << 30 {
        return Err(Error::Quantity);
    }
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(b"BIRNAMIM");
    output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&0x0102_0304u32.to_le_bytes());
    output.extend_from_slice(&spec.architecture.to_le_bytes());
    output.extend_from_slice(&spec.runtime_abi.to_le_bytes());
    let authority_spelling = authority_hex(authority);
    push_record(&mut output, 5, &authority_spelling)?;
    push_hashed_record(&mut output, 7, spec.programs)?;
    push_hashed_record(&mut output, 8, spec.bindings)?;
    push_hashed_record(&mut output, 9, spec.selection)?;
    for heap in spec.heaps {
        push_record(&mut output, 3, heap)?;
    }
    for facts in spec.facts {
        push_record(&mut output, 2, facts)?;
    }
    if let Some(unit) = spec.code {
        let mut payload = Vec::with_capacity(72 + unit.len());
        payload.extend_from_slice(&super::support::wyhash(0, unit).to_le_bytes());
        payload.extend_from_slice(&authority_spelling);
        payload.extend_from_slice(unit);
        push_record(&mut output, 4, &payload)?;
    }
    debug_assert_eq!(output.len(), total);
    structural_preflight(&output, spec.architecture, spec.runtime_abi)?;
    Ok(output)
}

pub fn structural_preflight(
    bytes: &[u8],
    architecture: u32,
    runtime_abi: u64,
) -> Result<Image, Error> {
    if bytes.len() < 28
        || bytes.len() > 1 << 30
        || bytes.get(..8) != Some(b"BIRNAMIM")
        || read_u32(bytes, 8) != Some(FORMAT_VERSION)
        || read_u32(bytes, 12) != Some(0x0102_0304)
        || read_u32(bytes, 16) != Some(architecture)
        || read_u64(bytes, 20) != Some(runtime_abi)
    {
        return Err(Error::Header);
    }
    let mut offset = 28usize;
    let authority_spelling = next_record(bytes, &mut offset, 5)?;
    if authority_spelling.len() != 64
        || authority_spelling
            .iter()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(Error::Authority);
    }
    let (programs_hash, programs) = hashed_record(next_record(bytes, &mut offset, 7)?)?;
    let (_, bindings) = hashed_record(next_record(bytes, &mut offset, 8)?)?;
    let (selection_hash, selection) = hashed_record(next_record(bytes, &mut offset, 9)?)?;
    let program_inventory =
        portable_inventory::inspect(programs).map_err(|_| Error::CanonicalValue)?;
    let binding_inventory =
        code_binding_inventory::inspect(bindings).map_err(|_| Error::CanonicalValue)?;
    materialization_selection::inspect(selection, programs, bindings)
        .map_err(|_| Error::CanonicalValue)?;
    let expected_authority = authority_hex(authority(
        programs_hash,
        binding_inventory.semantic_digest,
        selection_hash,
    ));
    if authority_spelling != expected_authority {
        return Err(Error::Authority);
    }

    let mut heaps = Vec::new();
    let mut facts = Vec::new();
    let mut code = None;
    while offset < bytes.len() {
        let kind = read_u32(bytes, offset).ok_or(Error::Record)?;
        match kind {
            3 if facts.is_empty() && code.is_none() => {
                heaps.push(next_record(bytes, &mut offset, 3)?);
            }
            2 if code.is_none() => facts.push(next_record(bytes, &mut offset, 2)?),
            4 if code.is_none() => {
                code = Some(next_record(bytes, &mut offset, 4)?);
                if offset != bytes.len() {
                    return Err(Error::Record);
                }
            }
            _ => return Err(Error::Record),
        }
    }
    let (heap_objects, code_objects) =
        validate_heaps(&heaps, &program_inventory, programs, &binding_inventory)?;
    for payload in facts {
        validate_facts(payload, heap_objects, &code_objects)?;
    }
    if let Some(payload) = code {
        if payload.len() < 73
            || payload.get(8..72) != Some(authority_spelling)
            || super::support::wyhash(0, &payload[72..])
                != read_u64(payload, 0).ok_or(Error::Code)?
        {
            return Err(Error::Code);
        }
    }
    Ok(Image {
        architecture,
        runtime_abi,
        heap_objects,
        has_code: code.is_some(),
    })
}

fn next_record<'a>(bytes: &'a [u8], offset: &mut usize, expected: u32) -> Result<&'a [u8], Error> {
    let header_end = offset.checked_add(12).ok_or(Error::Quantity)?;
    if read_u32(bytes, *offset) != Some(expected) {
        return Err(Error::Record);
    }
    let length = usize::try_from(read_u64(bytes, *offset + 4).ok_or(Error::Record)?)
        .map_err(|_| Error::Quantity)?;
    let end = header_end.checked_add(length).ok_or(Error::Quantity)?;
    let payload = bytes.get(header_end..end).ok_or(Error::Record)?;
    *offset = end;
    Ok(payload)
}

fn hashed_record(payload: &[u8]) -> Result<([u8; 32], &[u8]), Error> {
    if payload.len() < 40 {
        return Err(Error::Record);
    }
    let digest: [u8; 32] = payload[..32].try_into().map_err(|_| Error::Record)?;
    let length = usize::try_from(read_u64(payload, 32).ok_or(Error::Record)?)
        .map_err(|_| Error::Quantity)?;
    let value = payload
        .get(40..)
        .filter(|value| value.len() == length)
        .ok_or(Error::Record)?;
    if <[u8; 32]>::from(Sha256::digest(value)) != digest {
        return Err(Error::Record);
    }
    Ok((digest, value))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Field<'a> {
    pub tag: u8,
    pub index: Option<u32>,
    pub data: Option<&'a [u8]>,
}

struct HeapObject<'a> {
    locator: Field<'a>,
    word_count: u32,
    encoded_slots: &'a [u8],
}

fn validate_heaps(
    payloads: &[&[u8]],
    inventory: &portable_inventory::Inventory<'_>,
    inventory_bytes: &[u8],
    bindings: &code_binding_inventory::Inventory<'_>,
) -> Result<(u32, Vec<bool>), Error> {
    let mut objects = Vec::new();
    for payload in payloads {
        let count = read_u32(payload, 0).ok_or(Error::Heap)? as usize;
        if objects
            .len()
            .checked_add(count)
            .is_none_or(|total| total > 1_048_576)
        {
            return Err(Error::Quantity);
        }
        let mut offset = 4usize;
        for _ in 0..count {
            let locator = parse_field(payload, &mut offset)?;
            if !matches!(locator.tag, 9 | 12..=16) {
                return Err(Error::Heap);
            }
            let word_count = read_u32(payload, offset).ok_or(Error::Heap)?;
            offset = offset.checked_add(4).ok_or(Error::Quantity)?;
            if matches!(locator.tag, 13..=16) && word_count != 0 {
                return Err(Error::Heap);
            }
            if word_count as usize > payload.len().saturating_sub(offset) {
                return Err(Error::Heap);
            }
            let slots_start = offset;
            for _ in 0..word_count {
                parse_field(payload, &mut offset)?;
            }
            objects.push(HeapObject {
                locator,
                word_count,
                encoded_slots: payload.get(slots_start..offset).ok_or(Error::Heap)?,
            });
        }
        if offset != payload.len() {
            return Err(Error::Heap);
        }
    }
    let heap_count = u32::try_from(objects.len()).map_err(|_| Error::Quantity)?;
    let mut code_program_indices = vec![None; objects.len()];
    let mut roots = Vec::new();
    let mut code_object_count = 0usize;
    let mut portable_block_count = 0usize;
    for (object_index, object) in objects.iter().enumerate() {
        let is_code_object =
            object.locator.tag == 9 && object.locator.data == Some(b"CodeObjectVt");
        if is_code_object {
            code_object_count = code_object_count.checked_add(1).ok_or(Error::Quantity)?;
            if code_object_count > 1_048_576 {
                return Err(Error::Quantity);
            }
            if object.word_count != 2 {
                return Err(Error::Heap);
            }
            let mut slot_offset = 0usize;
            let program_field = parse_field(object.encoded_slots, &mut slot_offset)?;
            let provenance = parse_field(object.encoded_slots, &mut slot_offset)?;
            if slot_offset != object.encoded_slots.len()
                || program_field.tag != 11
                || !matches!(provenance.tag, 0 | 7 | 9)
            {
                return Err(Error::Heap);
            }
            let program_index = program_field.index.ok_or(Error::Heap)?;
            let program = inventory
                .program(program_index as usize)
                .ok_or(Error::Heap)?;
            code_program_indices[object_index] = Some(program_index);
            roots.push(program);
        } else {
            visit_slots(object, |field| {
                if matches!(field.tag, 4 | 6 | 11 | 13..=16) {
                    return Err(Error::Heap);
                }
                Ok(())
            })?;
        }
        if object.locator.tag == 12 {
            portable_block_count = portable_block_count.checked_add(1).ok_or(Error::Quantity)?;
            if portable_block_count > 1_048_576 {
                return Err(Error::Quantity);
            }
        }
        validate_field_references(object.locator, heap_count)?;
        visit_slots(object, |field| validate_field_references(field, heap_count))?;
    }
    portable_inventory::inspect_exact_closure(inventory_bytes, &roots).map_err(|_| Error::Heap)?;
    bindings
        .validate_heap_references(heap_count)
        .map_err(|_| Error::Heap)?;
    for object in &objects {
        validate_block_vtable_target(object.locator, &code_program_indices)?;
        visit_slots(object, |field| {
            validate_block_vtable_target(field, &code_program_indices)
        })?;
    }
    for object in &objects {
        if object.locator.tag != 12 {
            continue;
        }
        let code_object = object.locator.index.ok_or(Error::Heap)? as usize;
        let program_index = *code_program_indices
            .get(code_object)
            .and_then(Option::as_ref)
            .ok_or(Error::Heap)?;
        let program = portable_program::inspect(
            inventory
                .program(program_index as usize)
                .ok_or(Error::Heap)?,
        )
        .map_err(|_| Error::Heap)?;
        let mut slot_offset = 0usize;
        let first = parse_field(object.encoded_slots, &mut slot_offset)?;
        if object.word_count != program.capture_count + 1
            || first.tag != 7
            || first.index != Some(code_object as u32)
        {
            return Err(Error::Heap);
        }
    }
    Ok((
        heap_count,
        code_program_indices
            .into_iter()
            .map(|program| program.is_some())
            .collect(),
    ))
}

fn visit_slots(
    object: &HeapObject<'_>,
    mut visit: impl FnMut(Field<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut offset = 0usize;
    for _ in 0..object.word_count {
        visit(parse_field(object.encoded_slots, &mut offset)?)?;
    }
    if offset != object.encoded_slots.len() {
        return Err(Error::Heap);
    }
    Ok(())
}

fn validate_block_vtable_target(
    field: Field<'_>,
    code_program_indices: &[Option<u32>],
) -> Result<(), Error> {
    if field.tag == 12
        && field.index.is_none_or(|index| {
            code_program_indices
                .get(index as usize)
                .is_none_or(Option::is_none)
        })
    {
        return Err(Error::Heap);
    }
    Ok(())
}

fn validate_field_references(field: Field<'_>, heap_count: u32) -> Result<(), Error> {
    if matches!(field.tag, 7 | 12) && field.index.is_none_or(|index| index >= heap_count) {
        return Err(Error::Heap);
    }
    Ok(())
}

pub(crate) fn parse_field<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<Field<'a>, Error> {
    let tag = *bytes.get(*offset).ok_or(Error::Heap)?;
    *offset = offset.checked_add(1).ok_or(Error::Quantity)?;
    match tag {
        0..=2 | 8 => Ok(Field {
            tag,
            index: None,
            data: None,
        }),
        3 | 13..=15 => {
            *offset = offset
                .checked_add(8)
                .filter(|end| *end <= bytes.len())
                .ok_or(Error::Heap)?;
            Ok(Field {
                tag,
                index: None,
                data: None,
            })
        }
        7 | 11 | 12 => {
            let index = read_u32(bytes, *offset).ok_or(Error::Heap)?;
            *offset = offset.checked_add(4).ok_or(Error::Quantity)?;
            Ok(Field {
                tag,
                index: Some(index),
                data: None,
            })
        }
        5 | 9 | 16 => {
            let length = read_u32(bytes, *offset).ok_or(Error::Heap)? as usize;
            *offset = offset.checked_add(4).ok_or(Error::Quantity)?;
            let end = offset.checked_add(length).ok_or(Error::Quantity)?;
            let data = bytes.get(*offset..end).ok_or(Error::Heap)?;
            if matches!(tag, 5 | 9) && (data.is_empty() || data.contains(&0)) {
                return Err(Error::Heap);
            }
            *offset = end;
            Ok(Field {
                tag,
                index: None,
                data: Some(data),
            })
        }
        _ => Err(Error::Heap),
    }
}

fn validate_facts(payload: &[u8], heap_count: u32, code_objects: &[bool]) -> Result<(), Error> {
    if payload.len() < 36 {
        return Err(Error::Facts);
    }
    let basis = read_u64(payload, 0).ok_or(Error::Facts)?;
    let floor = read_u64(payload, 8).ok_or(Error::Facts)?;
    let policy = payload[16];
    let argument = read_u64(payload, 24).ok_or(Error::Facts)?;
    let count = read_u32(payload, 32).ok_or(Error::Facts)?;
    if floor > basis
        || payload[17..24].iter().any(|byte| *byte != 0)
        || !matches!(policy, 0..=2)
        || (policy == 0 && argument != 0)
        || (policy == 1 && argument > basis)
    {
        return Err(Error::Facts);
    }
    let mut offset = 36usize;
    for _ in 0..count {
        let prefix = payload.get(offset..offset + 10).ok_or(Error::Facts)?;
        if !matches!(prefix[0], 0 | 1) || !matches!(prefix[1], 0 | 1) {
            return Err(Error::Facts);
        }
        offset += 10;
        for _ in 0..3 {
            let field = parse_field(payload, &mut offset).map_err(|_| Error::Facts)?;
            if !matches!(field.tag, 0 | 1 | 2 | 5 | 7 | 8 | 9 | 12)
                || (matches!(field.tag, 7 | 12)
                    && field.index.is_none_or(|index| index >= heap_count))
                || (field.tag == 12
                    && field
                        .index
                        .is_none_or(|index| code_objects.get(index as usize) != Some(&true)))
            {
                return Err(Error::Facts);
            }
        }
    }
    if offset != payload.len() {
        return Err(Error::Facts);
    }
    Ok(())
}

fn authority(
    programs_hash: [u8; 32],
    binding_semantics: [u8; 32],
    selection_hash: [u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"birnam.portable-image-authority.v1\0");
    hash.update(programs_hash);
    hash.update(binding_semantics);
    hash.update(selection_hash);
    hash.finalize().into()
}

fn authority_hex(raw: [u8; 32]) -> [u8; 64] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = [0u8; 64];
    for (index, byte) in raw.into_iter().enumerate() {
        output[index * 2] = HEX[(byte >> 4) as usize];
        output[index * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }
    output
}

fn push_hashed_record(output: &mut Vec<u8>, kind: u32, value: &[u8]) -> Result<(), Error> {
    let mut payload = Vec::with_capacity(40 + value.len());
    payload.extend_from_slice(&Sha256::digest(value));
    payload.extend_from_slice(
        &u64::try_from(value.len())
            .map_err(|_| Error::Quantity)?
            .to_le_bytes(),
    );
    payload.extend_from_slice(value);
    push_record(output, kind, &payload)
}

fn push_record(output: &mut Vec<u8>, kind: u32, payload: &[u8]) -> Result<(), Error> {
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(
        &u64::try_from(payload.len())
            .map_err(|_| Error::Quantity)?
            .to_le_bytes(),
    );
    output.extend_from_slice(payload);
    Ok(())
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
    fn empty_stripped_image_has_frozen_v8_bytes() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-empty.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-empty.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-empty.bin"
        ));
        let bytes =
            super::compose_stripped(1, 0xcdb6_41f9_7854_0d45, programs, bindings, selection)
                .unwrap();
        assert_eq!(bytes.len(), 740);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x14, 0x61, 0x2b, 0xb3, 0x9c, 0xf1, 0x97, 0xbe, 0x3f, 0x30, 0x49, 0x96, 0x43, 0x1b,
                0x85, 0x53, 0x49, 0x27, 0xe1, 0x99, 0x34, 0xf6, 0xbd, 0x79, 0xde, 0x22, 0x4e, 0x2a,
                0xa5, 0x7f, 0x60, 0x97,
            ]
        );
        let image = super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45).unwrap();
        assert_eq!(image.heap_objects, 0);
        assert!(!image.has_code);
    }

    #[test]
    fn mixed_stripped_image_reifies_codeobjects_and_portable_blocks() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-mixed.bin"
        ));
        let heap = mixed_heap();
        let heaps = [heap.as_slice()];
        let bytes = super::compose(super::ImageSpec {
            architecture: 1,
            runtime_abi: 0xcdb6_41f9_7854_0d45,
            programs,
            bindings,
            selection,
            heaps: &heaps,
            facts: &[],
            code: None,
        })
        .unwrap();
        assert_eq!(bytes.len(), 2617);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x57, 0xc2, 0x3b, 0x8e, 0x2f, 0x84, 0x8d, 0x52, 0x40, 0x3d, 0x2d, 0x9f, 0xe0, 0xfd,
                0xf2, 0xbf, 0x2f, 0xae, 0x75, 0x12, 0x76, 0xd4, 0xdb, 0xf4, 0x77, 0xb7, 0x1c, 0xcd,
                0xba, 0xad, 0xb6, 0xac,
            ]
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-stripped.bin"
            ))
        );
        let image = super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45).unwrap();
        assert_eq!(image.heap_objects, 12);
        assert!(!image.has_code);
    }

    #[test]
    fn mapped_code_is_final_and_bound_to_portable_authority() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-mixed.bin"
        ));
        let heap = mixed_heap();
        let heaps = [heap.as_slice()];
        let bytes = super::compose(super::ImageSpec {
            architecture: 1,
            runtime_abi: 0xcdb6_41f9_7854_0d45,
            programs,
            bindings,
            selection,
            heaps: &heaps,
            facts: &[],
            code: Some(b"\x7fELFbirnam-v8-fixture"),
        })
        .unwrap();
        assert_eq!(bytes.len(), 2722);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xff, 0x1a, 0x05, 0xc7, 0x5a, 0xb7, 0x23, 0x2c, 0x8e, 0x5d, 0xe1, 0xb7, 0x41, 0xff,
                0x69, 0xfc, 0x1b, 0x81, 0x40, 0x35, 0x22, 0x41, 0x7e, 0x15, 0xb9, 0x1e, 0xf2, 0x4d,
                0x27, 0x4e, 0x21, 0x3e,
            ]
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-code.bin"
            ))
        );
        assert!(
            super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45)
                .unwrap()
                .has_code
        );
    }

    #[test]
    fn retained_facts_admit_portable_block_vtable_references() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-mixed.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-mixed.bin"
        ));
        let heap = mixed_heap();
        let heaps = [heap.as_slice()];
        let mut facts = Vec::new();
        facts.extend_from_slice(&10u64.to_le_bytes());
        facts.extend_from_slice(&5u64.to_le_bytes());
        facts.push(2);
        facts.extend_from_slice(&[0; 7]);
        facts.extend_from_slice(&3u64.to_le_bytes());
        facts.extend_from_slice(&1u32.to_le_bytes());
        facts.extend_from_slice(&[1, 0]);
        facts.extend_from_slice(&9u64.to_le_bytes());
        push_indexed(&mut facts, 7, 1);
        push_named(&mut facts, 5, b"seen");
        push_indexed(&mut facts, 12, 0);
        let fact_sets = [facts.as_slice()];
        let bytes = super::compose(super::ImageSpec {
            architecture: 1,
            runtime_abi: 0xcdb6_41f9_7854_0d45,
            programs,
            bindings,
            selection,
            heaps: &heaps,
            facts: &fact_sets,
            code: None,
        })
        .unwrap();
        assert_eq!(bytes.len(), 2694);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x65, 0x2f, 0xc3, 0xd5, 0x9d, 0xb0, 0x71, 0x3f, 0x11, 0x13, 0x6d, 0xd2, 0x2c, 0x34,
                0xe9, 0xbe, 0x52, 0x2a, 0xc1, 0x90, 0xa0, 0xe9, 0x03, 0x1f, 0x93, 0x4d, 0x19, 0x60,
                0xcc, 0x6b, 0x12, 0xa5,
            ]
        );
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-facts-stripped.bin"
            ))
        );
        assert_eq!(
            super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45)
                .unwrap()
                .heap_objects,
            12
        );
    }

    #[test]
    fn shared_codeobjects_and_cyclic_block_captures_match_independent_bytes() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-shared-cyclic.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-empty.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-shared-cyclic.bin"
        ));
        let heap = shared_cyclic_heap();
        let heaps = [heap.as_slice()];
        let bytes = super::compose(super::ImageSpec {
            architecture: 1,
            runtime_abi: 0xcdb6_41f9_7854_0d45,
            programs,
            bindings,
            selection,
            heaps: &heaps,
            facts: &[],
            code: None,
        })
        .unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-shared-cyclic-stripped.bin"
            ))
        );
        assert_eq!(
            super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45)
                .unwrap()
                .heap_objects,
            8
        );
    }

    #[test]
    fn codebinding_can_reference_a_portable_block_implementation() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-nested-closure.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-block-implementation.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-block-implementation.bin"
        ));
        let heap = mixed_heap();
        let heaps = [heap.as_slice()];
        let bytes = super::compose(super::ImageSpec {
            architecture: 1,
            runtime_abi: 0xcdb6_41f9_7854_0d45,
            programs,
            bindings,
            selection,
            heaps: &heaps,
            facts: &[],
            code: None,
        })
        .unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-block-binding-stripped.bin"
            ))
        );
    }

    #[test]
    fn every_non_image_prefix_rejects_and_stored_image_mutations_do_not_panic() {
        let fixtures: &[&[u8]] = &[
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-block-binding-stripped.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-empty-stripped.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-stripped.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-code.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-mixed-facts-stripped.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "image-shared-cyclic-stripped.bin"
            )),
        ];
        for fixture in fixtures {
            for end in 0..fixture.len() {
                let prefix = &fixture[..end];
                if super::structural_preflight(prefix, 1, 0xcdb6_41f9_7854_0d45).is_ok() {
                    assert!(
                        fixtures.contains(&prefix),
                        "accepted non-image prefix {end} of {}",
                        fixture.len()
                    );
                }
            }
            for offset in 0..fixture.len() {
                let mut mutated = fixture.to_vec();
                mutated[offset] ^= 1;
                assert!(
                    std::panic::catch_unwind(|| {
                        let _ = super::structural_preflight(&mutated, 1, 0xcdb6_41f9_7854_0d45);
                    })
                    .is_ok()
                );
            }
        }
    }

    #[test]
    fn obsolete_image_versions_reject_at_the_fixed_header() {
        let fixture = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "image-empty-stripped.bin"
        ));
        assert!(super::decode(fixture, 1, 0xcdb6_41f9_7854_0d45).is_ok());
        for version in [4u32, 5, 6] {
            let mut obsolete = fixture.to_vec();
            obsolete[8..12].copy_from_slice(&version.to_le_bytes());
            assert_eq!(
                super::decode(&obsolete, 1, 0xcdb6_41f9_7854_0d45).map(|_| ()),
                Err(super::Error::Header)
            );
        }
    }

    #[test]
    fn bounded_generated_code_records_round_trip_structurally() {
        let programs = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-empty.bin"
        ));
        let bindings = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "bindings-empty.bin"
        ));
        let selection = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "selection-empty.bin"
        ));
        for length in 1..=256usize {
            let unit: Vec<_> = (0..length)
                .map(|index| (index as u8).wrapping_mul(31).wrapping_add(length as u8))
                .collect();
            let bytes = super::compose(super::ImageSpec {
                architecture: 1,
                runtime_abi: 0xcdb6_41f9_7854_0d45,
                programs,
                bindings,
                selection,
                heaps: &[],
                facts: &[],
                code: Some(&unit),
            })
            .unwrap();
            assert!(
                super::structural_preflight(&bytes, 1, 0xcdb6_41f9_7854_0d45)
                    .unwrap()
                    .has_code
            );
            let offset = 28 + (length * 97 % (bytes.len() - 28));
            let mut mutated = bytes;
            mutated[offset] ^= 1;
            assert!(
                std::panic::catch_unwind(|| {
                    let _ = super::structural_preflight(&mutated, 1, 0xcdb6_41f9_7854_0d45);
                })
                .is_ok()
            );
        }
    }

    fn mixed_heap() -> Vec<u8> {
        let mut heap = 12u32.to_le_bytes().to_vec();
        push_named(&mut heap, 9, b"CodeObjectVt");
        heap.extend_from_slice(&2u32.to_le_bytes());
        push_indexed(&mut heap, 11, 1);
        heap.push(0);
        push_indexed(&mut heap, 12, 0);
        heap.extend_from_slice(&1u32.to_le_bytes());
        push_indexed(&mut heap, 7, 0);
        for _ in 0..6 {
            push_named(&mut heap, 9, b"ObjectVt");
            heap.extend_from_slice(&0u32.to_le_bytes());
        }
        heap.push(13);
        heap.extend_from_slice(&(-1i64).to_le_bytes());
        heap.extend_from_slice(&0u32.to_le_bytes());
        heap.push(14);
        heap.extend_from_slice(&0x7ff8_0000_0000_0042u64.to_le_bytes());
        heap.extend_from_slice(&0u32.to_le_bytes());
        heap.push(15);
        heap.extend_from_slice(&0xfedc_ba98_7654_3210u64.to_le_bytes());
        heap.extend_from_slice(&0u32.to_le_bytes());
        push_named(&mut heap, 16, b"source");
        heap.extend_from_slice(&0u32.to_le_bytes());
        heap
    }

    fn shared_cyclic_heap() -> Vec<u8> {
        let programs = birnam_artifact::portable_inventory::inspect(include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "inventory-shared-cyclic.bin"
        )))
        .unwrap();
        let nested = include_bytes!(concat!(
            "../../../abi/fixtures/portable-code-v1/",
            "program-nonutf8-names-block.bin"
        ));
        let nested = birnam_artifact::portable_program::inspect(nested)
            .unwrap()
            .node(0)
            .unwrap()
            .data;
        let nested_index = (0..programs.program_count as usize)
            .find(|index| programs.program(*index) == Some(nested))
            .unwrap() as u32;
        let outer_index = (0..programs.program_count as usize)
            .find(|index| {
                programs.program(*index)
                    == Some(
                        include_bytes!(concat!(
                            "../../../abi/fixtures/portable-code-v1/",
                            "program-nonutf8-names-block.bin"
                        ))
                        .as_slice(),
                    )
            })
            .unwrap() as u32;

        let mut heap = 8u32.to_le_bytes().to_vec();
        for _ in 0..2 {
            push_named(&mut heap, 9, b"CodeObjectVt");
            heap.extend_from_slice(&2u32.to_le_bytes());
            push_indexed(&mut heap, 11, nested_index);
            heap.push(0);
        }
        push_named(&mut heap, 9, b"CodeObjectVt");
        heap.extend_from_slice(&2u32.to_le_bytes());
        push_indexed(&mut heap, 11, outer_index);
        heap.push(0);
        for (code_object, capture) in [(0, 4), (0, 3), (1, 5), (2, 6)] {
            push_indexed(&mut heap, 12, code_object);
            heap.extend_from_slice(&2u32.to_le_bytes());
            push_indexed(&mut heap, 7, code_object);
            push_indexed(&mut heap, 7, capture);
        }
        push_named(&mut heap, 9, b"ObjectVt");
        heap.extend_from_slice(&0u32.to_le_bytes());
        heap
    }

    fn push_named(output: &mut Vec<u8>, tag: u8, name: &[u8]) {
        output.push(tag);
        output.extend_from_slice(&(name.len() as u32).to_le_bytes());
        output.extend_from_slice(name);
    }

    fn push_indexed(output: &mut Vec<u8>, tag: u8, index: u32) {
        output.push(tag);
        output.extend_from_slice(&index.to_le_bytes());
    }
}
