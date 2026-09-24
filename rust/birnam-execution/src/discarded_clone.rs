//! One producer-proven, nonescaping clone followed by an existing-argument return.
//! Uses the existing inline boundary; unsupported owners execute the baseline.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Plan {
    pub source: usize,
    pub answer: usize,
    pub selector: Oop,
    pub namespace: Oop,
}
pub(super) unsafe fn attach(
    s: &Execution,
    receiver: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let [source, answer, selector, slot, namespace] = args else {
            return Err(invalid());
        };
        let plan = Plan {
            source: source.as_word().ok_or_else(invalid)?,
            answer: answer.as_word().ok_or_else(invalid)?,
            selector: object(*selector)?,
            namespace: object(*namespace)?,
        };
        let slot = slot.as_word().ok_or_else(invalid)?;
        let roots = s.scope(&[receiver, plan.selector, plan.namespace]);
        {
            let methods = s.methods.borrow();
            let r = methods
                .get(&receiver.addr())
                .and_then(|m| m.resumable.as_ref())
                .ok_or_else(invalid)?;
            let binding = r.descriptor.v2_entry_binding().ok_or_else(invalid)?;
            if plan.source >= binding.argument_count
                || plan.answer >= binding.argument_count
                || !r.descriptor.entry_capture_slots().is_empty()
                || r.descriptor.slot_kind(slot) != Some(execution::SlotKind::Oop)
            {
                return Err(invalid());
            }
        }
        // Like withInline:, the producer owns the body proof. This owner checks
        // the invocation shape and the current semantics of every omitted owner.
        let result = s.capture(s.entry(receiver, false)?)?;
        roots.keep(object(result)?);
        s.methods
            .borrow_mut()
            .get_mut(&object(result)?.addr())
            .unwrap()
            .inline = Some(recovery::Inline {
            body: recovery::Body::DiscardedClone(plan),
            owner: receiver,
            return_slot: slot,
        });
        Ok(result)
    }
}
pub(super) unsafe fn try_result(
    s: &Execution,
    machine: Oop,
    plan: Plan,
    args: &[Oop],
) -> Result<Operand, Outcome> {
    unsafe {
        let Some(&source) = args.get(plan.source) else {
            return Ok(Operand::unit());
        };
        let Some(&answer) = args.get(plan.answer) else {
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
            || s.send(
                s.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(source)],
            )?
            .as_word()
                != Some(1)
            || s.send(s.integer, b"canOmitIndexing", &[])?.as_word() != Some(1)
        {
            return Ok(Operand::unit());
        }
        let Some(selected) = machine::lookup_target(s, source, plan.selector)? else {
            return Ok(Operand::unit());
        };
        if s.send(
            s.model,
            b"canDiscardClone:of:",
            &[Operand::entry(selected), Operand::object(source)],
        )?
        .as_word()
            != Some(1)
            || !machine::inline_controls_current(s, machine)?
            || s.send(
                plan.namespace,
                b"at:",
                &[Operand::object(s.names[b"Integer".as_slice()])],
            )?
            .as_object()
                != Some(s.integer)
            || s.send(s.id.receiver, b"behaviorEpoch", &[])?.as_word() != Some(epoch)
        {
            return Ok(Operand::unit());
        }
        // No callback, allocation or state publication between this verdict and
        // the existing owner's atomic inline-return commit.
        Ok(Operand::object(answer))
    }
}
