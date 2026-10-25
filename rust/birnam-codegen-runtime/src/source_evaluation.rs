//! Retained loader diagnostics and embedding, followed by a continuation handoff.
use super::*;
impl State {
    unsafe fn syntax_error(&self, q: &Scope<'_>, source: &[u8]) -> Result<(), Oop> {
        unsafe {
            let offset = q.keep(self.send(self.named(b"Lang")?, b"furthestFail", &[])?);
            let offset = (self.index(offset)? as i64).clamp(0, source.len() as i64) as usize;
            let start = source[..offset]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            let end = source[offset..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(source.len(), |i| offset + i);
            let line = source[..offset].iter().filter(|&&b| b == b'\n').count() + 1;
            let mut message = format!("{line}:{}: syntax error\n", offset - start + 1).into_bytes();
            message.extend_from_slice(&source[start..end]);
            message.push(b'\n');
            message.extend(
                source[start..offset]
                    .iter()
                    .map(|&b| if b == b'\t' { b } else { b' ' }),
            );
            message.push(b'^');
            self.sources.note(&message);
            Ok(())
        }
    }
}
unsafe fn prepare(s: &State, q: &Scope<'_>, args: &[Oop]) -> Result<Option<Oop>, Oop> {
    unsafe {
        s.sources.clear_error();
        let path = if args.len() == 2 {
            let Some(path) = s.source_bytes(q, args[1])? else {
                s.sources.note(b"RustCodegen source path was not a String");
                return Ok(None);
            };
            path
        } else {
            b"<compiler>".to_vec()
        };
        let source = args[0];
        let Some(bytes) = s.source_bytes(q, source)? else {
            s.sources.note(b"RustCodegen could not lower the source");
            return Ok(None);
        };
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            s.syntax_error(q, &bytes)?;
            return Ok(None);
        };
        let mut program = match birnam_codegen::lower_program(&tree, &bytes) {
            Ok(program) => program,
            Err(error) => {
                s.sources.note(
                    if error == birnam_codegen::LowerError::ThreadedSpecialForm {
                        b"RustCodegen source-sensitive special forms cannot use => threading"
                    } else {
                        b"RustCodegen could not lower the source"
                    },
                );
                return Ok(None);
            }
        };
        if let Some(error) = s.embed_program(q, &mut program, &path)? {
            let message = q.keep(s.send(error, b"message", &[])?);
            s.sources.note(&s.bytes(message)?);
            return Ok(None);
        }
        let native = q.keep(emit_program(s, program, &bytes, Some(&path), true)?);
        if s.is_error(native)? {
            s.sources.note(b"RustCodegen could not emit the source");
            return Ok(None);
        }
        let nil = s.named(b"nil")?;
        let unit = q.keep(s.send(s.named(b"TCC")?, b"compile:", &[native])?);
        if unit == nil || s.is_error(unit)? {
            s.sources.note(b"RustCodegen generated C failed to compile");
            return Ok(None);
        }
        let method = q.keep(s.send(unit, b"initialize", &[])?);
        if method == nil || s.is_error(method)? {
            s.sources.note(b"RustCodegen generated C failed to compile");
            return Ok(None);
        }
        Ok(Some(method))
    }
}
unsafe fn evaluate(c: Oop, a: *const Oop, n: usize, from: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 2 + usize::from(from) {
            return Err(invalid());
        }
        let q = s.scope(args);
        let k = args[0];
        let Some(method) = prepare(s, &q, &args[1..])? else {
            return s.transfer(b"answerTo:with:", &[k, s.named(b"nil")?]);
        };
        let block = q.keep(s.send(
            s.named(b"Block")?,
            b"fromMethod:captureOperands:",
            &[method],
        )?);
        s.transfer(b"prepareSend", &[block, s.names[b"value".as_slice()], k])
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn eval(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { evaluate(c, a, n, false) }
}
pub(super) unsafe extern "C" fn eval_from(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { evaluate(c, a, n, true) }
}
