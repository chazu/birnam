//! Guarded elimination of compiler-proven nonescaping unsigned Word boxes.
//! This owner alone reads Word storage; all dependency guards are Id messages.
use super::native::*;
use birnam_abi::{
    borrowed_bytes, borrowed_slice,
    id_protocol::{Method, NativeEntry, Operand, Outcome},
};
use core::ffi::c_void;

pub(crate) unsafe extern "C" fn evaluate(
    c: *mut c_void,
    r: birnam_abi::Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    evaluate_inner(c, r, a, count, None)
}
pub(crate) unsafe extern "C" fn evaluate_at_epoch(
    c: *mut c_void,
    r: birnam_abi::Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    let Some(args) = (unsafe { borrowed_slice(a, count) }) else {
        return invalid();
    };
    let Some((epoch, rest)) = args.split_first() else {
        return invalid();
    };
    let Some(epoch) = epoch.as_word() else {
        return invalid();
    };
    evaluate_inner(c, r, rest.as_ptr(), rest.len(), Some(epoch))
}
fn evaluate_inner(
    c: *mut c_void,
    r: birnam_abi::Oop,
    a: *const Operand,
    count: usize,
    expected: Option<usize>,
) -> Outcome {
    done((|| unsafe {
        let n = state(c)?;
        let args = borrowed_slice(a, count).ok_or_else(invalid)?;
        let Some((program, inputs)) = args.split_first() else {
            return Err(invalid());
        };
        let bytes = program.as_bytes().ok_or_else(invalid)?;
        if bytes.length > 100 || inputs.len() > 32 {
            return Err(invalid());
        }
        // Own ABI borrows before any callback, using the protocol's hard limits.
        let mut owned = [0u8; 100];
        owned[..bytes.length]
            .copy_from_slice(borrowed_bytes(bytes.pointer, bytes.length).ok_or_else(invalid)?);
        let bytes = &owned[..bytes.length];
        let mut owned_inputs = [Operand::unit(); 32];
        owned_inputs[..inputs.len()].copy_from_slice(inputs);
        let inputs = &owned_inputs[..inputs.len()];
        if bytes.len() < 4
            || bytes[0] != 1
            || bytes[1] as usize != inputs.len()
            || inputs.len() > 32
            || bytes[2] == 0
            || bytes[2] > 32
            || bytes.len() != 4 + 3 * bytes[2] as usize
        {
            return Err(invalid());
        }
        // Validate all Object carriers before any callback, preserving the
        // prior admission/error order. The caller supplies their lifetime.
        for input in inputs { object(*input)?; }
        (|| {
            // Unit is a refusal, never a failure or a committed expression result.
            let refuse = || Ok(Operand::unit());
            if r != n.prototype {
                return refuse();
            }
            let epoch =
                n.send(n.id.receiver, b"behaviorEpoch", &[])?
                    .as_word()
                    .ok_or_else(invalid)?;
            if expected.is_some_and(|e| e != epoch)
                || epoch == usize::MAX
                || n
                    .send(n.id.receiver, b"usesDefaultAllocation", &[])?
                    .as_word()
                    != Some(1)
                || n
                    .send(
                        n.model,
                        b"canOmitAllocationFor:",
                        &[Operand::object(n.behavior)],
                    )?
                    .as_word()
                    != Some(1)
            {
                return refuse();
            }
            let mut values = [0u64; 64];
            let mut used = 0usize;
            for input in inputs {
                let Some(bits) = n.read(object(*input)?)? else {
                    return refuse();
                };
                values[used] = bits;
                used += 1;
            }
            let (cached_epoch, mut meanings) = n.expression_meanings.get();
            if cached_epoch != epoch {
                meanings = 0;
            }
            for (i, op) in bytes[4..].chunks_exact(3).enumerate() {
                let Some((name, method)) = meaning(op[0]) else {
                    return Err(invalid());
                };
                if meanings & (1u32 << op[0]) == 0 {
                    let selector = n.names[name];
                    let admitted = n.binding_guard.admits(n.id, &[(
                        n.prototype, selector, NativeEntry::new(method, c),
                    )])?;
                    if !admitted && (n
                        .send(
                            n.id.receiver,
                            b"usesDefaultLookup:",
                            &[Operand::object(n.prototype)],
                        )?
                        .as_word()
                        != Some(1)
                        || n
                            .send(
                                n.id.receiver,
                                b"bind:to:",
                                &[Operand::object(selector), Operand::object(n.prototype)],
                            )?
                            .as_entry()
                            != Some(NativeEntry::new(method, c)))
                    {
                        return refuse();
                    }
                    meanings |= 1u32 << op[0];
                }
                if op[1] as usize >= used || op[2] as usize >= used {
                    return Err(invalid());
                }
                let (left, right) = (values[op[1] as usize], values[op[2] as usize]);
                let value = match op[0] {
                    2 => u64::from(left == right),
                    5 => !left,
                    16 => u64::from(left < right),
                    code => {
                        let Some(v) = crate::operations::calculate(left, right, code as usize)
                        else {
                            return refuse();
                        };
                        v
                    }
                };
                if matches!(op[0], 2 | 16) && i + 1 != bytes[2] as usize {
                    return Err(invalid());
                }
                values[used] = value;
                used += 1;
            }
            if bytes[3] as usize + 1 != used {
                return Err(invalid());
            }
            // Guards and input reads may callback. Fence all dependencies before commit.
            if n
                .send(n.id.receiver, b"behaviorEpoch", &[])?
                .as_word()
                != Some(epoch)
            {
                return refuse();
            }
            n.expression_meanings.set((epoch, meanings));
            let value = values[bytes[3] as usize];
            let last = bytes[bytes.len() - 3];
            if matches!(last, 2 | 16) {
                Ok(n.boolean(value != 0))
            } else {
                n.make(value as usize)
            }
        })()
    })())
}

fn meaning(code: u8) -> Option<(&'static [u8], Method)> {
    Some(match code {
        2 => (b"=", equal),
        5 => (b"bitNot", bit_not),
        6 => (b"wrappingAdd:", wrapping_add),
        7 => (b"wrappingSubtract:", wrapping_sub),
        8 => (b"wrappingMultiply:", wrapping_mul),
        9 => (b"unsignedDivide:", unsigned_div),
        10 => (b"unsignedRemainder:", unsigned_rem),
        11 => (b"bitAnd:", bit_and),
        12 => (b"bitOr:", bit_or),
        13 => (b"bitXor:", bit_xor),
        14 => (b"shiftLeft:", shift_left),
        15 => (b"shiftRight:", shift_right),
        16 => (b"unsignedLessThan:", unsigned_less),
        _ => return None,
    })
}
