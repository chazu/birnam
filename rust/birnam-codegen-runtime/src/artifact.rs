//! Ordinary resumable object artifacts. Validate the frozen snapshot before body
//! execution; TCC owns relocation, SourceRegistry owns source publication.
use super::*;
use birnam_codegen::{artifact as format, source_metadata::Metadata, source_revision};
unsafe fn fail(s: &State, message: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        s.sources.note(message);
        s.named(b"nil")
    }
}
unsafe fn at(s: &State, q: &Scope<'_>, value: Oop, i: usize) -> Result<Oop, Oop> {
    unsafe { compiled_source::field(s, q, value, i) }
}
unsafe fn count(s: &State, q: &Scope<'_>, value: Oop) -> Result<usize, Oop> {
    unsafe { s.index(q.keep(s.send(value, b"size", &[])?)) }
}
unsafe fn name(s: &State, q: &Scope<'_>, value: Oop) -> Result<Vec<u8>, Oop> {
    unsafe { s.symbol_bytes(q, value)?.ok_or_else(invalid) }
}
unsafe fn compile(c: Oop, a: *const Oop, n: usize, from: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 2 + usize::from(from) {
            return Err(invalid());
        }
        let q = s.scope(args);
        s.sources.clear_error();
        let Some(source) = s.source_bytes(&q, args[0])? else {
            return fail(s, b"RustCodegen source was not a String");
        };
        let Some(output) = s.source_bytes(&q, args[1])? else {
            return fail(s, b"RustCodegen output path was not a String");
        };
        let filename = if from {
            let Some(path) = s.source_bytes(&q, args[2])? else {
                return fail(s, b"RustCodegen source path was not a String");
            };
            path
        } else {
            b"<compiler>".to_vec()
        };
        let Some(tree) = s.source_tree(&q, args[0], source.len())? else {
            s.syntax_error(&q, &source)?;
            return s.named(b"nil");
        };
        let mut program = match birnam_codegen::lower_program(&tree, &source) {
            Ok(p) => p,
            Err(_) => return fail(s, b"RustCodegen could not lower the source"),
        };
        if !source_revision::native_requirements(&tree, &source).is_empty() {
            return fail(
                s,
                b"RustCodegen foreign-library artifact packaging remains unconnected",
            );
        }
        if let Some(error) = s.embed_program(&q, &mut program, &filename)? {
            return Ok(error);
        }
        if let Some(error) = s.prepare_foreign(&q, &mut program)? {
            return Ok(error);
        }
        let generated = q.keep(emit_program(s, program, &source, Some(&filename), true)?);
        if s.is_error(generated)? {
            return Ok(generated);
        }
        let metadata = Metadata::inspect(&tree, &source);
        let Some(generated) =
            format::emit_oop(&tree, &source, &s.bytes(generated)?, &output, &metadata)
        else {
            return fail(s, b"RustCodegen artifact identity is invalid");
        };
        let generated = q.keep(s.text(&generated)?);
        let bytes = q.keep(s.send(s.named(b"TCC")?, b"compileObject:", &[generated])?);
        if s.is_error(bytes)? {
            return Ok(bytes);
        }
        let contents = q.keep(s.text(&s.bytes(bytes)?)?);
        let result = q.keep(s.send(
            s.named(b"System")?,
            b"writeFile:contents:",
            &[args[1], contents],
        )?);
        if s.is_error(result)? {
            return Ok(result);
        }
        if result == s.named(b"nil")? {
            return fail(s, b"RustCodegen could not write the artifact");
        }
        Ok(args[1])
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn compile_output(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, false) }
}
pub(super) unsafe extern "C" fn compile_from(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, true) }
}
unsafe fn admit(s: &State, q: &Scope<'_>, path: Oop) -> Result<Option<(Oop, Oop, Oop)>, Oop> {
    unsafe {
        macro_rules! reject {
            ($message:expr) => {{
                s.sources.note($message);
                return Ok(None);
            }};
        }
        if !s.exact(q, path, b"StringVt")? {
            reject!(b"RustCodegen load path was not a String");
        }
        let contents = q.keep(s.send(s.named(b"System")?, b"readFile:", &[path])?);
        if s.is_error(contents)? {
            return Ok(Some((contents, s.named(b"nil")?, s.named(b"nil")?)));
        }
        let bytes = s.bytes(contents)?;
        if !bytes.starts_with(b"\x7fELF") {
            reject!(b"RustCodegen artifact is not an Id native object");
        }
        let factory = q.keep(s.named(b"Bytes")?);
        let values = q.keep(s.byte_values(&bytes)?);
        let snapshot = q.keep(s.send(factory, b"fromBytes:", &[values])?);
        let descriptor = q.keep(s.send(
            s.named(b"Artifact")?,
            b"inspectBytes:from:",
            &[snapshot, path],
        )?);
        let nil = s.named(b"nil")?;
        if descriptor == nil {
            reject!(b"RustCodegen artifact inspection failed");
        }
        if s.is_error(descriptor)? {
            return Ok(Some((descriptor, nil, nil)));
        }
        if name(s, q, q.keep(s.send(descriptor, b"kind", &[])?))? != b"module" {
            reject!(b"RustCodegen artifact is not a module");
        }
        let capabilities = q.keep(s.send(descriptor, b"capabilities", &[])?);
        let mut compatible = false;
        for i in 0..count(s, q, capabilities)? {
            if name(s, q, at(s, q, capabilities, i)?)? == format::CAPABILITY {
                compatible = true;
            }
        }
        if !compatible {
            reject!(b"RustCodegen artifact native contract is incompatible");
        }
        let requires = q.keep(s.send(descriptor, b"requires", &[])?);
        let mut requirements = Vec::new();
        for i in 0..count(s, q, requires)? {
            requirements.push(at(s, q, requires, i)?);
        }
        if !requirements.is_empty() {
            let manager = q.keep(s.send(
                s.named(b"Globals")?,
                b"at:",
                &[s.names[b"Module".as_slice()]],
            )?);
            if manager == nil {
                reject!(b"RustCodegen artifact module dependency surface is unavailable");
            }
            for &required in &requirements {
                if s.send(manager, b"isLoaded:", &[required])? != s.named(b"true")? {
                    reject!(b"RustCodegen artifact module requirement is unavailable");
                }
            }
        }
        let native = q.keep(s.send(descriptor, b"nativeRequires", &[])?);
        if count(s, q, native)? != 0 {
            reject!(b"RustCodegen foreign-library artifact admission remains unconnected");
        }
        let unit = q.keep(s.send(s.named(b"TCC")?, b"loadObject:", &[snapshot])?);
        if s.is_error(unit)? {
            return Ok(Some((unit, nil, nil)));
        }
        let result = q.keep(s.send(unit, b"initialize", &[])?);
        if result == nil {
            return Ok(None);
        }
        if s.is_error(result)? {
            return Ok(Some((result, nil, nil)));
        }
        if !s.exact(q, result, b"VectorVt")? || count(s, q, result)? != 4 {
            reject!(b"RustCodegen artifact initializer returned an invalid record");
        }
        let method = at(s, q, result, 0)?;
        let method_vt = q.keep(s.send(s.id.root, b"methodBehavior", &[])?);
        if s.send(s.id.root, b"behaviorOf:", &[method])? != method_vt {
            reject!(b"RustCodegen artifact Method is invalid");
        }
        let source = at(s, q, result, 1)?;
        let Some(source_bytes) = s.source_bytes(q, source)? else {
            reject!(b"RustCodegen artifact source is missing");
        };
        let digest = q.keep(s.send(descriptor, b"sourceDigest", &[])?);
        if source_revision::source_digest(&source_bytes).as_slice() != s.bytes(digest)? {
            reject!(b"RustCodegen artifact source digest is incompatible");
        }
        let encoded = at(s, q, result, 2)?;
        let Some(metadata) = Metadata::decode(&s.bytes(encoded)?) else {
            reject!(b"RustCodegen artifact compiled source metadata is invalid");
        };
        if !metadata.matches_source(&source_bytes) {
            reject!(b"RustCodegen artifact compiled source metadata mismatches source");
        }
        let identity = q.keep(s.send(descriptor, b"identity", &[])?);
        let identity_bytes = name(s, q, identity)?;
        let expected = metadata
            .module
            .as_ref()
            .map(|m| m.1.as_slice())
            .unwrap_or(&[]);
        if expected.len() != requirements.len() {
            reject!(b"RustCodegen artifact requirements are incompatible");
        }
        if metadata
            .module
            .as_ref()
            .is_some_and(|m| m.0.is_empty() || m.0 != identity_bytes)
        {
            reject!(b"RustCodegen artifact source identity is incompatible");
        }
        for (expected, required) in expected.iter().zip(requirements) {
            if *expected != name(s, q, required)? {
                reject!(b"RustCodegen artifact requirements are incompatible");
            }
        }
        if let Some(revision) = &metadata.revision {
            let digest = q.keep(s.send(descriptor, b"revisionDigest", &[])?);
            if revision.name != identity_bytes
                || revision.revision_digest.as_slice() != s.bytes(digest)?
            {
                reject!(b"RustCodegen artifact source identity is incompatible");
            }
            let previous =
                q.keep(s.send(s.named(b"Codegen")?, b"moduleSourceFor:", &[identity])?);
            if previous != nil && s.bytes(previous)? != source_bytes {
                reject!(b"RustCodegen duplicate module source");
            }
        }
        let adapters = at(s, q, result, 3)?;
        if !s.exact(q, adapters, b"VectorVt")? || count(s, q, adapters)? != 0 {
            reject!(b"RustCodegen compiled arity-adapter artifacts remain unconnected");
        }
        let compilation = if metadata.revision.is_some() {
            source_metadata::retain_metadata(s, q, source, &metadata)?
        } else {
            nil
        };
        Ok(Some((method, compilation, identity)))
    }
}
// Artifact activation retains its synchronous admission/initialization boundary.
// Source bodies and task-owned Machines still use their ordinary continuations.
pub(super) unsafe extern "C" fn load(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [path] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*path]);
        s.sources.clear_error();
        let Some((method, compilation, name)) = admit(s, &q, *path)? else {
            return s.named(b"nil");
        };
        if s.is_error(method)? {
            return Ok(method);
        }
        let block = q.keep(s.send(
            s.named(b"Block")?,
            b"fromMethod:captureOperands:",
            &[method],
        )?);
        let result = q.keep(s.send(block, b"value", &[])?);
        let nil = s.named(b"nil")?;
        if result == nil || s.is_error(result)? || compilation == nil {
            return Ok(result);
        }
        let recorded = q.keep(s.send(
            s.named(b"Codegen")?,
            b"recordCompiledSource:named:",
            &[compilation, name],
        )?);
        Ok(if recorded == nil || s.is_error(recorded)? {
            recorded
        } else {
            result
        })
    })()
    .unwrap_or_else(abi::fault_reference)
}
