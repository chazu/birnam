//! Deferred synchronous Id sends for authored native-facing policies.
//! The request owns ordinary Objects; no foreign representation is inspected.
//! Execution owns carrier adaptation and cleanup, never the caller's policy.
use super::*;

unsafe fn query(s: &Execution, roots: &Scope<'_>, r: Oop, name: &[u8]) -> Result<Oop, Outcome> {
    let value = object(unsafe { s.id.send_named(r, name, &[]) }?)?;
    roots.keep(value);
    Ok(value)
}

/// Incoming result signature `m` executes this request. Its messages
/// supply nativeReceiver, nativeSelector, nativeArguments, nativeSignature and
/// nativeCleanup. Signature arguments use the existing O/o/w/b vocabulary;
/// `v` forwards the native result; `x` discards a successful result to language
/// nil. Both propagate failures. Cleanup always follows an attempted send.
pub(super) unsafe extern "C" fn evaluate(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let roots = s.scope(&[r]);
        let cleanup = query(s, &roots, r, b"nativeCleanup")?;
        struct Restore<'a>(&'a Cell<bool>, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(
            &s.preserve_native_failures,
            s.preserve_native_failures.replace(true),
        );
        // A target may return a borrow of an argument. Keep copied argument
        // storage alive until its result has been copied, including cleanup.
        let mut copied = Vec::new();
        let mut answer = (|| {
            let target = query(s, &roots, r, b"nativeReceiver")?;
            let selector = query(s, &roots, r, b"nativeSelector")?;
            let signature = query(s, &roots, r, b"nativeSignature")?;
            let raw =
                s.id.send_named(signature, b"bytes", &[])?
                    .as_bytes()
                    .ok_or_else(invalid)?;
            let bytes = borrowed_bytes(raw.pointer, raw.length)
                .ok_or_else(invalid)?
                .to_vec();
            let signature =
                birnam_abi::native_signature::Signature::parse(&bytes).ok_or_else(invalid)?;
            if !matches!(signature.result(), b'v' | b'x') {
                return Err(invalid());
            }
            let values = query(s, &roots, r, b"nativeArguments")?;
            let values = machine::sequence_bounded(s, values, &roots, 128)?;
            if values.len() != signature.arguments().len() {
                return Err(invalid());
            }
            // Own every byte borrow before the next getter or target callback.
            copied.reserve(values.len());
            let mut args = Vec::with_capacity(values.len());
            for (&value, &kind) in values.iter().zip(signature.arguments()) {
                args.push(match kind {
                    b'O' => Operand::object(value),
                    b'o' => Operand::object(if value == s.nil {
                        ptr::null_mut()
                    } else {
                        value
                    }),
                    b'w' => Operand::word(
                        s.id.send_named(value, b"bits", &[])?
                            .as_word()
                            .ok_or_else(invalid)?,
                    ),
                    b'b' => {
                        let raw =
                            s.id.send_named(value, b"bytes", &[])?
                                .as_bytes()
                                .ok_or_else(invalid)?;
                        copied.push(
                            borrowed_bytes(raw.pointer, raw.length)
                                .ok_or_else(invalid)?
                                .to_vec(),
                        );
                        Operand::bytes(copied.last().unwrap())
                    }
                    _ => return Err(invalid()),
                });
            }
            let result = s.id.send(target, selector, &args)?;
            Ok(if signature.result() == b'x' {
                Operand::object(s.nil)
            } else {
                result
            })
        })();
        if let Ok(value) = answer {
            if let Some(o) = value.as_object().filter(|o| !o.is_null()) {
                roots.keep(o);
            }
        }
        // Cleanup can overwrite the provider's borrowed return buffer.
        let returned_bytes = match answer {
            Ok(value) if value.kind() == native::BYTES => {
                let raw = value.as_bytes().unwrap();
                match borrowed_bytes(raw.pointer, raw.length) {
                    Some(bytes) => Some(bytes.to_vec()),
                    None => {
                        answer = Err(invalid());
                        None
                    }
                }
            }
            _ => None,
        };
        if cleanup != s.nil {
            let cleaned = s.id.send_named(cleanup, b"value", &[]);
            // Keep the original failure if cleanup also fails.
            if answer.is_ok() {
                cleaned?;
            }
        }
        if let Some(bytes) = returned_bytes {
            let mut storage = s.native_return_bytes.borrow_mut();
            *storage = bytes;
            return Ok(Operand::bytes(&storage));
        }
        answer
    })())
}

/// Explicit failure emission, usable by authored protocol validators.
pub(super) unsafe extern "C" fn fail(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        state(c)?;
        let [status, detail] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let status = status
            .as_word()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n != 0)
            .ok_or_else(invalid)?;
        Err(Outcome::failed(
            status,
            detail.as_word().ok_or_else(invalid)?,
        ))
    })())
}

/// Guarded incoming callbacks notify even on failures in carrier adaptation. The
/// receiver's failure observer cannot replace the original native failure.
pub(super) unsafe fn notify_failure(s: &Execution, receiver: Oop, error: Outcome) {
    let _roots = s.scope(&[receiver]);
    let _ = unsafe {
        s.id.send_named(
            receiver,
            b"nativeFailed:detail:",
            &[
                Operand::word(error.status as usize),
                Operand::word(error.detail),
            ],
        )
    };
}
