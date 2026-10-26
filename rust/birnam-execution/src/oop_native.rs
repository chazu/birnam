//! Explicit native-facing adapters. Id arguments and results remain Oops;
//! Word/Bytes normalization and synchronous failure/cleanup belong to this owner.
use super::*;
use birnam_abi::native_signature::Signature;
struct Preserve<'a>(&'a Cell<bool>, bool);
impl Drop for Preserve<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
unsafe fn copied_bytes(s: &State, q: &Scope<'_>, value: Oop) -> Result<Oop, Oop> {
    unsafe {
        let buffer = q.keep(s.send(value, b"bytes", &[])?);
        let bytes = s.id.read_bytes(
            buffer,
            s.names[b"sizeWord".as_slice()],
            s.names[b"atWord:".as_slice()],
        )?;
        let values: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let values = q.keep(s.send(s.id.root, b"sequence", &values)?);
        Ok(q.keep(s.send(s.named(b"Bytes")?, b"fromBytes:", &[values])?))
    }
}
unsafe fn signature(s: &State, q: &Scope<'_>, value: Oop) -> Result<Signature, Oop> {
    unsafe {
        let bytes = q.keep(s.send(value, b"bytes", &[])?);
        let bytes = s.id.read_bytes(
            bytes,
            s.names[b"sizeWord".as_slice()],
            s.names[b"atWord:".as_slice()],
        )?;
        Signature::parse(&bytes).ok_or_else(invalid)
    }
}
unsafe fn collection(s: &State, q: &Scope<'_>, value: Oop) -> Result<Vec<Oop>, Oop> {
    unsafe {
        let count = q.keep(s.send(value, b"size", &[])?);
        let count = resumable::index(s, count)?;
        if count > 128 {
            return Err(invalid());
        }
        let mut values = Vec::new();
        for i in 0..count {
            let index = q.keep(resumable::number(s, i)?);
            values.push(q.keep(s.send(value, b"at:", &[index])?));
        }
        Ok(values)
    }
}
unsafe fn outbound(c: Oop, a: *const Oop, n: usize, kind: u8) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let count = match kind {
            0 => 4,
            1 | 2 | 3 | 4 => 3,
            _ => 2,
        };
        if n != count {
            return Err(invalid());
        }
        let q = s.scope(a);
        let target = a[0];
        let selector = a[1];
        let args = match kind {
            0 => vec![a[2]],
            1 | 2 => vec![q.keep(s.send(a[2], b"bits", &[])?)],
            3 => vec![copied_bytes(s, &q, a[2])?],
            _ => vec![],
        };
        let value = q.keep(s.id.send(target, selector, &args)?);
        match kind {
            0 if value.is_null() || value == s.nil => Ok(s.nil),
            0 => s.send(a[3], b"fromBits:", &[value]),
            1 | 3 => Ok(if value.is_null() { s.nil } else { value }),
            2 => Ok(s.nil),
            4 => {
                let bytes = copied_bytes(s, &q, value)?;
                s.send(a[2], b"fromBytes:", &[bytes])
            }
            5 => s.send(s.named(b"Word")?, b"fromBits:", &[value]),
            _ => copied_bytes(s, &q, value),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! outbound {
    ($f:ident,$kind:expr) => {
        pub(super) unsafe extern "C" fn $f(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
            unsafe { outbound(c, a, n, $kind) }
        }
    };
}
outbound!(optional_word, 0);
outbound!(object_word, 1);
outbound!(discard_word, 2);
outbound!(object_bytes, 3);
outbound!(bytes_factory, 4);
outbound!(word_result, 5);
outbound!(bytes_result, 6);
pub(super) unsafe extern "C" fn with_signature(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [block, sig] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*block, *sig]);
        let _ = signature(s, &q, *sig)?;
        let mut fields = s.fields(*block)?;
        for &value in &fields {
            q.keep(value);
        }
        let method = *fields.first().ok_or_else(invalid)?;
        let context = q.keep(s.send(s.id.root, b"sequence", &[method, *sig])?);
        fields[0] = q.keep(s.send(s.native_value, b"withContext:", &[context])?);
        let code = q.keep(s.send(method, b"codeObject", &[])?);
        if code != s.nil {
            let code = q.keep(s.send(code, b"withNativeSignature:", &[*sig])?);
            if s.send(code, b"isError", &[])? == s.named(b"true")? {
                return Ok(code);
            }
            let sync = q.keep(s.send(method, b"isSynchronous", &[])?);
            s.send(fields[0], b"admitCodeObject:synchronous:", &[code, sync])?;
        }
        s.make_block(s.block_vt, &fields)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn runtime<'a>() -> Result<&'a State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        state((*p).ns)
    }
}
unsafe fn context(s: &State, c: Oop, index: usize) -> Result<Oop, Oop> {
    unsafe { s.send(c, b"at:", &[abi::size(index).unwrap()]) }
}
unsafe fn notify(s: &State, q: &Scope<'_>, receiver: Oop, error: Oop) {
    unsafe {
        q.keep(error);
        let mut args = [abi::size(1).unwrap(), abi::size(0).unwrap()];
        if s.send(error, b"faultKind", &[]).ok() == Some(s.names[b"nativeFailure".as_slice()]) {
            if let Ok(details) = s.send(error, b"detail", &[]) {
                q.keep(details);
                if let (Ok(a), Ok(b)) = (context(s, details, 0), context(s, details, 1)) {
                    args = [q.keep(a), q.keep(b)];
                }
            }
        }
        let _ = s.send(receiver, b"nativeFailed:detail:", &args);
    }
}
pub(super) unsafe extern "C" fn value(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = runtime()?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let q = s.scope(&[c, r]);
        for &a in args {
            q.keep(a);
        }
        let method = q.keep(context(s, c, 0)?);
        let sig = q.keep(context(s, c, 1)?);
        let sig = signature(s, &q, sig)?;
        if args.len() != sig.arguments().len() + 1 {
            return Err(invalid());
        }
        let _restore = Preserve(&s.preserve, s.preserve.replace(true));
        let answer = (|| {
            let mut values = vec![args[0]];
            for (&value, &kind) in args[1..].iter().zip(sig.arguments()) {
                let value = match kind {
                    b'O' if value.is_null() => return Err(invalid()),
                    b'O' | b'o' => {
                        if value.is_null() {
                            s.nil
                        } else {
                            value
                        }
                    }
                    b'w' => s.send(s.named(b"Word")?, b"fromBits:", &[value])?,
                    b'b' => copied_bytes(s, &q, value)?,
                    _ => return Err(invalid()),
                };
                values.push(q.keep(value));
            }
            let value = q.keep(s.invoke(method, r, &values)?);
            match sig.result() {
                b'o' => Ok(value),
                b'x' => Ok(ptr::null_mut()),
                b'u' if value == s.nil => Ok(ptr::null_mut()),
                b'W' | b'B' if value == s.nil => Ok(ptr::null_mut()),
                b'w' | b'W' => s.send(value, b"bits", &[]),
                b'b' | b'B' => copied_bytes(s, &q, value),
                b'v' => s.send(value, b"value", &[]),
                b'm' => evaluate_request(s, &q, value),
                _ => Err(invalid()),
            }
        })();
        if let Err(error) = answer {
            if sig.observes_failure() {
                notify(s, &q, args[0], error);
            }
        }
        answer
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn evaluate_request(s: &State, q: &Scope<'_>, request: Oop) -> Result<Oop, Oop> {
    unsafe {
        let cleanup = q.keep(s.send(request, b"nativeCleanup", &[])?);
        let _restore = Preserve(&s.preserve, s.preserve.replace(true));
        let answer = (|| {
            let target = q.keep(s.send(request, b"nativeReceiver", &[])?);
            let selector = q.keep(s.send(request, b"nativeSelector", &[])?);
            let spec = q.keep(s.send(request, b"nativeSignature", &[])?);
            let spec = signature(s, q, spec)?;
            if !matches!(spec.result(), b'v' | b'x') {
                return Err(invalid());
            }
            let values = q.keep(s.send(request, b"nativeArguments", &[])?);
            let values = collection(s, q, values)?;
            if values.len() != spec.arguments().len() {
                return Err(invalid());
            }
            let mut args = Vec::new();
            for (&value, &kind) in values.iter().zip(spec.arguments()) {
                args.push(q.keep(match kind {
                    b'O' => value,
                    b'o' => {
                        if value == s.nil {
                            ptr::null_mut()
                        } else {
                            value
                        }
                    }
                    b'w' => s.send(value, b"bits", &[])?,
                    b'b' => copied_bytes(s, q, value)?,
                    _ => return Err(invalid()),
                }));
            }
            let value = q.keep(s.id.send(target, selector, &args)?);
            Ok(if spec.result() == b'x' { s.nil } else { value })
        })();
        if let Err(error) = answer {
            q.keep(error);
        }
        if cleanup != s.nil {
            let cleaned = s.send(cleanup, b"value", &[]);
            if answer.is_ok() {
                cleaned?;
            }
        }
        answer
    }
}
pub(super) unsafe extern "C" fn fail(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [status, detail] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*status, *detail]);
        let status = q.keep(s.send(*status, b"bits", &[])?);
        if !abi::as_size(status).is_some_and(|v| v > 0 && v <= u32::MAX as usize) {
            return Err(invalid());
        }
        let detail = q.keep(s.send(*detail, b"bits", &[])?);
        let details = q.keep(s.send(s.id.root, b"sequence", &[status, detail])?);
        s.send(
            s.id.root,
            b"fault:detail:",
            &[s.names[b"nativeFailure".as_slice()], details],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
