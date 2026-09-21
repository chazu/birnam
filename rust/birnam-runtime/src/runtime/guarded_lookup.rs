//! Non-effectful eligibility for the existing canonical lookup primitive.

use super::{Method, MethodAddress, Oop, Runtime, Behaviors};

#[cfg(test)]
thread_local! {
    static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

impl Runtime {
    pub(crate) fn guarded_default_lookup_eligible(&self, receiver_vtable: Oop) -> bool {
        #[cfg(test)]
        if !ENABLED.get() {
            return false;
        }
        // SAFETY: bind_active requires active owner-thread authority. This
        // check neither allocates Birnam objects nor invokes foreign code.
        let state = unsafe { &*self.state.get() };
        let globals = state.globals;
        if !state.execution.has_managed_turn()
            || Behaviors::checked_vtable_of(&state.heap, receiver_vtable) != globals.vtable_vt
        {
            return false;
        }
        let epoch = state.vtables.mutation_epoch();
        let (verified_epoch, verified) = self.lookup_guard_cache.get();
        let canonical = MethodAddress::from_method(crate::dispatch::vtable_lookup_method as Method);
        let verified = if epoch != u64::MAX && verified_epoch == epoch {
            verified
        } else {
            let verified = Behaviors::checked_vtable_of(&state.heap, globals.vtable_vt)
                == globals.vtable_vt
                && state.vtables.is_default_lookup(
                    state.vtables.native_direct_lookup(globals.vtable_vt, globals.lookup));
            self.lookup_guard_cache.set((epoch, verified));
            verified
        };
        if !verified || state.execution.has_resumable_thunk(canonical) {
            return false;
        }
        #[cfg(test)]
        HITS.set(HITS.get() + 1);
        true
    }
}

#[cfg(test)]
mod tests;
