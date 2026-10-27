//! Bounded owner-local materialization cache; only canonical bytes and Oops cross
//! compiler/Execution messages. Captures remain independent for every Block.
use super::*;
use birnam_codegen::backend_request::Request;
pub(super) struct Cached {
    resumable: bool,
    program: Vec<u8>,
    pub method: Oop,
    generated_bytes: usize,
}
const MAX_ENTRIES: usize = 64;
const MAX_PROGRAM_BYTES: usize = 64 << 20;
const MAX_GENERATED_BYTES: usize = 128 << 20;
unsafe fn copy_captures(s: &State, q: &Scope<'_>, captures: Oop, count: usize) -> Result<Oop, Oop> {
    unsafe {
        let mut values = Vec::new();
        for i in 0..count {
            values.push(compiled_source::field(s, q, captures, i)?);
        }
        s.vector(q, &values)
    }
}
unsafe fn materialize(c: Oop, a: *const Oop, n: usize, resumable: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [code, captures] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*code, *captures]);
        s.sources.clear_error();
        let nil = s.named(b"nil")?;
        let fail = |sync: &[u8], resum: &[u8]| {
            s.sources.note(if resumable { resum } else { sync });
            Ok(nil)
        };
        let compilation_failed = || {
            fail(
                b"RustCodegen could not compile PortableProgram materialization",
                b"RustCodegen could not compile portable resumable materialization",
            )
        };
        let bytes = q.keep(s.send(s.named(b"CodeObject")?, b"canonicalBytesOf:", &[*code])?);
        if bytes == nil {
            return fail(
                b"RustCodegen materialization needs an admitted CodeObject",
                b"RustCodegen resumable materialization needs an admitted CodeObject",
            );
        }
        if s.is_error(bytes)? {
            return Ok(bytes);
        }
        let program = s.bytes(bytes)?;
        let Ok((_, count)) = birnam_codegen::portable::program_signature(&program) else {
            return fail(
                b"RustCodegen materialization rejected PortableProgram semantics",
                b"RustCodegen resumable materialization rejected PortableProgram semantics",
            );
        };
        if !s.exact(&q, *captures, b"VectorVt")?
            || s.index(q.keep(s.send(*captures, b"size", &[])?))? != count as usize
        {
            return fail(
                b"RustCodegen materialization capture count disagrees with CodeObject",
                b"RustCodegen resumable capture count disagrees with CodeObject",
            );
        }
        let copied = if resumable {
            Some(copy_captures(s, &q, *captures, count as usize)?)
        } else {
            None
        };
        let hit = {
            let mut cache = s.materializations.borrow_mut();
            cache
                .iter()
                .position(|e| e.resumable == resumable && e.program == program)
                .map(|i| {
                    let entry = cache.remove(i);
                    let m = entry.method;
                    cache.push(entry);
                    m
                })
        };
        let mut generated_bytes = 0;
        let method = if let Some(m) = hit {
            q.keep(m)
        } else {
            let request = Request::Program {
                program: program.clone(),
                source: vec![],
                filename: b"<compiler>".to_vec(),
                persistent: false,
                synchronous: !resumable,
            };
            let source = q.keep(image::emit(s, &q, &request)?);
            if s.is_error(source)? {
                return compilation_failed();
            }
            generated_bytes = s.bytes(source)?.len();
            if program.len() > MAX_PROGRAM_BYTES || generated_bytes > MAX_GENERATED_BYTES {
                return compilation_failed();
            }
            let unit = q.keep(s.send(s.named(b"TCC")?, b"compile:", &[source])?);
            if s.is_error(unit)? {
                return compilation_failed();
            }
            let method = q.keep(s.send(unit, b"initialize", &[])?);
            if method == nil || s.is_error(method)? {
                return compilation_failed();
            }
            method
        };
        let captures = match copied {
            Some(v) => v,
            None => copy_captures(s, &q, *captures, count as usize)?,
        };
        let block = q.keep(s.send(
            s.named(b"Block")?,
            b"fromMethod:codeObject:captures:",
            &[method, *code, captures],
        )?);
        if block == nil || s.is_error(block)? {
            return Ok(block);
        }
        if hit.is_none() {
            let mut cache = s.materializations.borrow_mut();
            if !cache
                .iter()
                .any(|e| e.resumable == resumable && e.program == program)
            {
                while !cache.is_empty()
                    && (cache.len() >= MAX_ENTRIES
                        || cache.iter().map(|e| e.program.len()).sum::<usize>() + program.len()
                            > MAX_PROGRAM_BYTES
                        || cache.iter().map(|e| e.generated_bytes).sum::<usize>() + generated_bytes
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
        Ok(block)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn synchronous(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { materialize(c, a, n, false) }
}
pub(super) unsafe extern "C" fn resumable(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { materialize(c, a, n, true) }
}
unsafe fn cache(c: Oop, n: usize, evict: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let count = {
            let mut cache = s.materializations.borrow_mut();
            let count = cache.len();
            if evict {
                cache.clear();
            }
            count
        };
        s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(count).ok_or_else(invalid)?],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn count(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    unsafe { cache(c, n, false) }
}
pub(super) unsafe extern "C" fn evict(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    unsafe { cache(c, n, true) }
}
