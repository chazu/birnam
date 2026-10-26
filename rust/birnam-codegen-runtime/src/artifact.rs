//! Ordinary resumable object artifacts. Validate the frozen snapshot before body
//! execution; TCC owns relocation, SourceRegistry owns source publication.
use super::*;
use birnam_codegen::{artifact as format, source_metadata::Metadata, source_revision};
use std::os::unix::ffi::OsStrExt;
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
#[derive(Clone, Copy)]
enum Mode {
    Output,
    From,
    Root,
    Synchronous,
    Embedded,
}
unsafe fn compile(c: Oop, a: *const Oop, n: usize, mode: Mode) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != match mode {
            Mode::Output => 2,
            Mode::From | Mode::Root => 3,
            _ => 4,
        } {
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
        let mut filename = if !matches!(mode, Mode::Output) {
            let Some(path) = s.source_bytes(&q, args[2])? else {
                return fail(s, b"RustCodegen source path was not a String");
            };
            path
        } else {
            b"<compiler>".to_vec()
        };
        if matches!(mode, Mode::Root | Mode::Embedded) {
            filename.extend_from_slice(b"/<module>");
        }
        let Some(tree) = s.source_tree(&q, args[0], source.len())? else {
            s.syntax_error(&q, &source)?;
            return s.named(b"nil");
        };
        let mut program = match birnam_codegen::lower_program(&tree, &source) {
            Ok(p) => p,
            Err(_) => return fail(s, b"RustCodegen could not lower the source"),
        };
        let required = source_revision::native_requirements(&tree, &source);
        let mut libraries = Vec::new();
        if matches!(mode, Mode::Embedded) {
            let length = count(s, &q, args[3])?;
            if length > 4096 {
                return fail(s, b"too many embedded libraries");
            }
            let mut seen = Vec::new();
            for i in 0..length {
                let row = at(s, &q, args[3], i)?;
                if count(s, &q, row)? != 2 {
                    return fail(s, b"embedded library input must contain name and bytes");
                }
                let name = s.bytes(at(s, &q, row, 0)?)?;
                if name.is_empty()
                    || name == b"."
                    || name == b".."
                    || name.iter().any(|b| matches!(b, 0 | b'/' | b'\\'))
                {
                    return fail(s, b"embedded library name must be a bare filename");
                }
                if seen.contains(&name) {
                    return fail(s, b"duplicate embedded library input");
                }
                seen.push(name.clone());
                let contents = s.bytes(at(s, &q, row, 1)?)?;
                if contents.is_empty() || contents.len() > 256 * 1024 * 1024 {
                    return fail(s, b"embedded library size is invalid");
                }
                if required.contains(&name) {
                    libraries.push((name, contents));
                }
            }
        }
        let synchronous = if matches!(mode, Mode::Synchronous) {
            let truth = s.named(b"true")?;
            let falsity = s.named(b"false")?;
            if args[3] != truth && args[3] != falsity {
                return Err(invalid());
            }
            args[3] == truth
        } else {
            false
        };
        if let Some(error) = s.embed_program(&q, &mut program, &filename)? {
            return Ok(error);
        }
        if let Some(error) = s.prepare_foreign(&q, &mut program)? {
            return Ok(error);
        }
        let generated = q.keep(if synchronous {
            let block = birnam_codegen::Block {
                params: Vec::new(),
                body: program.body,
            };
            let portable =
                birnam_codegen::portable::encode_block(&block, &[]).map_err(|_| invalid())?;
            let request =
                birnam_codegen::backend_request::Request::synchronous_factory(portable, &source)
                    .ok_or_else(invalid)?;
            let birnam_codegen::backend_request::Request::ModuleFactory {
                program,
                source,
                dispositions,
            } = request
            else {
                return Err(invalid());
            };
            let choices = dispositions
                .into_iter()
                .map(|v| {
                    s.named(if v { b"true" } else { b"false" })
                        .map(|v| q.keep(v))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let choices = s.vector(&q, &choices)?;
            let program = q.keep(s.text(&program)?);
            let source = q.keep(s.text(&source)?);
            s.send(
                s.named(b"NativeBackend")?,
                b"moduleFactory:source:dispositions:",
                &[program, source, choices],
            )?
        } else {
            emit_program(s, program, &source, Some(&filename), true)?
        });
        if s.is_error(generated)? {
            return Ok(generated);
        }
        let metadata = Metadata::inspect(&tree, &source);
        let Some(generated) = format::emit_oop(
            &tree,
            &source,
            &s.bytes(generated)?,
            &output,
            &metadata,
            &libraries,
        ) else {
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
    unsafe { compile(c, a, n, Mode::Output) }
}
pub(super) unsafe extern "C" fn compile_from(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, Mode::From) }
}
pub(super) unsafe extern "C" fn compile_root(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, Mode::Root) }
}
pub(super) unsafe extern "C" fn compile_synchronous(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, Mode::Synchronous) }
}
pub(super) unsafe extern "C" fn compile_embedded(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { compile(c, a, n, Mode::Embedded) }
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
        let view = q.keep(s.send(contents, b"bytes", &[])?);
        let snapshot = q.keep(s.send(view, b"clone", &[])?);
        let length = abi::as_size(s.send(snapshot, b"sizeWord", &[])?).ok_or_else(invalid)?;
        let mut magic = Vec::new();
        for i in 0..length.min(4) {
            let byte = s.send(snapshot, b"atWord:", &[abi::size(i).unwrap()])?;
            magic.push(
                abi::as_size(byte)
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or_else(invalid)?,
            );
        }
        if magic != b"\x7fELF" {
            reject!(b"RustCodegen artifact is not an Id native object");
        }
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
        if !s.exact(q, result, b"VectorVt")? || !matches!(count(s, q, result)?, 4 | 5) {
            reject!(b"RustCodegen artifact initializer returned an invalid record");
        }
        let method = at(s, q, result, 0)?;
        let method_vt = q.keep(s.send(s.id.root, b"methodBehavior", &[])?);
        let mut behavior = q.keep(s.send(s.id.root, b"behaviorOf:", &[method])?);
        let mut visited = std::collections::HashSet::new();
        while behavior != method_vt {
            if behavior.is_null() || !visited.insert(behavior) {
                reject!(b"RustCodegen artifact Method is invalid");
            }
            behavior = q.keep(s.send(s.id.root, b"parentOf:", &[behavior])?);
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
            reject!(b"RustCodegen legacy compiled arity-adapter artifacts are incompatible");
        }
        let adjacent = count(s, q, native)?;
        let embedded = if count(s, q, result)? == 5 {
            Some(at(s, q, result, 4)?)
        } else {
            None
        };
        if adjacent != 0 || embedded.is_some() {
            let key = s.names[b"Alien".as_slice()];
            let provider = q.keep(s.send(s.named(b"Globals")?, b"at:", &[key])?);
            let provider = if provider == nil {
                q.keep(s.named(b"Alien")?)
            } else {
                provider
            };
            let path_bytes = s.bytes(path)?;
            let directory = std::path::Path::new(std::ffi::OsStr::from_bytes(&path_bytes))
                .parent()
                .unwrap_or(std::path::Path::new("."));
            for i in 0..adjacent {
                let name = at(s, q, native, i)?;
                let filename = s.bytes(name)?;
                let resolved = q.keep(
                    s.text(
                        directory
                            .join(std::ffi::OsStr::from_bytes(&filename))
                            .as_os_str()
                            .as_bytes(),
                    )?,
                );
                let provided =
                    q.keep(s.send(provider, b"provideLibraryPath:named:", &[resolved, name])?);
                if provided == nil || s.is_error(provided)? {
                    return Ok(Some((provided, nil, nil)));
                }
            }
            if let Some(libraries) = embedded {
                let length = count(s, q, libraries)?;
                if length > 4096 {
                    reject!(b"too many embedded libraries");
                }
                for i in 0..length {
                    let row = at(s, q, libraries, i)?;
                    if count(s, q, row)? != 2 {
                        reject!(b"invalid embedded library record");
                    }
                    let name = at(s, q, row, 0)?;
                    let encoded = s.bytes(at(s, q, row, 1)?)?;
                    if encoded.is_empty()
                        || encoded.len() % 2 != 0
                        || encoded.len() > 512 * 1024 * 1024
                    {
                        reject!(b"invalid embedded library encoding");
                    }
                    let nibble = |b| match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        _ => None,
                    };
                    let decoded = encoded
                        .chunks_exact(2)
                        .map(|p| Some((nibble(p[0])? << 4) | nibble(p[1])?))
                        .collect::<Option<Vec<_>>>();
                    let Some(decoded) = decoded else {
                        reject!(b"invalid embedded library hex");
                    };
                    let bytes = q.keep(s.text(&decoded)?);
                    let provided =
                        q.keep(s.send(provider, b"provideLibrary:named:", &[bytes, name])?);
                    if provided == nil || s.is_error(provided)? {
                        return Ok(Some((provided, nil, nil)));
                    }
                }
            }
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
        if method == s.named(b"nil")? || s.is_error(method)? {
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
