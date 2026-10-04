//! Frozen record value helpers used by live heap projection/restoration.
//! Complete container admission and composition live in modules/image-format.birnam.
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
    pub sources: Option<&'a [u8]>,
    pub behaviors: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    pub architecture: u32,
    pub runtime_abi: u64,
    pub heap_objects: u32,
    pub has_code: bool,
}

#[allow(dead_code)]
pub struct DecodedImage<'a> {
    pub programs: &'a [u8],
    pub bindings: &'a [u8],
    pub selection: &'a [u8],
    pub heaps: Vec<&'a [u8]>,
    pub facts: Vec<&'a [u8]>,
    pub code: Option<&'a [u8]>,
    pub sources: Option<&'a [u8]>,
    pub behaviors: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    pub tag: u8,
    pub index: Option<u32>,
    pub data: Option<&'a [u8]>,
}

pub fn parse_field<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<Field<'a>, Error> {
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
        7 | 11 | 12 | 18 => {
            let index = read_u32(bytes, *offset).ok_or(Error::Heap)?;
            *offset = offset.checked_add(4).ok_or(Error::Quantity)?;
            Ok(Field {
                tag,
                index: Some(index),
                data: None,
            })
        }
        5 | 9 | 16 | 17 => {
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

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
