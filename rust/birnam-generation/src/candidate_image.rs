//! Image application invalidates process-local capabilities without an inverse.
use super::*;

pub(crate) unsafe fn begin_image_application(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        // Resolve the reply before changing ownership. No outgoing send follows
        // invalidation; entered Methods/Blocks retain code through Execution.
        let yes = s.named(b"true")?;
        if s.candidates.busy.get() || s.candidates.image_application.get() {
            return fail(s, q, b"candidateBusy", b"image application cannot interrupt a candidate operation");
        }
        s.candidates.image_application.set(true);
        s.candidates.prepared.borrow_mut().clear();
        s.candidates.ledger.take();
        s.candidates.entries.borrow_mut().clear();
        Ok(Operand::object(yes))
    }
}

pub(crate) unsafe fn finish_image_application(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        let yes = s.named(b"true")?;
        if s.candidates.busy.get() {
            return fail(s, q, b"candidateBusy", b"cannot finish image application during a candidate operation");
        }
        // Cleanup before application must not clear live candidates.
        // After application, it never restores old capabilities.
        s.candidates.image_application.set(false);
        Ok(Operand::object(yes))
    }
}
