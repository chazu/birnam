//! Bounded specialization of canonical lookup, with no retained roots or leases.
//!
//! An entry is only a memoized dictionary answer. Classification and acquisition
//! still run for every send, including when registration changes without a
//! dictionary mutation. No cache borrow survives a callback.
use super::{CollectionPhase, MethodAddress, Oop, Runtime};
use core::cell::Cell;

const SLOTS: usize = 64;

#[derive(Clone, Copy)]
struct Entry {
    epoch: u64,
    selector: usize,
    shape: usize,
    method: Option<MethodAddress>,
}

const EMPTY: Entry = Entry {
    epoch: u64::MAX,
    selector: 0,
    shape: 0,
    method: None,
};

pub(super) struct Cache([Cell<Entry>; SLOTS]);

impl Default for Cache {
    fn default() -> Self {
        Self(core::array::from_fn(|_| Cell::new(EMPTY)))
    }
}

impl Cache {
    pub(super) fn clear(&self) {
        for entry in &self.0 {
            entry.set(EMPTY);
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: Cell<bool> = const { Cell::new(true) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MISSES: Cell<usize> = const { Cell::new(0) };
}

impl Runtime {
    /// Caller has just proved canonical lookup and holds dispatch-read authority.
    pub(crate) fn cached_default_lookup(&self, shape: Oop, selector: Oop) -> Option<MethodAddress> {
        let state = unsafe { &*self.state.get() };
        #[cfg(test)]
        if !ENABLED.get() {
            return state.vtables.lookup(shape, selector);
        }
        // A tiny direct dictionary is cheaper than checking cache metadata.
        // Keep this common path free of cache probes and cache writes.
        let parent = match state.vtables.lookup_head(shape, selector) {
            Ok(method) => return method,
            Err(parent) => parent,
        };
        let epoch = state.vtables.mutation_epoch();
        if epoch == u64::MAX || self.collection_phase.get() != CollectionPhase::Idle {
            return state.vtables.lookup(parent, selector);
        }
        // Address bits are used only as keys, never dereferenced from metadata.
        // Selector allocation alignment would otherwise concentrate collisions.
        let key = selector.addr();
        let slot = &self.dispatch_cache.0[((key >> 4) ^ (key >> 10)) & (SLOTS - 1)];
        let entry = slot.get();
        if entry.epoch == epoch && entry.selector == key && entry.shape == shape.addr() {
            #[cfg(test)]
            HITS.set(HITS.get() + 1);
            return entry.method;
        }
        #[cfg(test)]
        MISSES.set(MISSES.get() + 1);
        let method = state.vtables.lookup(parent, selector);
        // Missing methods follow normal DNU resolution each time.
        if method.is_some() {
            slot.set(Entry {
                epoch,
                selector: key,
                shape: shape.addr(),
                method,
            });
        }
        method
    }
}

#[cfg(test)]
mod tests;
