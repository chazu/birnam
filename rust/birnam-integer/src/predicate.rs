//! Owner-local arithmetic/comparison over exact Integer inputs. The synchronous
//! visitor consumes an existing Boolean before any root-release callback runs.
use super::*;
fn operation(n: &Number, selected: NativeEntry, c: *mut c_void) -> Option<usize> {
    [add as Method, sub, mul, div, modulo, equal, lt, le, gt, ge]
        .iter()
        .position(|method| selected == NativeEntry::new(*method, c))
        .or_else(|| mixed::operation(n, selected))
}
pub(super) unsafe extern "C" fn evaluate(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let n = state(c)?;
        let Some([first, then, left, right, limit, namespace, epoch, visitor]) =
            borrowed_slice(a, count)
        else {
            return Err(invalid());
        };
        let (first, then, left, right, limit) = (
            object(*first)?,
            object(*then)?,
            object(*left)?,
            object(*right)?,
            object(*limit)?,
        );
        let namespace = object(*namespace)?;
        let epoch = epoch.as_word().ok_or_else(invalid)?;
        let visitor = visitor
            .as_entry()
            .filter(|v| v.method.is_some())
            .ok_or_else(invalid)?;
        n.rooted(&[first, then, left, right, limit, namespace], || {
            if epoch == usize::MAX
                || n.send(n.id.receiver, b"usesDefaultAllocation", &[])?
                    .as_word()
                    != Some(1)
                || n.send(n.prototype, b"canOmitIndexing", &[])?.as_word() != Some(1)
            {
                return Ok(Operand::unit());
            }
            for receiver in [left, n.prototype] {
                if n.send(
                    n.id.receiver,
                    b"usesDefaultLookup:",
                    &[Operand::object(receiver)],
                )?
                .as_word()
                    != Some(1)
                {
                    return Ok(Operand::unit());
                }
            }
            let select = |selector, receiver| -> Result<Option<usize>, Outcome> {
                Ok(n.send(
                    n.id.receiver,
                    b"bind:to:",
                    &[Operand::object(selector), Operand::object(receiver)],
                )?
                .as_entry()
                .and_then(|entry| operation(n, entry, c)))
            };
            let Some(arithmetic @ 0..=4) = select(first, left)? else {
                return Ok(Operand::unit());
            };
            let Some(comparison @ 5..=9) = select(then, n.prototype)? else {
                return Ok(Operand::unit());
            };
            // ErrorVt is a live language binding. If Integer itself is designated
            // as Error, ordinary propagation must run rather than numeric folding.
            if n.send(
                namespace,
                b"at:",
                &[Operand::object(n.names[b"Integer".as_slice()])],
            )?
            .as_object()
                != Some(n.prototype)
                || n.binding(b"ErrorVt")?.as_object() == Some(n.behavior)
                || n.read(n.prototype)?.is_none()
                || n.send(n.id.receiver, b"usesDefaultAllocation", &[])?
                    .as_word()
                    != Some(1)
                || n.send(n.id.receiver, b"behaviorEpoch", &[])?.as_word() != Some(epoch)
            {
                return Ok(Operand::unit());
            }
            // These reads use the just-validated default Id fact methods; there
            // are no callbacks between reading mutable inputs and the commit.
            let (Some(left), Some(right), Some(limit)) =
                (n.read(left)?, n.read(right)?, n.read(limit)?)
            else {
                return Ok(Operand::unit());
            };
            let Some(result) =
                super::arithmetic(left, right, arithmetic).and_then(|v| i64::try_from(v).ok())
            else {
                return Ok(Operand::unit());
            };
            let answer = super::comparison(result, limit, comparison).ok_or_else(invalid)?;
            visitor.method.unwrap()(
                visitor.context,
                n.prototype,
                [n.boolean(answer)].as_ptr(),
                1,
            )
            .into_result()
        })
    })())
}
