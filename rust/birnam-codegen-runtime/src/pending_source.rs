//! Unconnected source evaluation/embedding and Generation projection; retained
//! for migration. Source record operations now live in source.rs.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Op {
    Eval,
    EvalFrom,
}
impl State {
    pub(super) unsafe fn syntax_error(
        &self,
        scope: &Scope<'_>,
        source: Oop,
    ) -> Result<(), Outcome> {
        unsafe {
            let lang = scope.keep(self.named(b"Lang")?)?;
            let offset = scope.keep(object(self.send(lang, b"furthestFail", &[])?)?)?;
            let integer = scope.keep(self.named(b"Integer")?)?;
            let offset = self.send(integer, b"wordOf:", &[Operand::object(offset)])?;
            let Some(offset) = offset.as_word() else {
                self.sources.note(b"RustCodegen syntax error");
                return Ok(());
            };
            let bytes = self.source_bytes(scope, source)?.unwrap_or_default();
            let offset = (offset as i64).clamp(0, bytes.len() as i64) as usize;
            let start = bytes[..offset]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            let end = bytes[offset..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |i| offset + i);
            let line = bytes[..offset].iter().filter(|&&b| b == b'\n').count() + 1;
            let mut message = format!("{line}:{}: syntax error\n", offset - start + 1).into_bytes();
            message.extend_from_slice(&bytes[start..end]);
            message.push(b'\n');
            message.extend(
                bytes[start..offset]
                    .iter()
                    .map(|&b| if b == b'\t' { b } else { b' ' }),
            );
            message.push(b'^');
            self.sources.note(&message);
            Ok(())
        }
    }
}
pub(super) unsafe fn operation(
    s: &State,
    scope: &Scope<'_>,
    op: Op,
    a: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let nil = scope.keep(s.named(b"nil")?)?;
        let nil_value = Operand::object(nil);
        match op {
            Op::Eval | Op::EvalFrom => {
                *s.sources.error.borrow_mut() = None;
                let path = if matches!(op, Op::EvalFrom) {
                    let Some(path) = s.source_bytes(scope, object(a[1])?)? else {
                        s.sources.note(b"RustCodegen source path was not a String");
                        return Ok(nil_value);
                    };
                    path
                } else {
                    b"<compiler>".to_vec()
                };
                let source = object(a[0])?;
                let Some(bytes) = s.source_bytes(scope, source)? else {
                    s.sources.note(b"RustCodegen could not lower the source");
                    return Ok(nil_value);
                };
                let Some(tree) = s.source_tree(scope, source, bytes.len())? else {
                    s.syntax_error(scope, source)?;
                    return Ok(nil_value);
                };
                let mut program = match lower_program(&tree, &bytes) {
                    Ok(program) => program,
                    Err(error) => {
                        s.sources.note(if error == birnam_codegen::LowerError::ThreadedSpecialForm {
                            b"RustCodegen source-sensitive special forms cannot use => threading"
                        } else { b"RustCodegen could not lower the source" });
                        return Ok(nil_value);
                    }
                };
                if let Some(error) = s.embed_program(scope, &mut program, &path)? {
                    let message = scope.keep(object(s.send(error, b"message", &[])?)?)?;
                    s.sources.note(&s.bytes(message)?);
                    return Ok(nil_value);
                }
                let c = match backend::emit(
                    s,
                    scope,
                    &Request::program(program, &bytes, &path).ok_or_else(invalid)?,
                ) {
                    Ok(c) => c,
                    Err(backend::Failure::Native(e)) => return Err(e),
                    Err(backend::Failure::Rejected(_)) => {
                        s.sources.note(b"RustCodegen could not emit the source");
                        return Ok(nil_value);
                    }
                };
                let unit = s.native_unit(scope, &c)?;
                if unit == nil || s.is_error(unit)? {
                    s.sources.note(b"RustCodegen generated C failed to compile");
                    return Ok(nil_value);
                }
                let method = scope.keep(object(s.send(unit, b"initialize", &[])?)?)?;
                if method == nil || s.is_error(method)? {
                    s.sources.note(b"RustCodegen generated C failed to compile");
                    return Ok(nil_value);
                }
                let _source = s.adapter_scope(None);
                s.run_method(scope, method)
            }
        }
    }
}
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
