//! Pointer-free private behavior ownership in the Image heap.
use super::v8::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parent {
    Export(Vec<u8>),
    Owner(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub object: u32,
    pub owner: u32,
    // Only the owner's row defines the private behavior's parent.
    pub parent: Option<Parent>,
}
pub fn encode(links: &[Link]) -> Vec<u8> {
    let mut out = (links.len() as u32).to_le_bytes().to_vec();
    for link in links {
        out.extend_from_slice(&link.object.to_le_bytes());
        out.extend_from_slice(&link.owner.to_le_bytes());
        if let Some(parent) = &link.parent {
            match parent {
                Parent::Export(name) => {
                    out.push(0);
                    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
                    out.extend_from_slice(name);
                }
                Parent::Owner(owner) => {
                    out.push(1);
                    out.extend_from_slice(&owner.to_le_bytes());
                }
            }
        }
    }
    out
}
pub fn decode(bytes: &[u8]) -> Result<Vec<Link>, Error> {
    fn word(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
        let end = offset.checked_add(4).ok_or(Error::Quantity)?;
        let word = u32::from_le_bytes(
            bytes
                .get(*offset..end)
                .ok_or(Error::Heap)?
                .try_into()
                .map_err(|_| Error::Heap)?,
        );
        *offset = end;
        Ok(word)
    }
    let mut offset = 0;
    let count = word(bytes, &mut offset)? as usize;
    if count > 1_048_576 || count > bytes.len().saturating_sub(offset) / 8 {
        return Err(Error::Heap);
    }
    let mut out = Vec::new();
    for _ in 0..count {
        let object = word(bytes, &mut offset)?;
        let owner = word(bytes, &mut offset)?;
        if out
            .last()
            .is_some_and(|prior: &Link| prior.object >= object)
        {
            return Err(Error::Heap);
        }
        let parent = if object == owner {
            let tag = *bytes.get(offset).ok_or(Error::Heap)?;
            offset += 1;
            Some(match tag {
                0 => {
                    let length = word(bytes, &mut offset)? as usize;
                    let end = offset.checked_add(length).ok_or(Error::Quantity)?;
                    let name = bytes.get(offset..end).ok_or(Error::Heap)?.to_vec();
                    offset = end;
                    Parent::Export(name)
                }
                1 => Parent::Owner(word(bytes, &mut offset)?),
                _ => return Err(Error::Heap),
            })
        } else {
            None
        };
        out.push(Link {
            object,
            owner,
            parent,
        });
    }
    if offset != bytes.len() {
        return Err(Error::Heap);
    }
    Ok(out)
}
