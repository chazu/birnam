//! Canonical Machine accesses inside the existing Id send implementation.
//!
//! This is an implementation of ordinary Id messages, with no additional
//! primitive or generated-code ABI. Every miss retains ordinary dispatch.

use super::{Oop, Runtime};

#[cfg(test)]
thread_local! {
    static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

impl Runtime {
    /// Called with an active Runtime and the send's dispatch-read lease held.
    /// No borrow, allocation or callback crosses the check-to-access interval.
    pub(crate) fn try_guarded_machine_send(
        &self,
        receiver: Oop,
        selector: Oop,
        arguments: &[Oop],
    ) -> Option<Oop> {
        #[cfg(test)]
        if !ENABLED.get() {
            return None;
        }
        let write = {
            // SAFETY: send_abi establishes owner-thread dynamic authority.
            let state = unsafe { &*self.state.get() };
            let objects = state.execution.objects()?;
            if selector == objects.oop_at {
                false
            } else if selector == objects.oop_at_put {
                true
            } else {
                return None;
            }
        };
        let answer = match (write, arguments) {
            (false, [index]) => self.guarded_frame_oop_at(receiver, *index),
            (true, [index, value]) => self.guarded_frame_oop_at_put(receiver, *index, *value),
            // Arity belongs to the current method, so it can only be handled
            // here after proving that the canonical method would run.
            _ if self.guarded_slot_access_eligible(receiver) => Some(self.canonical().nil),
            _ => None,
        };
        #[cfg(test)]
        if answer.is_some() {
            HITS.set(HITS.get() + 1);
        }
        answer
    }
}

#[cfg(test)]
mod tests;
