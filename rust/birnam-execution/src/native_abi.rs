//! Explicit carrier conversion for authored callers. Id remains the dispatcher.
//! Outbound conversions have fixed message shapes. Word/Bytes values cross
//! their owners' current messages. Incoming authored-method signatures remain
//! separate; this module never reads foreign payloads.
use super::*;

unsafe fn binding(s: &Execution, name: &[u8]) -> Result<Oop, Outcome> {
    s.value_factory(name)
}
unsafe fn snapshot(s: &Execution, value: Oop) -> Result<Vec<u8>, Outcome> {
    let raw = unsafe { s.send(value, b"bytes", &[]) }?
        .as_bytes()
        .ok_or_else(invalid)?;
    Ok(unsafe { borrowed_bytes(raw.pointer, raw.length) }
        .ok_or_else(invalid)?
        .to_vec())
}
unsafe fn word(s: &Execution, value: Oop) -> Result<usize, Outcome> {
    unsafe { s.send(value, b"bits", &[]) }?
        .as_word()
        .ok_or_else(invalid)
}

// Fixed conversion surfaces. Every dependency operation remains a current Id
// message; all Object operands are scoped before the first possible callback.
fn object_result(s: &Execution, result: Operand) -> Result<Operand, Outcome> {
    if result.kind() != native::OBJECT { return Err(invalid()); }
    Ok(Operand::object(result.as_object().filter(|v| !v.is_null()).unwrap_or(s.nil)))
}
unsafe fn bytes_result_with_factory(s: &Execution, factory: Oop, result: Operand) -> Result<Operand, Outcome> {
    let raw = result.as_bytes().ok_or_else(invalid)?;
    // Copy the returned borrow before any outgoing message or factory callback.
    let copied = unsafe { borrowed_bytes(raw.pointer, raw.length) }.ok_or_else(invalid)?.to_vec();
    unsafe { s.send(factory, b"fromBytes:", &[Operand::bytes(&copied)]) }
}
pub(super) unsafe extern "C" fn optional_word_argument(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [target,selector,argument,factory]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        let (target,selector,argument,factory)=(object(*target)?,object(*selector)?,object(*argument)?,object(*factory)?);
        let _roots=s.scope(&[r,target,selector,argument,factory]);
        let result=s.id.send(target,selector,&[Operand::object(argument)])?;
        if result.kind()==native::UNIT {return Ok(Operand::object(s.nil));}
        let bits=result.as_word().ok_or_else(invalid)?;
        s.send(factory,b"fromBits:",&[Operand::word(bits)])
    })())
}
pub(super) unsafe extern "C" fn object_word_argument(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [target,selector,argument]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        let (target,selector,argument)=(object(*target)?,object(*selector)?,object(*argument)?);
        let _roots=s.scope(&[r,target,selector,argument]);
        let bits=word(s,argument)?;
        object_result(s,s.id.send(target,selector,&[Operand::word(bits)])?)
    })())
}
pub(super) unsafe extern "C" fn discard_word_argument(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [target,selector,argument]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        let (target,selector,argument)=(object(*target)?,object(*selector)?,object(*argument)?);
        let _roots=s.scope(&[r,target,selector,argument]);
        let bits=word(s,argument)?;
        s.id.send(target,selector,&[Operand::word(bits)])?;
        Ok(Operand::object(s.nil))
    })())
}
pub(super) unsafe extern "C" fn object_byte_argument(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [target,selector,argument]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        let (target,selector,argument)=(object(*target)?,object(*selector)?,object(*argument)?);
        let _roots=s.scope(&[r,target,selector,argument]);
        // The provider may mutate/reallocate its buffer during the target call.
        let copied=snapshot(s,argument)?;
        object_result(s,s.id.send(target,selector,&[Operand::bytes(&copied)])?)
    })())
}
pub(super) unsafe extern "C" fn byte_query_factory(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s=state(c)?;
        let [target,selector,factory]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        let (target,selector,factory)=(object(*target)?,object(*selector)?,object(*factory)?);
        let _roots=s.scope(&[r,target,selector,factory]);
        bytes_result_with_factory(s,factory,s.id.send(target,selector,&[])?)
    })())
}

// Small zero-argument forms for existing native query surfaces.
pub(super) unsafe extern "C" fn word_result(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    query(c, r, a, n, true)
}
pub(super) unsafe extern "C" fn bytes_result(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    query(c, r, a, n, false)
}
fn query(c: *mut c_void, r: Oop, a: *const Operand, n: usize, is_word: bool) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let [target, selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (target, selector) = (object(*target)?, object(*selector)?);
        let roots = s.scope(&[r, target, selector]);
        let result = s.id.send(target, selector, &[])?;
        if is_word {
            let value = result.as_word().ok_or_else(invalid)?;
            let factory = binding(s, b"Word")?;
            roots.keep(factory);
            s.send(factory, b"fromBits:", &[Operand::word(value)])
        } else {
            let raw = result.as_bytes().ok_or_else(invalid)?;
            let bytes = borrowed_bytes(raw.pointer, raw.length)
                .ok_or_else(invalid)?
                .to_vec();
            let factory = binding(s, b"Bytes")?;
            roots.keep(factory);
            s.send(factory, b"fromBytes:", &[Operand::bytes(&bytes)])
        }
    })())
}

// Calling convention is immutable CodeObject metadata, recovered with the Block.
pub(super) unsafe fn implementation_signature(
    s: &Execution,
    implementation: Oop,
) -> Result<Option<birnam_abi::native_signature::Signature>, Outcome> {
    let code = s
        .machines
        .borrow()
        .block_codes
        .get(&implementation.addr())
        .copied();
    let Some(code) = code else { return Ok(None) };
    let roots = s.scope(&[implementation, code]);
    let signature = object(unsafe { s.send(code, b"nativeSignature", &[]) }?)?;
    roots.keep(signature);
    if signature == s.nil {
        return Ok(None);
    }
    let bytes = unsafe { snapshot(s, signature) }?;
    let signature = birnam_abi::native_signature::Signature::parse(&bytes).ok_or_else(invalid)?;
    let factory = unsafe { binding(s, b"CodeObject") }?;
    roots.keep(factory);
    let canonical = object(unsafe {
        s.send(factory, b"canonicalBytesOf:", &[Operand::object(code)])
    }?)?;
    roots.keep(canonical);
    let bytes = unsafe { snapshot(s, canonical) }?;
    let program =
        birnam_artifact_codec::portable_program::inspect(&bytes).map_err(|_| invalid())?;
    if program.parameter_count as usize != signature.arguments().len() + 1 {
        return Err(invalid());
    }
    Ok(Some(signature))
}

pub(super) unsafe fn incoming(
    s: &Execution,
    method: Oop,
    receiver: Oop,
    args: &[Operand],
    signature: &birnam_abi::native_signature::Signature,
) -> Result<Operand, Outcome> {
    if args.len() != signature.arguments().len() {
        return Err(invalid());
    }
    let roots = s.scope(&[method, receiver]);
    for argument in args {
        if let Some(o) = argument.as_object() {
            roots.keep(o)
        }
    }
    // Copy ALL incoming borrows before the first factory/lookup callback.
    let mut copied = Vec::with_capacity(args.len());
    for (&argument, &kind) in args.iter().zip(signature.arguments()) {
        copied.push(match kind {
            b'o' | b'O' => {
                if argument.kind() != native::OBJECT
                    || (kind == b'O' && argument.as_object().is_none_or(|o| o.is_null()))
                {
                    return Err(invalid());
                }
                None
            }
            b'w' => {
                argument.as_word().ok_or_else(invalid)?;
                None
            }
            b'b' => {
                let raw = argument.as_bytes().ok_or_else(invalid)?;
                Some(
                    unsafe { borrowed_bytes(raw.pointer, raw.length) }
                        .ok_or_else(invalid)?
                        .to_vec(),
                )
            }
            _ => return Err(invalid()),
        });
    }
    let mut converted = Vec::with_capacity(args.len());
    for ((&argument, &kind), copied) in args.iter().zip(signature.arguments()).zip(&copied) {
        let value = match kind {
            b'o' | b'O' => argument
                .as_object()
                .filter(|v| !v.is_null())
                .unwrap_or(s.nil),
            b'w' => {
                let factory = unsafe { binding(s, b"Word") }?;
                roots.keep(factory);
                object(unsafe { s.send(factory, b"fromBits:", &[argument]) }?)?
            }
            b'b' => {
                let factory = unsafe { binding(s, b"Bytes") }?;
                roots.keep(factory);
                object(unsafe {
                    s.send(
                        factory,
                        b"fromBytes:",
                        &[Operand::bytes(copied.as_deref().unwrap())],
                    )
                }?)?
            }
            _ => return Err(invalid()),
        };
        roots.keep(value);
        converted.push(Operand::object(value));
    }
    let result = unsafe { s.invoke_body(method, receiver, &converted) }?;
    let value = object(result)?;
    roots.keep(value);
    match signature.result() {
        b'o' => Ok(result),
        // A deferred result evaluates the selected thunk after the authored
        // selector body has returned, preserving every native result carrier.
        b'v' => unsafe { s.send(value, b"value", &[]) },
        b'u' if value == s.nil => Ok(Operand::unit()),
        b'x' => Ok(Operand::unit()),
        b'W' | b'B' if value == s.nil => Ok(Operand::unit()),
        b'w' | b'W' => Ok(Operand::word(unsafe { word(s, value) }?)),
        b'b' | b'B' => {
            let copied = unsafe { snapshot(s, value) }?;
            // One owned return buffer, valid until the next adapted byte result.
            // Callers must copy native borrows before issuing another Id message.
            let mut storage = s.native_return_bytes.borrow_mut();
            *storage = copied;
            Ok(Operand::bytes(&storage))
        }
        _ => Err(invalid()),
    }
}

pub(super) unsafe extern "C" fn block_with_signature(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let [block, signature] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (block, signature) = (object(*block)?, object(*signature)?);
        let roots = s.scope(&[r, block, signature]);
        let (method, code, captures) = {
            let state = s.machines.borrow();
            let code = state
                .block_codes
                .get(&block.addr())
                .copied()
                .ok_or_else(invalid)?;
            let words = state.storage.payload_size(block).ok_or_else(invalid)? / size_of::<usize>();
            let method = ptr::with_exposed_provenance_mut(
                state
                    .storage
                    .read_payload_word(block, 0)
                    .ok_or_else(invalid)?,
            );
            let captures = (1..words)
                .map(|i| {
                    state
                        .storage
                        .read_payload_word(block, i)
                        .map(ptr::with_exposed_provenance_mut)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<Oop>, _>>()?;
            (method, code, captures)
        };
        roots.keep(method);
        roots.keep(code);
        for &v in &captures {
            roots.keep(v)
        }
        let code = object(s.send(
            code,
            b"withNativeSignature:",
            &[Operand::object(signature)],
        )?)?;
        roots.keep(code);
        let vector = binding(s, b"Vector")?;
        roots.keep(vector);
        let mut values = object(s.send(vector, b"new", &[])?)?;
        roots.keep(values);
        for value in captures {
            values = object(s.send(values, b"add:", &[Operand::object(value)])?)?;
            roots.keep(values)
        }
        if s.methods
            .borrow()
            .get(&method.addr())
            .is_some_and(|m| m.resumable.is_some())
        {
            let codegen = binding(s, b"Codegen")?;
            roots.keep(codegen);
            s.send(
                codegen,
                b"materializeSynchronous:captures:",
                &[Operand::object(code), Operand::object(values)],
            )
        } else {
            machine::new_block(
                s,
                &[
                    Operand::object(method),
                    Operand::object(code),
                    Operand::object(values),
                ],
                true,
            )
        }
    })())
}
