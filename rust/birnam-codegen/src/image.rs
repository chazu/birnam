//! Private relocatable-object bundle bound to the existing portable inventories.
use birnam_artifact::{code_binding_inventory, materialization_selection, portable_inventory};
use sha2::{Digest, Sha256};
pub struct Plan {
    pub programs: usize,
    pub bindings: usize,
    pub requests: Vec<crate::backend_request::Request>,
    authority: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapped_plan_checks_inventory_authority_without_emitting_source() {
        let programs =
            include_bytes!("../../../tests/fixtures/portable-code-v1/inventory-nested-closure.bin");
        let bindings =
            include_bytes!("../../../tests/fixtures/portable-code-v1/bindings-mixed.bin");
        let selection =
            include_bytes!("../../../tests/fixtures/portable-code-v1/selection-mixed.bin");
        let plan = Plan::new(programs, bindings, selection, false).unwrap();
        // Constructing this plan now produces canonical requests, never C.
        assert_eq!(plan.requests.len(), 2 * plan.programs + plan.bindings);
        let objects = vec![b"trusted object".to_vec(); 2 * plan.programs];
        let encoded = plan.encode(&objects).unwrap();
        assert_eq!(
            plan.decode(&encoded),
            Some(objects.iter().map(Vec::as_slice).collect())
        );
        // The runtime holds its owned bundle snapshot through all loader sends.
        // Decoding must not duplicate the complete native-code inventory.
        let decoded = plan.decode(&encoded).unwrap();
        let mut offset = 44;
        for object in &decoded {
            offset += 8;
            assert_eq!(object.as_ptr(), encoded[offset..].as_ptr());
            offset += object.len();
        }
        assert_eq!(offset, encoded.len());
        for n in 0..encoded.len() {
            assert!(plan.decode(&encoded[..n]).is_none());
        }
        let mut old_bundle = encoded.clone();
        old_bundle[..8].copy_from_slice(b"BIRNAMO1");
        assert!(plan.decode(&old_bundle).is_none());
        let mut corrupt = encoded.clone();
        corrupt[8] ^= 1;
        assert!(plan.decode(&corrupt).is_none());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(plan.decode(&trailing).is_none());
    }
}
impl Plan {
    pub fn new(programs: &[u8], bindings: &[u8], selection: &[u8], _emit: bool) -> Option<Self> {
        let p = portable_inventory::inspect(programs).ok()?;
        let b = code_binding_inventory::inspect(bindings).ok()?;
        let selected = materialization_selection::inspect(selection, programs, bindings).ok()?;
        let mut hash = Sha256::new();
        for bytes in [programs, bindings, selection] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        use crate::backend_request::Request;
        let mut requests = Vec::new();
        for i in 0..p.program_count as usize {
            for synchronous in [false, true] {
                requests.push(Request::Program {
                    program: p.program(i)?.to_vec(),
                    source: vec![],
                    filename: b"<compiler>".to_vec(),
                    persistent: false,
                    synchronous,
                });
            }
        }
        for i in 0..b.binding_count as usize {
            requests.push(Request::Named {
                arity: b.binding(i)?.arity,
                synchronous: selected.row(i)?.disposition
                    == materialization_selection::Disposition::Synchronous,
            });
        }
        Some(Self {
            programs: p.program_count as usize,
            bindings: b.binding_count as usize,
            requests,
            authority: hash.finalize().into(),
        })
    }
    pub fn encode(&self, objects: &[Vec<u8>]) -> Option<Vec<u8>> {
        if objects.len() != 2 * self.programs {
            return None;
        }
        let mut out = b"BIRNAMO2".to_vec();
        out.extend_from_slice(&self.authority);
        out.extend_from_slice(&(objects.len() as u32).to_le_bytes());
        for bytes in objects {
            if bytes.is_empty() {
                return None;
            }
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
            if out.len() > 1 << 30 {
                return None;
            }
        }
        Some(out)
    }
    pub fn decode<'a>(&self, bytes: &'a [u8]) -> Option<Vec<&'a [u8]>> {
        if bytes.get(..8)? != b"BIRNAMO2" || bytes.get(8..40)? != self.authority {
            return None;
        }
        let count = u32::from_le_bytes(bytes.get(40..44)?.try_into().ok()?) as usize;
        if count != 2 * self.programs {
            return None;
        }
        let mut offset = 44usize;
        let mut objects = Vec::new();
        for _ in 0..count {
            let length = usize::try_from(u64::from_le_bytes(
                bytes.get(offset..offset + 8)?.try_into().ok()?,
            ))
            .ok()?;
            offset += 8;
            let end = offset.checked_add(length)?;
            if length == 0 {
                return None;
            }
            objects.push(bytes.get(offset..end)?);
            offset = end;
        }
        (offset == bytes.len()).then_some(objects)
    }
}
