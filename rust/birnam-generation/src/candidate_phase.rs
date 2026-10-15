//! Host-owned phase for explicitly coordinated Generation operations.
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
        // Running is an explicit-operation phase, not authorization for every
        // ordinary dictionary edit. No global Store callbacks are installed here.
        s.running.set(true);
        Ok(Operand::object(s.named(b"true")?))
    }
}
