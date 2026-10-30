//! Bounded expression evaluation uses current Method witnesses and object inputs.
use super::native::*;
use birnam_abi::{self as abi, Oop};
use core::ptr;
pub(crate) fn meaning(code: u8) -> Option<&'static [u8]> {
    Some(match code {
        2 => b"=",
        5 => b"bitNot",
        6 => b"wrappingAdd:",
        7 => b"wrappingSubtract:",
        8 => b"wrappingMultiply:",
        9 => b"unsignedDivide:",
        10 => b"unsignedRemainder:",
        11 => b"bitAnd:",
        12 => b"bitOr:",
        13 => b"bitXor:",
        14 => b"shiftLeft:",
        15 => b"shiftRight:",
        16 => b"unsignedLessThan:",
        _ => return None,
    })
}
pub(crate) fn code_for(name: &[u8]) -> Option<u8> {
    (0..=16).find(|&code| meaning(code) == Some(name))
}
pub(crate) unsafe extern "C" fn evaluate(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { inner(c, r, a, n, None) }.unwrap_or_else(abi::fault_reference)
}
pub(crate) unsafe extern "C" fn evaluate_at_epoch(
    _id: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = abi::borrowed_slice(a, n).ok_or(s.invalid())?;
        let Some((epoch, rest)) = a.split_first() else {
            return Err(s.invalid());
        };
        inner(
            c,
            r,
            rest.as_ptr(),
            rest.len(),
            Some(abi::as_size(*epoch).ok_or(s.invalid())?),
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn inner(
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
    expected: Option<usize>,
) -> Result<Oop, Oop> {
    unsafe {
        let s = state(c)?;
        let a = abi::borrowed_slice(a, n).ok_or(s.invalid())?;
        let Some((program, inputs)) = a.split_first() else {
            return Err(s.invalid());
        };
        if inputs.len() > 32 || inputs.iter().any(|o| o.is_null()) {
            return Err(s.invalid());
        }
        let bytes = s.byte_reader.read(*program)?;
        if bytes.len() < 4 {
            return Err(s.invalid());
        }
        let operations = bytes[2] as usize;
        if operations == 0 || operations > 32 {
            return Err(s.invalid());
        }
        let count = 4 + 3 * operations;
        if bytes.len() != count {
            return Err(s.invalid());
        }
        if bytes[0] != 1
            || bytes[1] as usize != inputs.len()
            || inputs.len() > 32
            || inputs.iter().any(|o| o.is_null())
        {
            return Err(s.invalid());
        }
        let refuse = ptr::null_mut();
        if r != s.prototype {
            return Ok(refuse);
        }
        let epoch = abi::as_size(s.send(s.id.root, b"behaviorEpoch", &[])?).ok_or(s.invalid())?;
        if expected.is_some_and(|e| e != epoch)
            || s.send(s.id.root, b"usesDefaultAllocation", &[])? != abi::size(1).unwrap()
            || s.send(s.model, b"canOmitAllocationFor:", &[s.behavior])? != abi::size(1).unwrap()
        {
            return Ok(refuse);
        }
        let mut values = [0u64; 64];
        if !s.read_inputs(inputs, &mut values)? {
            return Ok(refuse);
        }
        let mut used = inputs.len();
        let (cached, mut meanings) = s.expression_meanings.get();
        if cached != epoch {
            meanings = 0;
        }
        for (i, op) in bytes[4..].chunks_exact(3).enumerate() {
            let name = meaning(op[0]).ok_or(s.invalid())?;
            if meanings & (1u32 << op[0]) == 0 {
                if s.send(
                    s.id.root,
                    b"nativeBindingsMatch:",
                    &[
                        s.prototype,
                        s.names[name],
                        s.witnesses[op[0] as usize].get(),
                    ],
                )? != abi::size(1).unwrap()
                {
                    return Ok(refuse);
                }
                meanings |= 1u32 << op[0];
            }
            if op[1] as usize >= used || op[2] as usize >= used {
                return Err(s.invalid());
            }
            let (left, right) = (values[op[1] as usize], values[op[2] as usize]);
            let value = match op[0] {
                2 => u64::from(left == right),
                5 => !left,
                16 => u64::from(left < right),
                code => {
                    let Some(v) = crate::operations::calculate(left, right, code as usize) else {
                        return Ok(refuse);
                    };
                    v
                }
            };
            if matches!(op[0], 2 | 16) && i + 1 != bytes[2] as usize {
                return Err(s.invalid());
            }
            values[used] = value;
            used += 1;
        }
        if bytes[3] as usize + 1 != used {
            return Err(s.invalid());
        }
        if abi::as_size(s.send(s.id.root, b"behaviorEpoch", &[])?) != Some(epoch) {
            return Ok(refuse);
        }
        s.expression_meanings.set((epoch, meanings));
        let value = values[bytes[3] as usize];
        if matches!(bytes[count - 3], 2 | 16) {
            Ok(s.boolean(value != 0))
        } else {
            s.make(value)
        }
    }
}
