//! A source-owner snapshot projected into ordinary generation values. No parser
//! fallback, publication, or native Generation state is involved.
use super::*;
use birnam_codegen::{artifact_stackless_selection::Disposition, generation as g};
unsafe fn snapshot(s: &State, q: &Scope<'_>) -> Result<(Oop, Vec<(Oop, Oop)>), Oop> {
    unsafe {
        let owner = q.keep(s.source_owner(q)?);
        let rows = q.keep(s.send(owner, b"sourceRegistry", &[])?);
        let rows = source_metadata::seq(s, q, rows)?;
        let (tag, rows) = rows.split_first().ok_or_else(invalid)?;
        if s.symbol_bytes(q, *tag)?.as_deref() != Some(b"sourceRegistryV1") {
            return Err(invalid());
        }
        let mut values = Vec::new();
        for row in rows {
            let row = source_metadata::seq(s, q, *row)?;
            let [name, record] = row.as_slice() else {
                return Err(invalid());
            };
            values.push((*name, *record));
        }
        Ok((owner, values))
    }
}
pub(super) unsafe extern "C" fn initial(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[]);
        let original = snapshot(s, &q)?;
        let mut entries = Vec::new();
        for &(name, record) in &original.1 {
            let Some(metadata) = source_metadata::read(s, &q, record)?.and_then(|m| m.revision)
            else {
                return s.error(
                    b"compiled source metadata is missing; rebuild the artifact or image",
                );
            };
            let bytes = s.source_bytes(&q, record)?.ok_or_else(invalid)?;
            if s.symbol_bytes(&q, name)?.as_deref() != Some(metadata.name.as_slice()) {
                return s.error(b"generation source name mismatches");
            }
            let revision = source::reflect_revision(s, &q, record, &metadata)?;
            let module = g::Module::new(bytes, metadata).ok_or_else(invalid)?;
            let row = s.vector(&q, &[name, revision, s.named(b"nil")?])?;
            entries.push((module, row));
        }
        entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        let rows = entries.iter().map(|(_, row)| *row).collect::<Vec<_>>();
        let modules = entries.into_iter().map(|(m, _)| m).collect::<Vec<_>>();
        let selection =
            g::generation_selection_with_overrides(&modules, &[], Disposition::ResumableRequired)
                .ok_or_else(invalid)?;
        let mut descriptors = HashMap::new();
        for module in &modules {
            for operation in &module.metadata.operations {
                if descriptors.contains_key(&operation.selector) {
                    continue;
                }
                let name = q.keep(s.text(&operation.selector)?);
                let descriptor = q.keep(s.send(s.receiver, b"methodDescriptorFor:", &[name])?);
                if s.is_error(descriptor)? {
                    return Ok(descriptor);
                }
                descriptors.insert(
                    operation.selector.clone(),
                    g::sha256_raw(&s.bytes(descriptor)?),
                );
            }
        }
        let authority = g::dispatch_generation_authority_with(&modules, &selection, |m, i, _| {
            descriptors
                .get(&m.metadata.operations.get(i)?.selector)
                .copied()
        })
        .ok_or_else(invalid)?;
        let source_map = s.vector(&q, &rows)?;
        let digest = q.keep(s.text(&g::raw_digest_spelling(authority))?);
        let selection = q.keep(s.text(&selection)?);
        let tag = q.keep(s.intern(b"generationProjection")?);
        let result = s.vector(&q, &[tag, source_map, digest, selection])?;
        if snapshot(s, &q)? != original {
            return s.error(b"generation sources changed during projection");
        }
        Ok(result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
