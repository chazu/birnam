//! One arithmetic result used only by a comparison; commit while Integer still
//! owns the verdict. Delivery is a fresh send, before Integer releases roots.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Plan {
    pub first: Oop,
    pub then: Oop,
    pub inputs: [usize; 3],
    pub namespace: Oop,
}
pub(super) unsafe fn attach(
    s: &Execution,
    receiver: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let [first, then, left, right, limit, slot, namespace] = args else {
            return Err(invalid());
        };
        let plan = Plan {
            first: object(*first)?,
            then: object(*then)?,
            inputs: [
                left.as_word().ok_or_else(invalid)?,
                right.as_word().ok_or_else(invalid)?,
                limit.as_word().ok_or_else(invalid)?,
            ],
            namespace: object(*namespace)?,
        };
        let slot = slot.as_word().ok_or_else(invalid)?;
        let roots = s.scope(&[receiver, plan.first, plan.then, plan.namespace]);
        {
            let methods = s.methods.borrow();
            let r = methods
                .get(&receiver.addr())
                .and_then(|m| m.resumable.as_ref())
                .ok_or_else(invalid)?;
            let binding = r.descriptor.v2_entry_binding().ok_or_else(invalid)?;
            if plan.inputs.iter().any(|i| *i >= binding.argument_count)
                || !r.descriptor.entry_capture_slots().is_empty()
                || r.descriptor.slot_kind(slot) != Some(execution::SlotKind::Oop)
            {
                return Err(invalid());
            }
        }
        let value = s.capture(s.entry(receiver, false)?)?;
        roots.keep(object(value)?);
        s.methods
            .borrow_mut()
            .get_mut(&object(value)?.addr())
            .unwrap()
            .inline = Some(recovery::Inline {
            body: recovery::Body::NumericPredicate(plan),
            owner: receiver,
            return_slot: slot,
        });
        Ok(value)
    }
}
pub(super) unsafe extern "C" fn commit(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        if r != s.receiver {
            return Err(invalid());
        }
        let Some([value]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        recovery::commit_predicate(s, object(*value)?)?;
        Ok(Operand::unit())
    })())
}
pub(super) unsafe fn try_result(
    s: &Execution,
    machine: Oop,
    plan: Plan,
    args: &[Oop],
) -> Result<Operand, Outcome> {
    unsafe {
        let [Some(left), Some(right), Some(limit)] = plan.inputs.map(|i| args.get(i).copied())
        else {
            return Ok(Operand::unit());
        };
        let epoch = s
            .send(s.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
            .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || s.send(s.id.receiver, b"usesDefaultAllocation", &[])?
                .as_word()
                != Some(1)
            || !machine::inline_controls_current(s, machine)?
            || s.send(
                plan.namespace,
                b"at:",
                &[Operand::object(s.names[b"Integer".as_slice()])],
            )?
            .as_object()
                != Some(s.integer_factory()?)
        {
            return Ok(Operand::unit());
        }
        // The pending recovery request holds the commit target. Integer sends
        // the result to this receiver before running its root-release callbacks.
        s.send(
            s.integer_factory()?,
            b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
            &[
                Operand::object(plan.first),
                Operand::object(plan.then),
                Operand::object(left),
                Operand::object(right),
                Operand::object(limit),
                Operand::object(plan.namespace),
                Operand::word(epoch),
                Operand::object(s.receiver),
            ],
        )
    }
}
