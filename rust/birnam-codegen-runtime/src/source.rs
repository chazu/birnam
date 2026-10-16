//! Source compilation and loader-facing evaluation. Registration belongs to Generation;
//! parsing, values, roots and execution remain current receiver sends.
use super::*;
use birnam_codegen::source_revision::RevisionMetadata;

#[derive(Default)]
pub(super) struct Sources {
    error: RefCell<Option<Vec<u8>>>,
}
#[derive(Clone, Copy)]
pub(super) enum Op {
    Revision,
    Metadata,
    EmbeddedFiles,
    Record,
    RecordCompiled,
    Find,
    Text,
    Compilation,
    RetainedMetadata,
    ReplaceCompiled,
    CompilationValue,
    Seal,
    LastError,
    MethodSources,
    InitialProjection,
    Eval,
    EvalFrom,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Seal | Self::LastError | Self::MethodSources | Self::InitialProjection => 0,
            Self::Record | Self::RecordCompiled | Self::ReplaceCompiled | Self::CompilationValue | Self::EvalFrom => 2,
            _ => 1,
        }
    }
}
impl Sources {
    pub(super) fn clear_error(&self) {
        *self.error.borrow_mut() = None;
    }
    pub(super) fn note(&self, message: &[u8]) {
        let mut error = self.error.borrow_mut();
        if error.is_none() {
            *error = Some(message.to_vec());
        }
    }
}
impl State {
    pub(super) unsafe fn exact(
        &self,
        scope: &Scope<'_>,
        value: Oop,
        family: &[u8],
    ) -> Result<bool, Outcome> {
        unsafe {
            let vt = scope.keep(self.named(family)?)?;
            Ok(self
                .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                .as_object()
                == Some(vt))
        }
    }
    pub(super) unsafe fn source_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            let text=scope.keep(object(self.send(self.named(b"Codegen")?,b"sourceTextOf:",&[Operand::object(value)])?)?)?;
            if text==self.named(b"nil")? {return Ok(None)}
            Ok(Some(self.bytes(text)?))
        }
    }
    pub(super) unsafe fn symbol_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            if !self.exact(scope, value, b"SymbolVt")? {
                return Ok(None);
            }
            let bytes = self.send(
                self.id.receiver,
                b"symbolBytesOf:",
                &[Operand::object(value)],
            )?;
            let bytes = bytes.as_bytes().ok_or_else(invalid)?;
            Ok(Some(
                borrowed_bytes(bytes.pointer, bytes.length)
                    .ok_or_else(invalid)?
                    .to_vec(),
            ))
        }
    }
    pub(super) unsafe fn source_tree(
        &self,
        _scope: &Scope<'_>,
        source: Oop,
        length: usize,
    ) -> Result<Option<OwnedCst>, Outcome> {
        unsafe {
            let local = Scope {
                s: self,
                tokens: RefCell::new(Vec::new()),
            };
            let scope = &local;
            let text=scope.keep(object(self.send(self.named(b"Codegen")?,b"sourceTextOf:",&[Operand::object(source)])?)?)?;
            let lang = scope.keep(self.named(b"Lang")?)?;
            let cst = scope.keep(object(self.send(
                lang,
                b"parse:",
                &[Operand::object(text)],
            )?)?)?;
            if !self.exact(scope, cst, b"PegCstVt")? {
                return Ok(None);
            }
            Ok(Some(self.project(scope, cst, length, 0, &mut 1_000_000)?))
        }
    }
    pub(super) unsafe fn revision(
        &self,
        scope: &Scope<'_>,
        source: Oop,
    ) -> Result<Option<(Vec<u8>, RevisionMetadata)>, Outcome> {
        unsafe {
            let Some(bytes) = self.source_bytes(scope, source)? else {
                return Ok(None);
            };
            let Some(metadata) = source_metadata::read(self, scope, source)? else {
                self.sources
                    .note(b"compiled source metadata is missing; rebuild the artifact or image");
                return Ok(None);
            };
            Ok(metadata.revision.map(|revision| (bytes, revision)))
        }
    }
    pub(super) unsafe fn text(&self, scope: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            let factory = scope.keep(self.named(b"String")?)?;
            scope.keep(object(self.send(
                factory,
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )?)?)
        }
    }
    pub(super) unsafe fn vector(&self, scope: &Scope<'_>, values: &[Oop]) -> Result<Oop, Outcome> {
        unsafe {
            let factory = scope.keep(self.named(b"Vector")?)?;
            let mut result = scope.keep(object(self.send(factory, b"new", &[])?)?)?;
            for &value in values {
                result = scope.keep(object(self.send(
                    result,
                    b"add:",
                    &[Operand::object(value)],
                )?)?)?;
            }
            Ok(result)
        }
    }
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
            Op::InitialProjection => initial_projection(s, scope),
            Op::MethodSources => {
                let entries = publication::snapshot(s, scope)?;
                for &(name, source) in &entries {
                    scope.keep(name)?;
                    scope.keep(source)?;
                }
                let mut sources = Vec::new();
                for (_, source) in entries {
                    // Documentation uses the compiler's saved inventory.
                    let Some(metadata) = source_metadata::read(s, scope, source)? else {
                        return Err(invalid());
                    };
                    for text in metadata.method_sources {
                        sources.push(s.text(scope, &text)?);
                    }
                }
                Ok(Operand::object(s.vector(scope, &sources)?))
            }

            Op::LastError => {
                let error = s.sources.error.borrow().clone();
                Ok(match error {
                    Some(bytes) => Operand::object(s.text(scope, &bytes)?),
                    None => nil_value,
                })
            }
            Op::Seal => publication::send(s, scope, b"sealInitialGeneration", &[]),
            Op::EmbeddedFiles => {
                let source = object(a[0])?;
                let mut paths = Vec::new();
                if let Some(metadata) = source_metadata::inspect(s, scope, source)? {
                    for path in metadata.embedded_files {
                        paths.push(s.text(scope, &path)?);
                    }
                } else {
                    s.sources.note(b"RustCodegen source was not a String");
                }
                Ok(Operand::object(s.vector(scope, &paths)?))
            }
            Op::Text => Ok(Operand::object(compiled_source::text(s,scope,object(a[0])?)?.unwrap_or(nil))),
            Op::Compilation => Ok(Operand::object(compiled_source::find(s,scope,object(a[0])?)?.unwrap_or(nil))),
            Op::RetainedMetadata => {
                let value=if let Some(record)=compiled_source::find(s,scope,object(a[0])?)? {
                    compiled_source::field(s,scope,record,2)?
                } else {nil};
                Ok(Operand::object(value))
            }
            Op::ReplaceCompiled => publication::send(s,scope,b"replaceCompiledSource:named:",a),
            Op::CompilationValue => {
                let Some(text)=compiled_source::text(s,scope,object(a[0])?)? else{return Ok(nil_value)};
                compiled_source::make(s,scope,text,object(a[1])?).map(Operand::object)
            }
            Op::Revision => {
                let Some(record)=source_metadata::prepare(s,scope,object(a[0])?)? else {return Ok(nil_value)};
                let Some((_,revision))=s.revision(scope,record)? else {
                    s.sources.note(b"RustCodegen source was not a String");return Ok(nil_value)
                };
                reflect_revision(s,scope,record,&revision).map(Operand::object)
            }
            Op::Metadata => {
                let source = object(a[0])?;
                let Some(metadata) = source_metadata::inspect(s, scope, source)? else {
                    // Preserve the existing explicit-inspection diagnostic for
                    // a String whose parse failed; non-Strings simply yield nil.
                    if s.source_bytes(scope, source)?.is_some() {
                        s.sources.note(b"compiled source metadata is missing; rebuild the artifact or image");
                    }
                    return Ok(nil_value);
                };
                let Some(revision) = metadata.revision else {
                    return Ok(nil_value);
                };
                let tag = scope.keep(s.id.intern_name(b"moduleMetadata")?)?;
                let name = scope.keep(s.id.intern_name(&revision.name)?)?;
                let mut requires = Vec::new();
                for name in revision.requires {
                    requires.push(scope.keep(s.id.intern_name(&name)?)?);
                }
                let requires = s.vector(scope, &requires)?;
                let source_digest = s.text(scope, &metadata.source_digest)?;
                let revision_digest = s.text(scope, &revision.revision_digest)?;
                Ok(Operand::object(s.vector(
                    scope,
                    &[tag, name, requires, source_digest, revision_digest],
                )?))
            }
            Op::Record | Op::RecordCompiled | Op::Find => {
                let record=if matches!(op,Op::Record) {
                    let Some(record)=source_metadata::prepare(s,scope,object(a[0])?)? else {return Ok(nil_value)};
                    Some(record)
                } else {None};
                let prepared=record.map(|record|[Operand::object(record),a[1]]);
                publication::send(
                    s,
                    scope,
                    if matches!(op, Op::Find) {
                        b"moduleSourceFor:"
                    } else {
                        b"recordCompiledSource:named:"
                    },
                    prepared.as_ref().map_or(a,|v|v.as_slice()),
                )
            }
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

unsafe fn reflect_revision(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
    revision: &RevisionMetadata,
) -> Result<Oop, Outcome> {
    unsafe {
        let tag = q.keep(s.id.intern_name(b"sourceRevision")?)?;
        let name = q.keep(s.id.intern_name(&revision.name)?)?;
        let requires = revision
            .requires
            .iter()
            .map(|name| q.keep(s.id.intern_name(name)?))
            .collect::<Result<Vec<_>, _>>()?;
        let requires = s.vector(q, &requires)?;
        let non_method = s.text(q, &revision.non_method_digest)?;
        let method = s.text(q, &revision.method_digest)?;
        let digest = s.text(q, &revision.revision_digest)?;
        let text=compiled_source::text(s,q,source)?.ok_or_else(invalid)?;
        let record=compiled_source::find(s,q,source)?.ok_or_else(invalid)?;
        s.vector(q,&[tag,name,requires,text,non_method,method,digest,record])
    }
}

// G0 reflects only the registered source set. Constructing this value does not
// publish a generation, seal registration or mutate dispatch.
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
