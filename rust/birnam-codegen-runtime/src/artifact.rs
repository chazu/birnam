//! Durable compilation and ordinary module activation through current owners.
//! TCC owns mappings; execution owns active Methods; Codegen owns source records.
use super::*;
use birnam_codegen::{artifact as emission_artifact, source_revision};

#[derive(Clone, Copy)]
pub(super) enum Op {
    Compile,
    From,
    Root,
    Load,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Load => 1,
            Self::Compile => 2,
            _ => 3,
        }
    }
}
unsafe fn call(
    s: &State,
    q: &Scope<'_>,
    receiver: Oop,
    name: &[u8],
    args: &[Operand],
) -> Result<Oop, Outcome> {
    unsafe { q.keep(object(s.send(receiver, name, args)?)?) }
}
unsafe fn fail(s: &State, message: &[u8]) -> Result<Operand, Outcome> {
    s.sources.note(message);
    unsafe { Ok(Operand::object(s.named(b"nil")?)) }
}
unsafe fn global(s: &State, q: &Scope<'_>, name: &[u8]) -> Result<Oop, Outcome> {
    unsafe {
        let key = q.keep(s.id.intern_name(name)?)?;
        call(s, q, s.named(b"Globals")?, b"at:", &[Operand::object(key)])
    }
}
unsafe fn at(s: &State, q: &Scope<'_>, value: Oop, index: usize) -> Result<Oop, Outcome> {
    unsafe {
        let index = call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(index)],
        )?;
        call(s, q, value, b"at:", &[Operand::object(index)])
    }
}
unsafe fn symbol_bytes(s: &State, value: Oop) -> Result<Vec<u8>, Outcome> {
    unsafe {
        let bytes = s
            .send(s.id.receiver, b"symbolBytesOf:", &[Operand::object(value)])?
            .as_bytes()
            .ok_or_else(invalid)?;
        Ok(borrowed_bytes(bytes.pointer, bytes.length)
            .ok_or_else(invalid)?
            .to_vec())
    }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        s.sources.clear_error();
        if matches!(op, Op::Load) {
            return load(s, q, object(args[0])?);
        }
        let source = object(args[0])?;
        let output = object(args[1])?;
        let Some(bytes) = s.source_bytes(q, source)? else {
            return fail(s, b"RustCodegen source was not a String");
        };
        let Some(path) = s.source_bytes(q, output)? else {
            return fail(s, b"RustCodegen output path was not a String");
        };
        let filename = if matches!(op, Op::Compile) {
            b"<compiler>".to_vec()
        } else {
            let Some(mut filename) = s.source_bytes(q, object(args[2])?)? else {
                return fail(s, b"RustCodegen source path was not a String");
            };
            if matches!(op, Op::Root) {
                filename.extend_from_slice(b"/<module>");
            }
            filename
        };
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            s.syntax_error(q, source)?;
            return Ok(Operand::object(s.named(b"nil")?));
        };
        let mut program = match lower_program(&tree, &bytes) {
            Ok(p) => p,
            Err(_) => return fail(s, b"RustCodegen could not lower the source"),
        };
        if let Some(error) = s.embed_program(q, &mut program, &filename)? {
            return Ok(Operand::object(error));
        }
        let mut adapters = Vec::new();
        for arity in birnam_codegen::source_metadata::named_adapter_arities(&program) {
            let c = emission::compile_named(arity).map_err(|_| invalid())?;
            let c = s.text(q, &c)?;
            let bytes = call(
                s,
                q,
                s.named(b"TCC")?,
                b"compileObject:",
                &[Operand::object(c)],
            )?;
            if s.is_error(bytes)? {
                return Ok(Operand::object(bytes));
            }
            adapters.push((arity, s.bytes(bytes)?));
        }
        let Ok(generated) = emission::compile_program_from(program, &bytes, &filename) else {
            return fail(s, b"RustCodegen could not emit the source");
        };
        let metadata = birnam_codegen::source_metadata::Metadata::inspect(&tree, &bytes);
        let Some(generated) =
            emission_artifact::emit(&tree, &bytes, &generated, &path, &metadata, &adapters)
        else {
            return fail(s, b"RustCodegen artifact identity is invalid");
        };
        let c = s.text(q, &generated)?;
        let object_bytes = call(
            s,
            q,
            s.named(b"TCC")?,
            b"compileObject:",
            &[Operand::object(c)],
        )?;
        if s.is_error(object_bytes)? {
            return Ok(Operand::object(object_bytes));
        }
        // File IO's binary String contract is preserved; no filesystem adapter here.
        let contents = s.text(q, &s.bytes(object_bytes)?)?;
        let result = call(
            s,
            q,
            s.named(b"System")?,
            b"writeFile:contents:",
            &[Operand::object(output), Operand::object(contents)],
        )?;
        if s.is_error(result)? {
            return Ok(Operand::object(result));
        }
        if result == s.named(b"nil")? {
            return fail(s, b"RustCodegen could not write the artifact");
        }
        source_metadata::persist(s, q, source, &metadata)?;
        Ok(Operand::object(output))
    }
}
unsafe fn load(s: &State, q: &Scope<'_>, path: Oop) -> Result<Operand, Outcome> {
    unsafe {
        if s.sources.sealed.get() {
            return s.send(
                s.named(b"Error")?,
                b"kind:",
                &[Operand::object(
                    s.id.intern_name(b"moduleIntroductionNotAvailable")?,
                )],
            );
        }
        if !s.exact(q, path, b"StringVt")? {
            return fail(s, b"RustCodegen load path was not a String");
        }
        let contents = call(
            s,
            q,
            s.named(b"System")?,
            b"readFile:",
            &[Operand::object(path)],
        )?;
        if s.is_error(contents)? {
            return Ok(Operand::object(contents));
        }
        let bytes = s.bytes(contents)?;
        if !bytes.starts_with(b"\x7fELF") {
            return fail(s, b"RustCodegen artifact is not an Id native object");
        }
        let snapshot = call(
            s,
            q,
            s.named(b"Bytes")?,
            b"fromBytes:",
            &[Operand::bytes(&bytes)],
        )?;
        let descriptor = call(
            s,
            q,
            s.named(b"Artifact")?,
            b"inspectBytes:from:",
            &[Operand::object(snapshot), Operand::object(path)],
        )?;
        let nil = s.named(b"nil")?;
        if descriptor == nil {
            return fail(s, b"RustCodegen artifact inspection failed");
        }
        if s.is_error(descriptor)? {
            return Ok(Operand::object(descriptor));
        }
        let kind = call(s, q, descriptor, b"kind", &[])?;
        if symbol_bytes(s, kind)? != b"module" {
            return fail(s, b"RustCodegen artifact is not a module");
        }
        let capabilities = call(s, q, descriptor, b"capabilities", &[])?;
        let mut compatible = false;
        for index in 0..s.index(q, capabilities, b"size")? {
            if symbol_bytes(s, at(s, q, capabilities, index)?)? == emission_artifact::CAPABILITY {
                compatible = true;
            }
        }
        if !compatible {
            return fail(s, b"RustCodegen artifact native contract is incompatible");
        }
        let requires = call(s, q, descriptor, b"requires", &[])?;
        let count = s.index(q, requires, b"size")?;
        if count != 0 {
            let manager = global(s, q, b"Module")?;
            if manager == nil {
                return fail(
                    s,
                    b"RustCodegen artifact module dependency surface is unavailable",
                );
            }
            for index in 0..count {
                let required = at(s, q, requires, index)?;
                if call(s, q, manager, b"isLoaded:", &[Operand::object(required)])?
                    != s.named(b"true")?
                {
                    return fail(s, b"RustCodegen artifact module requirement is unavailable");
                }
            }
        }
        let unit = call(
            s,
            q,
            s.named(b"TCC")?,
            b"loadObject:",
            &[Operand::object(snapshot)],
        )?;
        if s.is_error(unit)? {
            return Ok(Operand::object(unit));
        }
        let result = call(s, q, unit, b"initialize", &[])?;
        if result == nil || s.is_error(result)? {
            return Ok(Operand::object(result));
        }
        if !s.exact(q, result, b"VectorVt")? || s.index(q, result, b"size")? != 4 {
            return fail(
                s,
                b"RustCodegen artifact initializer returned an invalid record",
            );
        }
        let method = at(s, q, result, 0)?;
        if !s.exact(q, method, b"MethodVt")? {
            return fail(s, b"RustCodegen artifact Method is invalid");
        }
        let source = at(s, q, result, 1)?;
        let Some(source_bytes) = s.source_bytes(q, source)? else {
            return fail(s, b"RustCodegen artifact source is missing");
        };
        let digest = call(s, q, descriptor, b"sourceDigest", &[])?;
        if source_revision::source_digest(&source_bytes).as_slice() != s.bytes(digest)? {
            return fail(s, b"RustCodegen artifact source digest is incompatible");
        }
        let encoded = at(s, q, result, 2)?;
        let Some(metadata) = birnam_codegen::source_metadata::Metadata::decode(&s.bytes(encoded)?)
        else {
            return fail(
                s,
                b"RustCodegen artifact compiled source metadata is invalid",
            );
        };
        if !metadata.matches_source(&source_bytes) {
            return fail(
                s,
                b"RustCodegen artifact compiled source metadata mismatches source",
            );
        }
        let name = call(s, q, descriptor, b"identity", &[])?;
        let module = &metadata.module;
        let expected_requires = module.as_ref().map(|m| m.1.as_slice()).unwrap_or(&[]);
        if module.as_ref().is_some_and(|m| m.0.is_empty()) || expected_requires.len() != count {
            return fail(s, b"RustCodegen artifact requirements are incompatible");
        }
        if let Some((identity, _)) = module
            && *identity != symbol_bytes(s, name)?
        {
            return fail(s, b"RustCodegen artifact source identity is incompatible");
        }
        for (index, required) in expected_requires.iter().enumerate() {
            if *required != symbol_bytes(s, at(s, q, requires, index)?)? {
                return fail(s, b"RustCodegen artifact requirements are incompatible");
            }
        }
        if let Some(revision) = &metadata.revision {
            let revision_digest = call(s, q, descriptor, b"revisionDigest", &[])?;
            if revision.name != symbol_bytes(s, name)?
                || revision.revision_digest.as_slice() != s.bytes(revision_digest)?
            {
                return fail(s, b"RustCodegen artifact source identity is incompatible");
            }
            let previous = q.keep(object(source::operation(
                s,
                q,
                source::Op::Find,
                &[Operand::object(name)],
            )?)?)?;
            if previous != nil && s.bytes(previous)? != source_bytes {
                return fail(s, b"RustCodegen duplicate module source");
            }
        }
        if s.sources.sealed.get() {
            return s.send(
                s.named(b"Error")?,
                b"kind:",
                &[Operand::object(
                    s.id.intern_name(b"moduleIntroductionNotAvailable")?,
                )],
            );
        }
        source_metadata::persist(s, q, source, &metadata)?;
        let adapter_rows = at(s, q, result, 3)?;
        let mut adapters = Vec::new();
        let mut previous = None;
        for i in 0..s.index(q, adapter_rows, b"size")? {
            let row = at(s, q, adapter_rows, i)?;
            if s.index(q, row, b"size")? != 2 {
                return Err(invalid());
            }
            let arity = s
                .send(
                    s.named(b"Integer")?,
                    b"wordOf:",
                    &[Operand::object(at(s, q, row, 0)?)],
                )?
                .as_word()
                .filter(|a| *a <= 16)
                .ok_or_else(invalid)? as u32;
            if previous.is_some_and(|p| p >= arity) {
                return Err(invalid());
            }
            previous = Some(arity);
            let bytes = s.bytes(at(s, q, row, 1)?)?;
            let bytes = call(
                s,
                q,
                s.named(b"Bytes")?,
                b"fromBytes:",
                &[Operand::bytes(&bytes)],
            )?;
            adapters.push((arity, bytes));
        }
        let _activation = s.adapter_scope(Some(adapters));
        let result = q.keep(object(s.run_method(q, method)?)?)?;
        if result == nil || s.is_error(result)? {
            return Ok(Operand::object(result));
        }
        if metadata.revision.is_some() {
            let recorded = q.keep(object(source::operation(
                s,
                q,
                source::Op::RecordCompiled,
                &[Operand::object(source), Operand::object(name)],
            )?)?)?;
            if recorded == nil || s.is_error(recorded)? {
                return Ok(Operand::object(recorded));
            }
        }
        Ok(Operand::object(result))
    }
}
