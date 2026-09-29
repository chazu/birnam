//! Source registration and loader-facing evaluation. State belongs to Codegen;
//! parsing, values, roots and execution remain current receiver sends.
use super::*;
use birnam_codegen::source_revision::{self, RevisionMetadata};
use core::cell::Cell;

#[derive(Default)]
pub(super) struct Sources {
    pub entries: RefCell<Vec<(Oop, Oop)>>,
    pub(super) sealed: Cell<bool>,
    error: RefCell<Option<Vec<u8>>>,
}
#[derive(Clone, Copy)]
pub(super) enum Op {
    Revision,
    Metadata,
    EmbeddedFiles,
    Record,
    Find,
    Seal,
    LastError,
    Eval,
    EvalFrom,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Seal | Self::LastError => 0,
            Self::Record | Self::EvalFrom => 2,
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
            if !self.exact(scope, value, b"StringVt")? {
                return Ok(None);
            }
            Ok(Some(self.bytes(value)?))
        }
    }
    unsafe fn symbol_bytes(
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
        scope: &Scope<'_>,
        source: Oop,
        length: usize,
    ) -> Result<Option<OwnedCst>, Outcome> {
        unsafe {
            let lang = scope.keep(self.named(b"Lang")?)?;
            let cst = scope.keep(object(self.send(
                lang,
                b"parse:",
                &[Operand::object(source)],
            )?)?)?;
            if !self.exact(scope, cst, b"PegCstVt")? {
                return Ok(None);
            }
            Ok(Some(self.project(scope, cst, length, 0, &mut 1_000_000)?))
        }
    }
    unsafe fn revision(
        &self,
        scope: &Scope<'_>,
        source: Oop,
    ) -> Result<Option<(Vec<u8>, RevisionMetadata)>, Outcome> {
        unsafe {
            let Some(bytes) = self.source_bytes(scope, source)? else {
                return Ok(None);
            };
            let Some(tree) = self.source_tree(scope, source, bytes.len())? else {
                return Ok(None);
            };
            Ok(source_revision::inspect(&tree, &bytes).map(|revision| (bytes, revision)))
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
    unsafe fn vector(&self, scope: &Scope<'_>, values: &[Oop]) -> Result<Oop, Outcome> {
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
            Op::LastError => {
                let error = s.sources.error.borrow().clone();
                Ok(match error {
                    Some(bytes) => Operand::object(s.text(scope, &bytes)?),
                    None => nil_value,
                })
            }
            Op::Seal => {
                s.sources.sealed.set(true);
                Ok(Operand::object(s.named(b"true")?))
            }
            Op::EmbeddedFiles => {
                let source = object(a[0])?;
                let mut paths = Vec::new();
                if let Some(bytes) = s.source_bytes(scope, source)? {
                    if let Some(tree) = s.source_tree(scope, source, bytes.len())? {
                        for path in source_revision::embedded_files(&tree, &bytes) {
                            paths.push(s.text(scope, &path)?);
                        }
                    }
                } else {
                    s.sources.note(b"RustCodegen source was not a String");
                }
                Ok(Operand::object(s.vector(scope, &paths)?))
            }
            Op::Revision | Op::Metadata => {
                let source = object(a[0])?;
                let Some((bytes, revision)) = s.revision(scope, source)? else {
                    if matches!(op, Op::Revision) {
                        s.sources.note(b"RustCodegen source was not a String");
                    }
                    return Ok(nil_value);
                };
                let tag = scope.keep(s.id.intern_name(if matches!(op, Op::Metadata) {
                    b"moduleMetadata"
                } else {
                    b"sourceRevision"
                })?)?;
                let name = scope.keep(s.id.intern_name(&revision.name)?)?;
                let mut requires = Vec::new();
                for name in revision.requires {
                    requires.push(scope.keep(s.id.intern_name(&name)?)?);
                }
                let requires = s.vector(scope, &requires)?;
                if matches!(op, Op::Metadata) {
                    let source_digest = s.text(scope, &source_revision::source_digest(&bytes))?;
                    let revision_digest = s.text(scope, &revision.revision_digest)?;
                    return Ok(Operand::object(s.vector(
                        scope,
                        &[tag, name, requires, source_digest, revision_digest],
                    )?));
                }
                let non_method = s.text(scope, &revision.non_method_digest)?;
                let method = s.text(scope, &revision.method_digest)?;
                let digest = s.text(scope, &revision.revision_digest)?;
                Ok(Operand::object(s.vector(
                    scope,
                    &[tag, name, requires, source, non_method, method, digest],
                )?))
            }
            Op::Record | Op::Find => {
                let name_object = object(a[if matches!(op, Op::Record) { 1 } else { 0 }])?;
                let Some(name) = s.symbol_bytes(scope, name_object)? else {
                    if matches!(op, Op::Record) {
                        s.sources.note(b"RustCodegen module name was not a Symbol");
                    }
                    return Ok(nil_value);
                };
                let recording = if matches!(op, Op::Record) {
                    let source = object(a[0])?;
                    let Some((bytes, revision)) = s.revision(scope, source)? else {
                        s.sources
                            .note(b"RustCodegen module source is not indexable");
                        return Ok(nil_value);
                    };
                    if revision.name != name {
                        s.sources
                            .note(b"RustCodegen module source identity does not match its name");
                        return Ok(nil_value);
                    }
                    Some((source, bytes))
                } else {
                    None
                };
                // The registry is append-only. Inspect snapshots without a borrow
                // across sends; retry if a dependency registered a new entry.
                loop {
                    if recording.is_some() && s.sources.sealed.get() {
                        s.sources
                            .note(b"RustCodegen module introduction is not available");
                        return Ok(nil_value);
                    }
                    let entries = s.sources.entries.borrow().clone();
                    let mut found = None;
                    for &(old_name, old_source) in &entries {
                        if s.symbol_bytes(scope, old_name)?.as_deref() == Some(name.as_slice()) {
                            found = Some(old_source);
                            break;
                        }
                    }
                    let same = if let (Some(old), Some((_, bytes))) = (found, &recording) {
                        s.source_bytes(scope, old)?.as_deref() == Some(bytes.as_slice())
                    } else {
                        false
                    };
                    if s.sources.entries.borrow().len() != entries.len() {
                        continue;
                    }
                    if recording.is_some() && s.sources.sealed.get() {
                        s.sources
                            .note(b"RustCodegen module introduction is not available");
                        return Ok(nil_value);
                    }
                    let Some((source, _)) = recording else {
                        return Ok(Operand::object(found.unwrap_or(nil)));
                    };
                    if found.is_some() {
                        if same {
                            return Ok(Operand::object(source));
                        }
                        s.sources.note(b"RustCodegen duplicate module source");
                        return Ok(nil_value);
                    }
                    s.sources.entries.borrow_mut().push((name_object, source));
                    return Ok(Operand::object(source));
                }
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
                let Ok(c) = emission::compile_program_from(program, &bytes, &path) else {
                    s.sources.note(b"RustCodegen could not emit the source");
                    return Ok(nil_value);
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
                s.run_method(scope, method)
            }
        }
    }
}
