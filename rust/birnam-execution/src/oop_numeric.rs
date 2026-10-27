//! Explicit checked arithmetic over Oop Words; no foreign payload reads.
use super::*;
use crate::numeric_operator::Operator;

unsafe fn bits(s: &State, value: Oop) -> Result<u64, Oop> {
    unsafe {
        if let Some(value) = abi::as_size(value) {
            return Ok(value as u64);
        }
        let low = abi::as_size(s.send(value, b"low32", &[])?)
            .filter(|n| *n <= u32::MAX as usize)
            .ok_or_else(invalid)?;
        let high = abi::as_size(s.send(value, b"high32", &[])?)
            .filter(|n| *n <= u32::MAX as usize)
            .ok_or_else(invalid)?;
        Ok((low as u64) | ((high as u64) << 32))
    }
}
pub(super) unsafe extern "C" fn evaluate(
    _: Oop,
    c: Oop,
    receiver: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [operation, left, right] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        if receiver != s.receiver {
            return Err(invalid());
        }
        let _q = s.scope(&[receiver, *operation, *left, *right]);
        let operation = u32::try_from(bits(s, *operation)?)
            .ok()
            .and_then(Operator::from_declaration)
            .ok_or_else(invalid)?;
        let Some(answer) = operation.evaluate(bits(s, *left)? as i64, bits(s, *right)? as i64)
        else {
            return Ok(ptr::null_mut());
        };
        let answer = answer as u64;
        if let Some(value) = abi::size(answer as usize) {
            return Ok(value);
        }
        let result = s.send(
            s.named(b"Word")?,
            b"fromUnsignedLow:high:",
            &[
                abi::size(answer as u32 as usize).unwrap(),
                abi::size((answer >> 32) as usize).unwrap(),
            ],
        )?;
        // Absence is reserved for arithmetic refusal, not a failed constructor.
        if result.is_null() {
            Err(invalid())
        } else {
            Ok(result)
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
