//! Host-owned running entry and Store's existing mutation authorization hook.
use super::*;

pub(crate) unsafe fn enter_running(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.busy.get()
            || s.candidates.quiescent.get()
            || s.candidates.observing.get()
            || s.candidates.recovery.borrow().is_some()
            || s.send(s.named(b"Execution")?, b"retirementReady", &[])?
                .as_word()
                != Some(1)
        {
            return fail(
                s,
                q,
                b"generationNotQuiescent",
                b"running entry requires an idle Host boundary",
            );
        }
        let store = s.named(b"Store")?;
        for (selector, receiver) in [
            (
                b"setMethodMutationObserver:".as_slice(),
                s.named(b"Generation")?,
            ),
            (b"setMethodMutationAuthorizer:", s.mutation_guard),
        ] {
            let result = call(s, q, store, selector, &[Operand::object(receiver)])?;
            if result != store {
                return if s.is_error(result)? {
                    Ok(Operand::object(result))
                } else {
                    fail(
                        s,
                        q,
                        b"generationObservationUnavailable",
                        b"Store rejected generation callbacks",
                    )
                };
            }
        }
        // Do not publish the phase until both callbacks are installed. No source
        // evaluation or compilation occurs in this transition.
        s.running.set(true);
        Ok(Operand::object(s.named(b"true")?))
    }
}

pub(crate) fn ordinary_mutation_allowed(s: &State) -> bool {
    !s.running.get() || s.candidates.recovery.borrow().is_some()
}

pub(crate) unsafe fn authorize(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        if ordinary_mutation_allowed(s)
            || (s.candidates.expected_mutation.borrow().is_some()
                && !s.candidates.mutation_authorized.replace(true))
        {
            return Ok(Operand::object(s.named(b"true")?));
        }
        reject_mutation(s, q)
    }
}

// This is a runtime Error value, not a parser/compiler diagnostic. A language
// handler may inspect or recover from it without poisoning Codegen lastError.
pub(crate) unsafe fn reject_mutation(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        let kind = symbol(s, q, b"generationNotQuiescent")?;
        let message = s.text(q, b"ordinary method mutation is unavailable while running")?;
        s.send(
            s.named(b"Error")?,
            b"kind:message:",
            &[Operand::object(kind), Operand::object(message)],
        )
    }
}
