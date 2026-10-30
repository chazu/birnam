//! Explicit native-facing adapters. Id arguments and results remain Oops;
//! Normalization, failure observers and deferred calls belong to this owner.
use super::*;
use birnam_abi::native_signature::Signature;
struct Preserve<'a>(&'a Cell<bool>, bool);
impl Drop for Preserve<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
unsafe fn copied_bytes(s: &State, q: &Scope<'_>, value: Oop) -> Result<Oop, Oop> {
    unsafe { new_bytes(s, q, &byte_snapshot(s, q, value)?) }
}
unsafe fn byte_snapshot(s: &State, q: &Scope<'_>, value: Oop) -> Result<Vec<u8>, Oop> {
    unsafe {
        let buffer = q.keep(s.send(value, b"bytes", &[])?);
        s.byte_reader.read(buffer)
    }
}
unsafe fn new_bytes(s: &State, q: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        let values = abi::byte_transfer::buffer(bytes, s.named(b"Bytes")?,
            |r,n,a| s.send(r,n,a), |v| Ok(q.keep(v)))?;
        Ok(q.keep(s.send(s.named(b"Bytes")?, b"fromBytes:", &[values])?))
    }
}
unsafe fn signature(s: &State, q: &Scope<'_>, value: Oop) -> Result<Signature, Oop> {
    unsafe {
        let bytes = q.keep(s.send(value, b"bytes", &[])?);
        let bytes = s.byte_reader.read(bytes)?;
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
        // These explicit adapters require their target, selector, input and
        // factory arguments before any effectful conversion or target send.
        if n != count || a.iter().any(|value| value.is_null()) {
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
    unsafe { construct(c, a, n, false) }
}
pub(super) unsafe extern "C" fn method_with_signature(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { construct(c, a, n, true) }
}
unsafe fn construct(c: Oop, a: *const Oop, n: usize, bound: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [block, sig] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*block, *sig]);
        let parsed = std::sync::Arc::new(signature(s, &q, *sig)?);
        // Signature bytes are immutable admission data, not a mutable object
        // hidden in the context. Public reflection returns a fresh byte value.
        if s.storage.borrow().family_of(*block) != Some(s.block_vt)
            || s.storage.borrow().image_shell(*block)
        {
            return Err(invalid());
        }
        let mut fields = s.fields(*block)?;
        for &value in &fields {
            q.keep(value);
        }
        let method = *fields.first().ok_or_else(invalid)?;
        let mut code = q.keep(s.send(method, b"codeObject", &[])?);
        let synchronous = q.keep(s.send(method, b"isSynchronous", &[])?);
        if code != s.nil {
            let existing = q.keep(s.send(code, b"nativeSignature", &[])?);
            if existing == s.nil || byte_snapshot(s, &q, existing)? != parsed.bytes() {
                let sig = q.keep(new_bytes(s, &q, parsed.bytes())?);
                code = q.keep(s.send(code, b"withNativeSignature:", &[sig])?);
                if s.send(code, b"isError", &[])? == s.named(b"true")? {
                    return Ok(code);
                }
            }
        }
        let declared = conversion(
            s,
            &q,
            method,
            if bound { *block } else { s.nil },
            parsed,
            code,
            synchronous,
        )?;
        if bound {
            return Ok(declared);
        }
        fields[0] = declared;
        s.make_block(s.block_vt, &fields)
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Code, environment, executable identity and source description share the
// conversion's existing context. Sequence owns tracing; no Method binding changes.
unsafe fn conversion(
    s: &State,
    q: &Scope<'_>,
    method: Oop,
    environment: Oop,
    signature: std::sync::Arc<Signature>,
    code: Oop,
    synchronous: Oop,
) -> Result<Oop, Oop> {
    unsafe {
        let context = q.keep(s.send(
            s.id.root,
            b"sequence",
            &[method, environment, s.nil, code, synchronous],
        )?);
        let declared = q.keep(s.send(
            if environment == s.nil {
                s.native_value
            } else {
                s.native_method_value
            },
            b"withContext:",
            &[context],
        )?);
        s.send(context, b"at:put:", &[abi::size(2).unwrap(), declared])?;
        s.native_signatures
            .borrow_mut()
            .insert(context.addr(), signature);
        s.signature_contexts.borrow_mut().insert(declared.addr(), context.addr());
        Ok(declared)
    }
}

pub(super) unsafe fn source(
    s: &State,
    method: Oop,
    q: &Scope<'_>,
) -> Result<Option<(Oop, bool)>, Oop> {
    unsafe {
        let Some((context, _)) = signature_for_method(s, method, q)? else {
            return Ok(None);
        };
        let code = q.keep(context_value(s, context, 3)?);
        let synchronous = context_value(s, context, 4)? == s.named(b"true")?;
        Ok(Some((code, synchronous)))
    }
}

pub(super) unsafe fn body(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe {
        let Some((context, signature)) = signature_for_method(s, method, q)? else {
            return Ok(None);
        };
        let environment = q.keep(context_value(s, context, 1)?);
        if environment == s.nil {
            return Ok(None);
        }
        let mut fields = s.fields(environment)?;
        for &value in &fields {
            q.keep(value);
        }
        let executable = q.keep(context_value(s, context, 0)?);
        // Binding an existing typed Block unwraps exactly its conversion entry.
        // Explicit conversion of an already typed body may instead be nested.
        if let Some((inner, inner_signature)) = signature_for_method(s, fields[0], q)? {
            let inner_executable = q.keep(context_value(s, inner, 0)?);
            if inner_signature.bytes() == signature.bytes()
                && s.send(
                    s.id.root,
                    b"sameNativeMethod:as:",
                    &[inner_executable, executable],
                )? == abi::size(1).unwrap()
            {
                return Ok(Some(environment));
            }
        }
        let source = q.keep(context_value(s, context, 3)?);
        let synchronous = q.keep(context_value(s, context, 4)?);
        fields[0] = q.keep(conversion(
            s,
            q,
            executable,
            s.nil,
            signature,
            source,
            synchronous,
        )?);
        Ok(Some(s.make_block(s.block_vt, &fields)?))
    }
}

pub(super) unsafe fn method_from_block(
    s: &State,
    body: Oop,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe {
        let method = q.keep(s.field(body, 0)?);
        let Some((context, signature)) = signature_for_method(s, method, q)? else {
            return Ok(None);
        };
        if context_value(s, context, 1)? != s.nil {
            return Err(invalid());
        }
        let executable = q.keep(context_value(s, context, 0)?);
        if let Some(shape) = s.admitted.borrow().get(&executable.addr()).copied() {
            let fields = s.fields(body)?;
            if fields.len() != shape.captures + 1 || fields[1..].iter().any(|v| v.is_null()) {
                return Err(invalid());
            }
        }
        let code = q.keep(context_value(s, context, 3)?);
        let synchronous = q.keep(context_value(s, context, 4)?);
        Ok(Some(conversion(
            s,
            q,
            executable,
            body,
            signature,
            code,
            synchronous,
        )?))
    }
}

pub(super) fn note_capture(s: &State, original: Oop, captured: Oop) {
    let mut contexts = s.signature_contexts.borrow_mut();
    if let Some(context) = contexts.get(&original.addr()).copied() {
        contexts.insert(captured.addr(), context);
    }
}
// A weak provider-local association avoids scanning known Methods. The ordinary
// Sequence remains mutable, so verify its current declared Method on every read.
unsafe fn signature_for_method(
    s: &State,
    method: Oop,
    q: &Scope<'_>,
) -> Result<Option<(Oop, std::sync::Arc<Signature>)>, Oop> {
    unsafe {
        let known = s.signature_contexts.borrow().get(&method.addr()).copied();
        if let Some(key) = known {
            let signature = s.native_signatures.borrow().get(&key).cloned();
            if let Some(signature) = signature {
                let context = q.keep(ptr::with_exposed_provenance_mut(key));
                let declared = q.keep(context_value(s, context, 2)?);
                if s.send(s.id.root, b"sameNativeMethod:as:", &[method, declared])? == abi::size(1).unwrap() {
                    return Ok(Some((context, signature)));
                }
            }
            s.signature_contexts.borrow_mut().remove(&method.addr());
        }
        let entries: Vec<_> = s
            .native_signatures
            .borrow()
            .iter()
            .map(|(&key, sig)| (q.keep(ptr::with_exposed_provenance_mut(key)), sig.clone()))
            .collect();
        for (context, signature) in entries {
            let declared = q.keep(s.send(context, b"at:", &[abi::size(2).unwrap()])?);
            if s.send(s.id.root, b"sameNativeMethod:as:", &[method, declared])?
                == abi::size(1).unwrap()
            {
                s.signature_contexts.borrow_mut().insert(method.addr(), context.addr());
                return Ok(Some((context, signature)));
            }
        }
        Ok(None)
    }
}
pub(super) unsafe extern "C" fn native_signature(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*value]);
        let method = if s.storage.borrow().family_of(*value) == Some(s.block_vt) {
            s.field(*value, 0)?
        } else {
            *value
        };
        if let Some((_, sig)) = signature_for_method(s, method, &q)? {
            new_bytes(s, &q, sig.bytes())
        } else {
            Ok(s.nil)
        }
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
unsafe fn context_value(s: &State, c: Oop, index: usize) -> Result<Oop, Oop> {
    unsafe { s.send(c, b"at:", &[abi::size(index).unwrap()]) }
}
unsafe fn notify(s: &State, q: &Scope<'_>, receiver: Oop, error: Oop) {
    unsafe {
        q.keep(error);
        let mut args = [abi::size(1).unwrap(), abi::size(0).unwrap()];
        if s.send(error, b"faultKind", &[]).ok() == Some(s.names[b"nativeFailure".as_slice()]) {
            if let Ok(details) = s.send(error, b"detail", &[]) {
                q.keep(details);
                if let (Ok(a), Ok(b)) = (context_value(s, details, 0), context_value(s, details, 1))
                {
                    args = [q.keep(a), q.keep(b)];
                }
            }
        }
        let _ = s.send(receiver, b"nativeFailed:detail:", &args);
    }
}
pub(super) unsafe extern "C" fn value(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { enter(c, r, a, n, false) }
}
pub(super) unsafe extern "C" fn method_value(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { enter(c, r, a, n, true) }
}
unsafe fn enter(c: Oop, r: Oop, a: *const Oop, n: usize, bound: bool) -> Oop {
    (|| unsafe {
        let s = runtime()?;
        let [k, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bound_arguments;
        let args = if bound {
            bound_arguments = std::iter::once(r)
                .chain(args.iter().copied())
                .collect::<Vec<_>>();
            bound_arguments.as_slice()
        } else {
            args
        };
        let q = s.scope(&[c, r, *k]);
        for &a in args {
            q.keep(a);
        }
        let method = q.keep(context_value(s, c, 0)?);
        let environment = if bound {
            q.keep(context_value(s, c, 1)?)
        } else {
            r
        };
        let sig = s
            .native_signatures
            .borrow()
            .get(&c.addr())
            .cloned()
            .ok_or_else(invalid)?;
        if args.len() != sig.arguments().len() + 1 {
            if sig.observes_failure() {
                if let Some(&receiver) = args.first() {
                    notify(s, &q, receiver, invalid());
                }
            }
            return Err(invalid());
        }
        let answer = (|| {
            if args[1..]
                .iter()
                .zip(sig.arguments())
                .any(|(&value, &kind)| kind == b'O' && value.is_null())
            {
                return Err(invalid());
            }
            // Read all byte views before Word/Bytes factory callbacks can
            // mutate another input. Payload interpretation stays with its owner.
            let copied = args[1..]
                .iter()
                .zip(sig.arguments())
                .map(|(&value, &kind)| {
                    if kind == b'b' {
                        byte_snapshot(s, &q, value).map(Some)
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Result<Vec<_>, Oop>>()?;
            let mut values = vec![args[0]];
            for ((&value, &kind), copied) in args[1..].iter().zip(sig.arguments()).zip(&copied) {
                let value = match kind {
                    b'O' | b'o' => {
                        if value.is_null() {
                            s.nil
                        } else {
                            value
                        }
                    }
                    b'w' => s.send(s.named(b"Word")?, b"fromBits:", &[value])?,
                    b'b' => new_bytes(s, &q, copied.as_deref().ok_or_else(invalid)?)?,
                    _ => return Err(invalid()),
                };
                values.push(q.keep(value));
            }
            let value = q.keep(s.invoke(method, environment, &values)?);
            match sig.result() {
                b'o' => Ok(value),
                b'x' => Ok(ptr::null_mut()),
                b'u' if value == s.nil => Ok(ptr::null_mut()),
                b'W' | b'B' if value == s.nil => Ok(ptr::null_mut()),
                b'w' | b'W' => s.send(value, b"bits", &[]),
                b'b' | b'B' => copied_bytes(s, &q, value),
                b'v' => Ok(value),
                b'm' => evaluate_request(s, &q, value),
                _ => Err(invalid()),
            }
        })();
        if let Err(error) = answer {
            if sig.observes_failure() {
                notify(s, &q, args[0], error);
            }
        }
        let value = q.keep(answer?);
        if sig.result() == b'v' {
            let destination = if sig.observes_failure() {
                let context = q.keep(s.send(s.id.root, b"sequence", &[*k, args[0]])?);
                let answer = q.keep(s.send(s.native_answer, b"withContext:", &[context])?);
                answer
            } else {
                *k
            };
            // The selector body has returned. Defer the chosen branch to Id's
            // continuing driver, outside the synchronous conversion/body scope.
            s.transfer(
                b"prepareSend",
                &[value, s.names[b"value".as_slice()], destination],
            )
        } else {
            s.transfer(b"answerTo:with:", &[*k, value])
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn deferred_answer(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = runtime()?;
        let [_, value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[c, *value]);
        let k = q.keep(context_value(s, c, 0)?);
        let receiver = q.keep(context_value(s, c, 1)?);
        if abi::is_fault(*value) {
            notify(s, &q, receiver, abi::object_reference(*value));
        }
        s.transfer(b"answerTo:with:", &[k, *value])
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
