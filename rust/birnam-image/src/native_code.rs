//! Native acceleration stored in image-v8's checksummed Code record.
//! Older records contain only the portable unit. Version 1 bundles that unit
//! with the compiled source generation and its admission manifest.
const MAGIC: &[u8; 8] = b"BIRNAMIC";
const HEADER: usize = 36;

pub(crate) struct Units<'a> {
    pub portable: &'a [u8],
    pub generation: Option<(&'a [u8], &'a [u8])>,
}

pub(crate) fn decode(bytes: &[u8]) -> Option<Units<'_>> {
    if !bytes.starts_with(MAGIC) {
        return (!bytes.is_empty()).then_some(Units {
            portable: bytes,
            generation: None,
        });
    }
    if bytes.get(8..12)? != 1u32.to_le_bytes() {
        return None;
    }
    let mut offset = HEADER;
    let mut parts = [&[][..]; 3];
    for (index, part) in parts.iter_mut().enumerate() {
        let start = 12 + index * 8;
        let length = usize::try_from(u64::from_le_bytes(
            bytes.get(start..start + 8)?.try_into().ok()?,
        ))
        .ok()?;
        if length == 0 {
            return None;
        }
        let end = offset.checked_add(length)?;
        *part = bytes.get(offset..end)?;
        offset = end;
    }
    (offset == bytes.len()).then_some(Units {
        portable: parts[0],
        generation: Some((parts[1], parts[2])),
    })
}

pub(crate) fn encode(portable: &[u8], generation: &[u8], manifest: &[u8]) -> Option<Vec<u8>> {
    let parts = [portable, generation, manifest];
    let size = parts.iter().try_fold(HEADER, |total, part| {
        if part.is_empty() {
            None
        } else {
            total.checked_add(part.len())
        }
    })?;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1u32.to_le_bytes());
    for part in parts {
        bytes.extend_from_slice(&u64::try_from(part.len()).ok()?.to_le_bytes());
    }
    for part in parts {
        bytes.extend_from_slice(part);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_units_and_legacy_portable_unit() {
        let bytes = encode(b"portable", b"generation", b"manifest").unwrap();
        let units = decode(&bytes).unwrap();
        assert_eq!(units.portable, b"portable");
        assert_eq!(
            units.generation,
            Some((&b"generation"[..], &b"manifest"[..]))
        );
        assert!(decode(b"legacy native unit").unwrap().generation.is_none());
        for end in 8..bytes.len() {
            assert!(decode(&bytes[..end]).is_none(), "truncated at {end}");
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_none());
        for offset in [12, 20, 28] {
            for length in [0, u64::MAX] {
                let mut invalid = bytes.clone();
                invalid[offset..offset + 8].copy_from_slice(&length.to_le_bytes());
                assert!(decode(&invalid).is_none());
            }
        }
        let mut version = bytes;
        version[8] = 2;
        assert!(decode(&version).is_none());
    }
}
