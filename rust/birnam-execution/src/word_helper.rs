//! Compiler-proven synchronous helper bodies, selected inside their Id boundary.
//! Plans describe unsigned Word operations, never Integer language policy.
use super::*;
pub(super) struct Plan {
    bytes: Vec<u8>,
    bindings: Vec<u32>,
    parameters: usize,
}

pub(super) unsafe extern "C" fn attach(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let [program, bindings, parameters] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bytes = program.as_bytes().ok_or_else(invalid)?;
        let binding_bytes = bindings.as_bytes().ok_or_else(invalid)?;
        let parameters = parameters
            .as_word()
            .filter(|p| *p > 0 && *p <= 17)
            .ok_or_else(invalid)?;
        if bytes.length < 4
            || bytes.length > 100
            || binding_bytes.length > 128
            || binding_bytes.length % 4 != 0
        {
            return Err(invalid());
        }
        let bytes = borrowed_bytes(bytes.pointer, bytes.length)
            .ok_or_else(invalid)?
            .to_vec();
        let bindings = borrowed_bytes(binding_bytes.pointer, binding_bytes.length)
            .ok_or_else(invalid)?
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect::<Vec<_>>();
        if bytes[0] != 1
            || bytes[1] as usize != bindings.len()
            || bytes[2] == 0
            || bytes[2] > 32
            || bytes.len() != 4 + 3 * bytes[2] as usize
            || bindings.iter().any(|b| *b >= parameters as u32 + 32)
        {
            return Err(invalid());
        }
        let mut methods = s.methods.borrow_mut();
        let m = methods.get_mut(&r.addr()).ok_or_else(invalid)?;
        if m.resumable.is_some() || m.implementation.is_some() || m.native_signature.is_some() {
            return Err(invalid());
        }
        m.word_expression = Some(Rc::new(Plan {
            bytes,
            bindings,
            parameters,
        }));
        Ok(Operand::object(r))
    })())
}
pub(super) unsafe extern "C" fn adapter(
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
        let mut methods = s.methods.borrow_mut();
        let m = methods.get_mut(&r.addr()).ok_or_else(invalid)?;
        // Typed adapters retain their carrier conversion and execute normally.
        if m.resumable.is_none() && m.implementation.is_some() && m.native_signature.is_none() {
            m.word_helper_adapter = true;
        }
        Ok(Operand::object(r))
    })())
}
pub(super) unsafe fn try_result(
    s: &Execution,
    method: Oop,
    receiver: Oop,
    args: &[Operand],
) -> Result<Option<Operand>, Outcome> {
    unsafe {
        let implementation = {
            let methods = s.methods.borrow();
            let m = methods.get(&method.addr()).ok_or_else(invalid)?;
            if !m.word_helper_adapter {
                return Ok(None);
            }
            m.implementation.ok_or_else(invalid)?
        };
        let Some(body) = machine::helper_body(s, implementation) else {
            return Ok(None);
        };
        let (plan, entry) = {
            let methods = s.methods.borrow();
            let Some(m) = methods.get(&body.addr()) else {
                return Ok(None);
            };
            let Some(plan) = m.word_expression.clone() else {
                return Ok(None);
            };
            (plan, s.entry(body, false)?)
        };
        if plan.parameters != args.len() + 1 {
            return Ok(None);
        }
        let roots = s.scope(&[implementation, body]);
        let epoch =
            s.id.send_named(s.id.receiver, b"behaviorEpoch", &[])?
                .as_word()
                .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || s.id
                .send_named(
                    s.id.receiver,
                    b"usesDefaultLookup:",
                    &[Operand::object(implementation)],
                )?
                .as_word()
                != Some(1)
            || s.id
                .send_named(
                    s.id.receiver,
                    b"bind:to:",
                    &[
                        Operand::object(s.id.intern_name(&b"value:".repeat(plan.parameters))?),
                        Operand::object(implementation),
                    ],
                )?
                .as_entry()
                != Some(entry)
        {
            return Ok(None);
        }
        let capture_entry = machine::default_capture_entry(s).ok_or_else(invalid)?;
        if !crate::binding_proof::matches_all(s, &[(implementation, b"captureWordAt:", capture_entry)])? {
            return Ok(None);
        }
        let mut operands = vec![Operand::word(epoch), Operand::bytes(&plan.bytes)];
        for &b in &plan.bindings {
            let value = if b == 0 {
                receiver
            } else if (b as usize) < plan.parameters {
                let Some(v) = args[b as usize - 1].as_object().filter(|v| !v.is_null()) else {
                    return Ok(None);
                };
                v
            } else {
                object(s.id.send_named(
                    implementation,
                    b"captureWordAt:",
                    &[Operand::word(b as usize - plan.parameters)],
                )?)?
            };
            roots.keep(value);
            operands.push(Operand::object(value));
        }
        let word = s.value_factory(b"Word")?;
        roots.keep(word);
        // The Word owner fences this expected epoch again after its own callbacks.
        match s
            .optional_query(word, b"tryPrimitiveExpressionAtEpoch:", &operands)
        {
            Ok(v) if v.kind() == native::UNIT => Ok(None),
            Ok(v) if v.as_object().is_some_and(|o| !o.is_null()) => Ok(Some(v)),
            Ok(_) => Err(invalid()),
            Err(e) => Err(e),
        }
    }
}
