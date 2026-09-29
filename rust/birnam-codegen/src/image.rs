//! Private relocatable-object bundle bound to the existing portable inventories.
use birnam_artifact::{code_binding_inventory, materialization_selection, portable_inventory};
use sha2::{Digest, Sha256};
pub struct Plan {
    pub programs: usize,
    pub bindings: usize,
    pub sources: Vec<Vec<u8>>,
    authority: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapped_plan_checks_inventory_authority_without_emitting_source() {
        let programs =
            include_bytes!("../../../abi/fixtures/portable-code-v1/inventory-nested-closure.bin");
        let bindings = include_bytes!("../../../abi/fixtures/portable-code-v1/bindings-mixed.bin");
        let selection =
            include_bytes!("../../../abi/fixtures/portable-code-v1/selection-mixed.bin");
        let plan = Plan::new(programs, bindings, selection, false).unwrap();
        assert!(plan.sources.iter().all(Vec::is_empty));
        assert_eq!(plan.sources.len(), 2 * plan.programs + plan.bindings);
        let objects = vec![b"trusted object".to_vec(); plan.sources.len()];
        let encoded = plan.encode(&objects).unwrap();
        assert_eq!(plan.decode(&encoded), Some(objects));
        for n in 0..encoded.len() {
            assert!(plan.decode(&encoded[..n]).is_none());
        }
        let mut corrupt = encoded.clone();
        corrupt[8] ^= 1;
        assert!(plan.decode(&corrupt).is_none());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(plan.decode(&trailing).is_none());
    }
}
impl Plan {
    pub fn new(programs: &[u8], bindings: &[u8], selection: &[u8], emit: bool) -> Option<Self> {
        let p = portable_inventory::inspect(programs).ok()?;
        let b = code_binding_inventory::inspect(bindings).ok()?;
        materialization_selection::inspect(selection, programs, bindings).ok()?;
        let mut hash = Sha256::new();
        for bytes in [programs, bindings, selection] {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        let mut sources = Vec::new();
        if emit {
            for i in 0..p.program_count as usize {
                let program = p.program(i)?;
                sources.push(crate::native::compile_portable(program, &[]).ok()?);
                sources.push(crate::native::compile_portable_synchronous(program).ok()?);
            }
            for i in 0..b.binding_count as usize {
                sources.push(crate::native::compile_named(b.binding(i)?.arity).ok()?);
            }
        } else {
            sources.resize(
                2 * p.program_count as usize + b.binding_count as usize,
                Vec::new(),
            );
        }
        if sources
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()))?
            > 128 << 20
        {
            return None;
        }
        Some(Self {
            programs: p.program_count as usize,
            bindings: b.binding_count as usize,
            sources,
            authority: hash.finalize().into(),
        })
    }
    pub fn encode(&self, objects: &[Vec<u8>]) -> Option<Vec<u8>> {
        if objects.len() != self.sources.len() {
            return None;
        }
        let mut out = b"BIRNAMO1".to_vec();
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
    pub fn decode(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        if bytes.get(..8)? != b"BIRNAMO1" || bytes.get(8..40)? != self.authority {
            return None;
        }
        let count = u32::from_le_bytes(bytes.get(40..44)?.try_into().ok()?) as usize;
        if count != self.sources.len() {
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
            objects.push(bytes.get(offset..end)?.to_vec());
            offset = end;
        }
        (offset == bytes.len()).then_some(objects)
    }
}
