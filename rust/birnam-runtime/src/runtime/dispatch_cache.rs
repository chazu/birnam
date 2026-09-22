//! Bounded specialization of canonical lookup, with no retained roots or leases.
//!
//! An entry is only a memoized dictionary answer. Classification and acquisition
//! still run for every send, including when registration changes without a
//! dictionary mutation. No cache borrow survives a callback.
use super::{CollectionPhase, NativeEntry, Oop, Runtime};
#[cfg(test)]
use core::cell::Cell;
#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: Cell<bool> = const { Cell::new(true) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MISSES: Cell<usize> = const { Cell::new(0) };
}

impl Runtime {
    /// Caller has just proved canonical lookup and holds dispatch-read authority.
    pub(crate) fn cached_default_lookup(&self, shape: Oop, selector: Oop) -> Option<NativeEntry> {
        let state = unsafe { &*self.state.get() };
        #[cfg(test)]
        if !ENABLED.get() {
            return state.vtables.native_lookup(shape, selector);
        }
        // A tiny direct dictionary is cheaper than checking cache metadata.
        // Keep this common path free of cache probes and cache writes.
        let parent = match state.vtables.native_lookup_head(shape, selector) {
            Ok(method) => return method,
            Err(parent) => parent,
        };
        let epoch = state.vtables.mutation_epoch();
        if epoch == u64::MAX || self.collection_phase.get() != CollectionPhase::Idle {
            return state.vtables.native_lookup(parent, selector);
        }
        // Address bits are used only as keys, never dereferenced from metadata.
        let key = selector.addr();
        if let Some(method) = self.dispatch_cache.lookup(epoch, shape.addr(), key) {
            #[cfg(test)]
            HITS.set(HITS.get() + 1);
            return Some(method);
        }
        #[cfg(test)]
        MISSES.set(MISSES.get() + 1);
        let method = state.vtables.native_lookup(parent, selector);
        // Missing methods follow normal DNU resolution each time.
        if let Some(method) = method {
            self.dispatch_cache.insert(epoch, shape.addr(), key, method);
        }
        method
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ownership_tests;
