//! Explicit carrier conversion for authored callers. Id remains the dispatcher.
//! Signature: result kind followed by one kind per argument (o=Object,
//! w=Word, W=optional Word result, b=Bytes, u=Unit result, x=discard result). Word and byte values cross their owners'
//! current messages; this module never reads their payloads.
use super::*;

unsafe fn binding(s: &Execution, name: &[u8]) -> Result<Oop, Outcome> {
    unsafe {
        object(s.id.send_named(
            s.namespace,
            b"at:",
            &[Operand::object(s.id.intern_name(name)?)],
        )?)
    }
}
unsafe fn snapshot(s: &Execution, value: Oop) -> Result<Vec<u8>, Outcome> {
    let raw = unsafe { s.id.send_named(value, b"bytes", &[]) }?
        .as_bytes()
        .ok_or_else(invalid)?;
    Ok(unsafe { borrowed_bytes(raw.pointer, raw.length) }
        .ok_or_else(invalid)?
        .to_vec())
}
unsafe fn word(s: &Execution, value: Oop) -> Result<usize, Outcome> {
    unsafe { s.id.send_named(value, b"bits", &[]) }?
        .as_word()
        .ok_or_else(invalid)
}

pub(super) unsafe extern "C" fn send(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let [target, selector, arguments, signature, factory] =
            borrowed_slice(a, n).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let (target, selector, arguments, signature, factory) = (
            object(*target)?,
            object(*selector)?,
            object(*arguments)?,
            object(*signature)?,
            object(*factory)?,
        );
        let roots = s.scope(&[r, target, selector, arguments, signature, factory]);
        let signature = snapshot(s, signature)?;
        let (result_kind, kinds) = signature.split_first().ok_or_else(invalid)?;
        if !matches!(result_kind, b'o' | b'w' | b'b' | b'u' | b'x' | b'W')
            || kinds.len() > 128
            || kinds.iter().any(|k| !matches!(k, b'o' | b'w' | b'b'))
        {
            return Err(invalid());
        }
        // Do not consult Integer just to enumerate ABI operands. In particular,
        // a target callback may have replaced Integer's constructor already.
        let count =
            s.id.send_named(arguments, b"sizeWord", &[])?
                .as_word()
                .ok_or_else(invalid)?;
        if count != kinds.len() {
            return Err(invalid());
        }
        let mut values = Vec::with_capacity(count);
        for i in 0..count {
            let value = object(s.id.send_named(arguments, b"atWord:", &[Operand::word(i)])?)?;
            roots.keep(value);
            values.push(value);
        }
        invoke_values(s, &roots, target, selector, factory, *result_kind, kinds, &values)
    })())
}
// Shared carrier adaptation: both paths issue the target's current Id message.
unsafe fn invoke_values(
    s: &Execution,
    roots: &Scope<'_>,
    target: Oop,
    selector: Oop,
    factory: Oop,
    result_kind: u8,
    kinds: &[u8],
    values: &[Oop],
) -> Result<Operand, Outcome> {
    unsafe {
        // Own every byte argument before issuing the target callback. The owned
        // vectors stay alive throughout that call, including reentrant collection.
        let mut bytes = Vec::new();
        let mut small = [Operand::unit(); 8];
        let mut large = if values.len() > 8 {
            vec![Operand::unit(); values.len()]
        } else {
            Vec::new()
        };
        let operands = if values.len() > 8 {
            large.as_mut_slice()
        } else {
            &mut small[..values.len()]
        };
        for (i, (&value, &kind)) in values.iter().zip(kinds).enumerate() {
            operands[i] = match kind {
                b'o' => Operand::object(value),
                b'w' => Operand::word(word(s, value)?),
                b'b' => {
                    bytes.push(snapshot(s, value)?);
                    Operand::bytes(bytes.last().unwrap())
                }
                _ => return Err(invalid()),
            };
        }
        let result = s.id.send(target, selector, operands)?;
        // No callback between receiving a native borrow and copying its bytes.
        let copied = if result_kind == b'b' {
            let raw = result.as_bytes().ok_or_else(invalid)?;
            Some(
                borrowed_bytes(raw.pointer, raw.length)
                    .ok_or_else(invalid)?
                    .to_vec(),
            )
        } else {
            None
        };
        if let Some(value) = result.as_object() {
            roots.keep(value);
        }
        match &result_kind {
            b'o' if result.kind() == native::OBJECT => Ok(Operand::object(
                result.as_object().filter(|v| !v.is_null()).unwrap_or(s.nil),
            )),
            b'x' => Ok(Operand::object(s.nil)),
            b'u' if result.kind() == native::UNIT => Ok(Operand::object(s.nil)),
            b'W' if result.kind() == native::UNIT => Ok(Operand::object(s.nil)),
            b'w' | b'W' => s.id.send_named(
                factory,
                b"fromBits:",
                &[Operand::word(result.as_word().ok_or_else(invalid)?)],
            ),
            b'b' => s.id.send_named(
                factory,
                b"fromBytes:",
                &[Operand::bytes(copied.as_deref().unwrap())],
            ),
            _ => Err(invalid()),
        }
    }
}

// Native variadic compiler protocol: target, selector, signature, factory,
// Vector factory, followed by nonescaping literal Object arguments.
pub(super) unsafe extern "C" fn try_send(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if args.len() < 5 || args.len() > 13 {
            return Err(invalid());
        }
        let mut owned_values = [ptr::null_mut(); 13];
        for (i, arg) in args.iter().enumerate() {
            owned_values[i] = object(*arg)?;
        }
        let values = &owned_values[..args.len()];
        let roots = s.scope(&[r]);
        for &v in values {
            roots.keep(v);
        }
        let refuse = || Ok(Operand::unit());
        if r != s.receiver {
            return refuse();
        }
        let epoch =
            s.id.send_named(s.id.receiver, b"behaviorEpoch", &[])?
                .as_word()
                .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || s.id
                .send_named(s.id.receiver, b"usesDefaultLookup:", &[Operand::object(r)])?
                .as_word()
                != Some(1)
            || s.id
                .send_named(
                    s.id.receiver,
                    b"bind:to:",
                    &[
                        Operand::object(s.id.intern_name(
                            b"sendNativeTo:selector:arguments:signature:resultFactory:",
                        )?),
                        Operand::object(r),
                    ],
                )?
                .as_entry()
                != Some(NativeEntry::new(send, c))
        {
            return refuse();
        }
        match s.id.send_named(values[4], b"canOmitNativeArguments", &[]) {
            Ok(v) if v.as_word() == Some(1) => {}
            Ok(_) => return refuse(),
            Err(e) if e.status == native::UNBOUND => return refuse(),
            Err(e) => return Err(e),
        }
        let mut owned_signature = [0u8; 9];
        let signature = match s.id.send_named(values[2], b"tryPrimitiveBytes", &[]) {
            Ok(v) if v.kind() == native::UNIT => return refuse(),
            Ok(v) => {
                let raw = v.as_bytes().ok_or_else(invalid)?;
                if raw.length > owned_signature.len() {
                    return refuse();
                }
                owned_signature[..raw.length]
                    .copy_from_slice(borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?);
                &owned_signature[..raw.length]
            }
            Err(e) if e.status == native::UNBOUND => return refuse(),
            Err(e) => return Err(e),
        };
        let Some((&result_kind, kinds)) = signature.split_first() else {
            return refuse();
        };
        if !matches!(result_kind, b'o' | b'w' | b'b' | b'u' | b'x' | b'W')
            || kinds.len() != values.len() - 5
            || kinds.iter().any(|k| !matches!(k, b'o' | b'w' | b'b'))
            || s.id
                .send_named(s.id.receiver, b"behaviorEpoch", &[])?
                .as_word()
                != Some(epoch)
        {
            return refuse();
        }
        invoke_values(
            s,
            &roots,
            values[0],
            values[1],
            values[3],
            result_kind,
            kinds,
            &values[5..],
        )
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
            s.id.send_named(factory, b"fromBits:", &[Operand::word(value)])
        } else {
            let raw = result.as_bytes().ok_or_else(invalid)?;
            let bytes = borrowed_bytes(raw.pointer, raw.length)
                .ok_or_else(invalid)?
                .to_vec();
            let factory = binding(s, b"Bytes")?;
            roots.keep(factory);
            s.id.send_named(factory, b"fromBytes:", &[Operand::bytes(&bytes)])
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
    let signature = object(unsafe { s.id.send_named(code, b"nativeSignature", &[]) }?)?;
    roots.keep(signature);
    if signature == s.nil {
        return Ok(None);
    }
    let bytes = unsafe { snapshot(s, signature) }?;
    let signature = birnam_abi::native_signature::Signature::parse(&bytes).ok_or_else(invalid)?;
    let factory = unsafe { binding(s, b"CodeObject") }?;
    roots.keep(factory);
    let canonical = object(unsafe {
        s.id.send_named(factory, b"canonicalBytesOf:", &[Operand::object(code)])
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
                object(unsafe { s.id.send_named(factory, b"fromBits:", &[argument]) }?)?
            }
            b'b' => {
                let factory = unsafe { binding(s, b"Bytes") }?;
                roots.keep(factory);
                object(unsafe {
                    s.id.send_named(
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
        b'v' => unsafe { s.id.send_named(value, b"value", &[]) },
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
        let code = object(s.id.send_named(
            code,
            b"withNativeSignature:",
            &[Operand::object(signature)],
        )?)?;
        roots.keep(code);
        let vector = binding(s, b"Vector")?;
        roots.keep(vector);
        let mut values = object(s.id.send_named(vector, b"new", &[])?)?;
        roots.keep(values);
        for value in captures {
            values = object(s.id.send_named(values, b"add:", &[Operand::object(value)])?)?;
            roots.keep(values)
        }
        if s.methods
            .borrow()
            .get(&method.addr())
            .is_some_and(|m| m.resumable.is_some())
        {
            let codegen = binding(s, b"Codegen")?;
            roots.keep(codegen);
            s.id.send_named(
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
