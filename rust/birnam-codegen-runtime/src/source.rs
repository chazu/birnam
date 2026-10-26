//! Source records and metadata use the retained value formats. The authored
//! SourceRegistry owns named publication; no native source table is maintained.
use super::*;
use birnam_codegen::source_revision::RevisionMetadata;
#[derive(Default)]
pub(super) struct Sources {
    error: RefCell<Option<Vec<u8>>>,
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
        q: &Scope<'_>,
        value: Oop,
        family: &[u8],
    ) -> Result<bool, Oop> {
        unsafe {
            let family = q.keep(self.named(family)?);
            Ok(self.send(self.id.root, b"behaviorOf:", &[value])? == family)
        }
    }
    pub(super) unsafe fn intern(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let name = q.keep(self.send(self.id.root, b"nameWithBytes", &values)?);
            self.id.send(name, self.id.intern, &[])
        }
    }
    pub(super) unsafe fn byte_values(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            self.send(self.id.root, b"sequence", &values)
        }
    }
    pub(super) unsafe fn source_text(&self, q: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe { Ok(q.keep(self.text(bytes)?)) }
    }
    pub(super) unsafe fn vector(&self, q: &Scope<'_>, values: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            for &value in values {
                q.keep(value);
            }
            let mut result = q.keep(self.send(self.named(b"Vector")?, b"new", &[])?);
            for value in values {
                result = q.keep(self.send(result, b"add:", &[*value])?);
            }
            Ok(result)
        }
    }
    pub(super) unsafe fn symbol_bytes(
        &self,
        q: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Oop> {
        unsafe {
            if !self.exact(q, value, b"SymbolVt")? {
                return Ok(None);
            }
            Ok(Some(self.id.read_bytes(
                value,
                self.names[b"sizeWord".as_slice()],
                self.names[b"atWord:".as_slice()],
            )?))
        }
    }
    pub(super) unsafe fn source_bytes(
        &self,
        q: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Oop> {
        unsafe {
            let text = q.keep(self.send(self.named(b"Codegen")?, b"sourceTextOf:", &[value])?);
            if text == self.named(b"nil")? {
                Ok(None)
            } else {
                self.bytes(text).map(Some)
            }
        }
    }
    pub(super) unsafe fn source_tree(
        &self,
        _q: &Scope<'_>,
        source: Oop,
        length: usize,
    ) -> Result<Option<OwnedCst>, Oop> {
        unsafe {
            let q = self.scope(&[source]);
            let text = q.keep(self.send(self.named(b"Codegen")?, b"sourceTextOf:", &[source])?);
            let tree = q.keep(self.send(self.named(b"Lang")?, b"parse:", &[text])?);
            if !self.exact(&q, tree, b"PegCstVt")? {
                return Ok(None);
            }
            self.project(tree, length, 0, &mut 1_000_000).map(Some)
        }
    }
    pub(super) unsafe fn source_owner(&self, q: &Scope<'_>) -> Result<Oop, Oop> {
        unsafe {
            let owner = q.keep(self.send(
                self.named(b"Globals")?,
                b"at:",
                &[self.names[b"SourceRegistry".as_slice()]],
            )?);
            if owner == self.named(b"nil")? || owner.is_null() {
                Err(invalid())
            } else {
                Ok(owner)
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    Text,
    Compilation,
    RetainedMetadata,
    CompilationValue,
    Revision,
    Metadata,
    Prepare,
    Encoded,
    EmbeddedFiles,
    LastError,
    Note,
    Clear,
    MethodSources,
}
unsafe fn operation(c: Oop, a: *const Oop, n: usize, op: Op) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let expected = match op {
            Op::LastError | Op::Clear | Op::MethodSources => 0,
            Op::CompilationValue => 2,
            _ => 1,
        };
        if n != expected {
            return Err(invalid());
        }
        let q = s.scope(args);
        let nil = q.keep(s.named(b"nil")?);
        match op {
            Op::Text => Ok(compiled_source::text(s, &q, args[0])?.unwrap_or(nil)),
            Op::Compilation => Ok(compiled_source::find(s, &q, args[0])?.unwrap_or(nil)),
            Op::RetainedMetadata => {
                if let Some(record) = compiled_source::find(s, &q, args[0])? {
                    compiled_source::field(s, &q, record, 2)
                } else {
                    Ok(nil)
                }
            }
            Op::CompilationValue => {
                if let Some(text) = compiled_source::text(s, &q, args[0])? {
                    compiled_source::make(s, &q, text, args[1])
                } else {
                    Ok(nil)
                }
            }
            Op::Prepare => Ok(source_metadata::prepare(s, &q, args[0])?.unwrap_or(nil)),
            Op::Encoded => {
                if let Some(metadata) = source_metadata::read(s, &q, args[0])? {
                    s.text(&metadata.encode().ok_or_else(invalid)?)
                } else {
                    Ok(nil)
                }
            }
            Op::Revision => {
                let Some(record) = source_metadata::prepare(s, &q, args[0])? else {
                    return Ok(nil);
                };
                let Some(metadata) = source_metadata::read(s, &q, record)? else {
                    return Ok(nil);
                };
                if let Some(revision) = metadata.revision {
                    reflect_revision(s, &q, record, &revision)
                } else {
                    s.sources.note(b"RustCodegen source was not a String");
                    Ok(nil)
                }
            }
            Op::Metadata => {
                let Some(metadata) = source_metadata::inspect(s, &q, args[0])? else {
                    if s.source_bytes(&q, args[0])?.is_some() {
                        s.sources.note(
                            b"compiled source metadata is missing; rebuild the artifact or image",
                        );
                    }
                    return Ok(nil);
                };
                let Some(revision) = metadata.revision else {
                    return Ok(nil);
                };
                let tag = q.keep(s.intern(b"moduleMetadata")?);
                let name = q.keep(s.intern(&revision.name)?);
                let requires = revision
                    .requires
                    .iter()
                    .map(|name| s.intern(name).map(|v| q.keep(v)))
                    .collect::<Result<Vec<_>, _>>()?;
                let requires = s.vector(&q, &requires)?;
                let source = s.source_text(&q, &metadata.source_digest)?;
                let digest = s.source_text(&q, &revision.revision_digest)?;
                s.vector(&q, &[tag, name, requires, source, digest])
            }
            Op::EmbeddedFiles => {
                let mut paths = Vec::new();
                if let Some(metadata) = source_metadata::inspect(s, &q, args[0])? {
                    for path in metadata.embedded_files {
                        paths.push(s.source_text(&q, &path)?);
                    }
                } else {
                    s.sources.note(b"RustCodegen source was not a String");
                }
                s.vector(&q, &paths)
            }
            Op::MethodSources => {
                let owner = s.source_owner(&q)?;
                let rows = q.keep(s.send(owner, b"sourceRegistry", &[])?);
                let count = s.index(q.keep(s.send(rows, b"size", &[])?))?;
                if count == 0
                    || count > 1_000_001
                    || compiled_source::field(s, &q, rows, 0)? != s.intern(b"sourceRegistryV1")?
                {
                    return Err(invalid());
                }
                let mut sources = Vec::new();
                for i in 1..count {
                    let row = compiled_source::field(s, &q, rows, i)?;
                    if s.index(q.keep(s.send(row, b"size", &[])?))? != 2 {
                        return Err(invalid());
                    }
                    let record = compiled_source::field(s, &q, row, 1)?;
                    let metadata = source_metadata::read(s, &q, record)?.ok_or_else(invalid)?;
                    for text in metadata.method_sources {
                        sources.push(s.source_text(&q, &text)?);
                    }
                }
                s.vector(&q, &sources)
            }
            Op::LastError => {
                let saved = s.sources.error.borrow().clone();
                if let Some(bytes) = saved {
                    s.text(&bytes)
                } else {
                    Ok(nil)
                }
            }
            Op::Note => {
                s.sources.note(&s.bytes(args[0])?);
                s.named(b"true")
            }
            Op::Clear => {
                s.sources.clear_error();
                s.named(b"true")
            }
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Publication leaves the native activation after metadata preparation. The
// registry's current method runs with the caller's original answer destination.
unsafe fn forward(c: Oop, a: *const Oop, n: usize, selector: &[u8], prepare: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [k, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let expected = if selector == b"moduleSourceFor:" {
            1
        } else {
            2
        };
        if args.len() != expected {
            return Err(invalid());
        }
        let q = s.scope(borrowed_slice(a, n).ok_or_else(invalid)?);
        let mut values = args.to_vec();
        if prepare {
            let Some(record) = source_metadata::prepare(s, &q, values[0])? else {
                return s.transfer(b"answerTo:with:", &[*k, s.named(b"nil")?]);
            };
            values[0] = q.keep(record);
        }
        let owner = s.source_owner(&q)?;
        let mut send = vec![owner, s.names[selector], *k];
        send.extend(values);
        s.transfer(b"prepareSend", &send)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn reflect_revision(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
    revision: &RevisionMetadata,
) -> Result<Oop, Oop> {
    unsafe {
        let tag = q.keep(s.intern(b"sourceRevision")?);
        let name = q.keep(s.intern(&revision.name)?);
        let requires = revision
            .requires
            .iter()
            .map(|name| s.intern(name).map(|v| q.keep(v)))
            .collect::<Result<Vec<_>, _>>()?;
        let requires = s.vector(q, &requires)?;
        let non_method = s.source_text(q, &revision.non_method_digest)?;
        let method = s.source_text(q, &revision.method_digest)?;
        let digest = s.source_text(q, &revision.revision_digest)?;
        let text = compiled_source::text(s, q, source)?.ok_or_else(invalid)?;
        let record = compiled_source::find(s, q, source)?.ok_or_else(invalid)?;
        s.vector(
            q,
            &[
                tag, name, requires, text, non_method, method, digest, record,
            ],
        )
    }
}
macro_rules! entry {
    ($name:ident,$op:ident) => {
        pub(super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            _: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            unsafe { operation(c, a, n, Op::$op) }
        }
    };
}
entry!(text, Text);
entry!(compilation, Compilation);
entry!(retained, RetainedMetadata);
entry!(make, CompilationValue);
entry!(revision, Revision);
entry!(metadata, Metadata);
entry!(prepare, Prepare);
entry!(encoded, Encoded);
entry!(embedded, EmbeddedFiles);
entry!(method_sources, MethodSources);
entry!(last_error, LastError);
entry!(note, Note);
entry!(clear, Clear);
macro_rules! delegate {
    ($name:ident,$selector:literal,$prepare:literal) => {
        pub(super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            _: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            unsafe { forward(c, a, n, $selector, $prepare) }
        }
    };
}
delegate!(record, b"recordCompiledSource:named:", true);
delegate!(record_compiled, b"recordCompiledSource:named:", false);
delegate!(find, b"moduleSourceFor:", false);
delegate!(replace, b"replaceCompiledSource:named:", false);
