//! Unconnected Generation projection, retained for migration.
use super::*;
unsafe fn initial_projection(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    use birnam_codegen::{artifact_stackless_selection::Disposition, generation as g};
    unsafe {
        let snapshot = publication::snapshot(s, q)?;
        for &(name, source) in &snapshot {
            q.keep(name)?;
            q.keep(source)?;
        }
        let mut entries = Vec::new();
        for &(name, source) in &snapshot {
            let Some((bytes, metadata)) = s.revision(q, source)? else {
                return s.error(
                    q,
                    b"compiled source metadata is missing; rebuild the artifact or image",
                );
            };
            if s.symbol_bytes(q, name)?.as_deref() != Some(metadata.name.as_slice()) {
                return s.error(q, b"generation source name mismatches");
            }
            let revision = reflect_revision(s, q, source, &metadata)?;
            let module = g::Module::new(bytes, metadata).ok_or_else(invalid)?;
            let row = s.vector(q, &[name, revision, s.named(b"nil")?])?;
            entries.push((module, row));
        }
        entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        let rows = entries.iter().map(|(_, row)| *row).collect::<Vec<_>>();
        let modules = entries.into_iter().map(|(m, _)| m).collect::<Vec<_>>();
        // The production source backend currently emits resumable implementations
        // throughout. Never label these rows with the legacy synchronous default.
        let selection =
            g::generation_selection_with_overrides(&modules, &[], Disposition::ResumableRequired)
                .ok_or_else(invalid)?;
        let descriptor_hashes = backend::hashes(
            s,
            q,
            modules
                .iter()
                .flat_map(|m| m.metadata.operations.iter().map(|o| o.selector.as_slice())),
        )?;
        let authority =
            g::dispatch_generation_authority_with(&modules, &selection, |module, index, _| {
                descriptor_hashes
                    .get(&module.metadata.operations.get(index)?.selector)
                    .copied()
                    .flatten()
            })
            .ok_or_else(invalid)?;
        let source_map = s.vector(q, &rows)?;
        let digest = s.text(q, &g::raw_digest_spelling(authority))?;
        let selection = s.text(q, &selection)?;
        let tag = q.keep(s.id.intern_name(b"generationProjection")?)?;
        let result = s.vector(q, &[tag, source_map, digest, selection])?;
        // Callbacks may register another module or swap image sources while we
        // build reflected values. Reject a mixed snapshot, never seal it.
        if publication::snapshot(s, q)? != snapshot {
            return s.error(q, b"generation sources changed during projection");
        }
        Ok(Operand::object(result))
    }
}
