//! Bounded materialization leases. Values, compilation, construction and roots
//! cross their owners only through messages; cache lookup compares complete bytes.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Op {
    Synchronous,
    Resumable,
    Count,
    Evict,
}
impl Op {
    pub fn count(self) -> usize {
        if matches!(self, Self::Count | Self::Evict) {
            0
        } else {
            2
        }
    }
}
pub(super) struct Cached {
    resumable: bool,
    program: Vec<u8>,
    pub method: Oop,
    generated_bytes: usize,
}
const MAX_ENTRIES: usize = 64;
const MAX_PROGRAM_BYTES: usize = 64 << 20;
const MAX_GENERATED_BYTES: usize = 128 << 20;

pub(super) unsafe fn operation(
    s: &State,
    scope: &Scope<'_>,
    op: Op,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        if matches!(op, Op::Count | Op::Evict) {
            let count = {
                let mut cache = s.materializations.borrow_mut();
                let count = cache.len();
                if matches!(op, Op::Evict) {
                    cache.clear();
                }
                count
            };
            return s.send(s.named(b"Integer")?, b"fromWord:", &[Operand::word(count)]);
        }
        s.sources.clear_error();
        let resumable = matches!(op, Op::Resumable);
        let nil = scope.keep(s.named(b"nil")?)?;
        let fail = |sync: &[u8], resum: &[u8]| {
            s.sources.note(if resumable { resum } else { sync });
            Ok(Operand::object(nil))
        };
        let code = object(args[0])?;
        let captures = object(args[1])?;
        let factory = scope.keep(s.named(b"CodeObject")?)?;
        let bytes = scope.keep(object(s.send(
            factory,
            b"canonicalBytesOf:",
            &[Operand::object(code)],
        )?)?)?;
        if bytes == nil {
            return fail(
                b"RustCodegen materialization needs an admitted CodeObject",
                b"RustCodegen resumable materialization needs an admitted CodeObject",
            );
        }
        let program = s.bytes(bytes)?;
        let Ok((_, capture_count)) = birnam_codegen::portable::program_signature(&program) else {
            return fail(
                b"RustCodegen materialization rejected PortableProgram semantics",
                b"RustCodegen resumable materialization rejected PortableProgram semantics",
            );
        };
        if !s.exact(scope, captures, b"VectorVt")? {
            return fail(
                b"RustCodegen materialization capture count disagrees with CodeObject",
                b"RustCodegen resumable capture count disagrees with CodeObject",
            );
        }
        if s.index(scope, captures, b"size")? != capture_count as usize {
            return fail(
                b"RustCodegen materialization capture count disagrees with CodeObject",
                b"RustCodegen resumable capture count disagrees with CodeObject",
            );
        }
        // Preserve the old read order: resumable captures are snapshotted
        // before compilation; synchronous captures are read after compilation.
        let copy_captures = || -> Result<Oop, Outcome> {
            let vector = scope.keep(s.named(b"Vector")?)?;
            let mut copied = scope.keep(object(s.send(vector, b"new", &[])?)?)?;
            for i in 0..capture_count {
                let index = scope.keep(object(s.send(
                    s.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(i as usize)],
                )?)?)?;
                let value = scope.keep(object(s.send(
                    captures,
                    b"at:",
                    &[Operand::object(index)],
                )?)?)?;
                copied = scope.keep(object(s.send(
                    copied,
                    b"add:",
                    &[Operand::object(value)],
                )?)?)?;
            }
            Ok(copied)
        };
        let copied = if resumable {
            Some(copy_captures()?)
        } else {
            None
        };
        let hit = {
            let mut cache = s.materializations.borrow_mut();
            if let Some(i) = cache
                .iter()
                .position(|c| c.resumable == resumable && c.program == program)
            {
                let entry = cache.remove(i);
                let method = entry.method;
                cache.push(entry);
                Some(method)
            } else {
                None
            }
        };
        let mut generated_bytes = 0;
        let method = if let Some(method) = hit {
            scope.keep(method)?
        } else {
            let c = if resumable {
                emission::compile_portable(&program, &[])
            } else {
                emission::compile_portable_synchronous(&program)
            };
            let Ok(c) = c else {
                return fail(
                    b"RustCodegen could not compile PortableProgram materialization",
                    b"RustCodegen could not compile portable resumable materialization",
                );
            };
            generated_bytes = c.len();
            if program.len() > MAX_PROGRAM_BYTES || generated_bytes > MAX_GENERATED_BYTES {
                return fail(
                    b"RustCodegen could not compile PortableProgram materialization",
                    b"RustCodegen could not compile portable resumable materialization",
                );
            }
            let result = scope.keep(object(s.native_method(scope, &c, None)?)?)?;
            if result == nil || s.is_error(result)? {
                return fail(
                    b"RustCodegen could not compile PortableProgram materialization",
                    b"RustCodegen could not compile portable resumable materialization",
                );
            }
            result
        };
        let copied = match copied {
            Some(v) => v,
            None => copy_captures()?,
        };
        let block = scope.keep(object(s.send(
            s.named(b"Block")?,
            b"fromMethod:codeObject:captures:",
            &[
                Operand::object(method),
                Operand::object(code),
                Operand::object(copied),
            ],
        )?)?)?;
        if block == nil || s.is_error(block)? {
            return Ok(Operand::object(block));
        }
        if hit.is_none() {
            let mut cache = s.materializations.borrow_mut();
            // A collecting/reentrant constructor may have published this key.
            // Keep one repository lease; the returned Block owns its own Method.
            if !cache
                .iter()
                .any(|c| c.resumable == resumable && c.program == program)
            {
                while !cache.is_empty()
                    && (cache.len() >= MAX_ENTRIES
                        || cache.iter().map(|c| c.program.len()).sum::<usize>() + program.len()
                            > MAX_PROGRAM_BYTES
                        || cache.iter().map(|c| c.generated_bytes).sum::<usize>() + generated_bytes
                            > MAX_GENERATED_BYTES)
                {
                    cache.remove(0);
                }
                cache.push(Cached {
                    resumable,
                    program,
                    method,
                    generated_bytes,
                });
            }
        }
        Ok(Operand::object(block))
    }
}
