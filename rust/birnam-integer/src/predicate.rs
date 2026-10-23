//! Guarded owner-local arithmetic; commit still goes through the current visitor.
use super::*;
pub(super) unsafe extern "C" fn evaluate(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    count: usize,
) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [first, then, left, right, limit, namespace, epoch, visitor] =
            borrowed_slice(a, count).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let epoch = abi::as_size(*epoch).ok_or_else(invalid)?;
        let absent = ptr::null_mut();
        let yes = abi::size(1).unwrap();
        if epoch == usize::MAX >> 1
            || n.send(n.id.root, b"usesDefaultAllocation", &[])? != yes
            || n.send(n.prototype, b"canOmitIndexing", &[])? != yes
        {
            return Ok(absent);
        }
        for r in [*left, n.prototype, *visitor] {
            if n.send(n.id.root, b"usesDefaultLookup:", &[r])? != yes {
                return Ok(absent);
            }
        }
        let select = |selector, r| -> Result<Option<usize>, Oop> {
            let selected = n.send(n.id.root, b"bind:to:", &[selector, r])?;
            Ok(n.operation(selected)?.map(|(op, _)| op))
        };
        let Some(arithmetic @ 0..=4) = select(*first, *left)? else {
            return Ok(absent);
        };
        let Some(comparison @ 5..=9) = select(*then, n.prototype)? else {
            return Ok(absent);
        };
        if n.send(*namespace, b"at:", &[n.names[b"Integer".as_slice()]])? != n.prototype
            || n.binding(b"ErrorVt")? == n.behavior
            || n.read(n.prototype)?.is_none()
            || n.send(n.id.root, b"usesDefaultAllocation", &[])? != yes
            || n.send(n.id.root, b"behaviorEpoch", &[])? != abi::size(epoch).unwrap()
        {
            return Ok(absent);
        }
        let (Some(left), Some(right), Some(limit)) =
            (n.read(*left)?, n.read(*right)?, n.read(*limit)?)
        else {
            return Ok(absent);
        };
        let Some(result) =
            super::arithmetic(left, right, arithmetic).and_then(|v| i64::try_from(v).ok())
        else {
            return Ok(absent);
        };
        let answer = super::comparison(result, limit, comparison).ok_or_else(invalid)?;
        if n.send(n.id.root, b"behaviorEpoch", &[])? != abi::size(epoch).unwrap() {
            return Ok(absent);
        }
        n.send(*visitor, b"commitNumericPredicate:", &[n.boolean(answer)])
    })())
}
