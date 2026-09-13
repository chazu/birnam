//! Bounded, untraced scratch; leases clear all values before returning storage.
use super::*;
use std::cell::RefCell;

const MAX_FREE: usize = 4;

#[derive(Default)]
pub(super) struct Storage {
    pub values: Values,
}

pub(in crate::runtime::native_tier) struct Pool {
    free: Rc<RefCell<Vec<Storage>>>,
}

impl Pool {
    pub(super) fn new() -> Self {
        Self {
            free: Rc::new(RefCell::new(Vec::with_capacity(MAX_FREE))),
        }
    }

    pub(super) fn take(&self, results: usize) -> Lease {
        assert!(results <= birnam_codegen::numeric_chain::MAX_OPERATIONS);
        let mut storage = self.free.borrow_mut().pop().unwrap_or_default();
        debug_assert!(storage.values.identities.is_empty());
        debug_assert!(storage.values.materialized.is_empty());
        storage.values.identities.reserve_exact(results);
        storage.values.materialized.reserve_exact(results);
        storage.values.materialized.resize(results, None);
        Lease {
            storage,
            free: self.free.clone(),
        }
    }

    /// Pool header, free storage slots and retained vector capacity. Excludes
    /// Rc/allocator bookkeeping; free slots retain no snapshot owners or Oops.
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(in crate::runtime::native_tier) fn resources(&self) -> (usize, usize) {
        let free = self.free.borrow();
        let bytes = core::mem::size_of::<RefCell<Vec<Storage>>>()
            + free.capacity() * core::mem::size_of::<Storage>()
            + free
                .iter()
                .map(|s| {
                    s.values.identities.capacity() * core::mem::size_of::<DeferredAllocation>()
                        + s.values.materialized.capacity() * core::mem::size_of::<Option<Oop>>()
                })
                .sum::<usize>();
        (free.len(), bytes)
    }
}

pub(super) struct Lease {
    pub storage: Storage,
    free: Rc<RefCell<Vec<Storage>>>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        // These are host allocations and raw carriers. Clearing invokes no Id
        // callback or collection and leaves journal-owned snapshots intact.
        self.storage.values.identities.clear();
        self.storage.values.materialized.clear();
        let mut free = self.free.borrow_mut();
        if free.len() < MAX_FREE {
            free.push(core::mem::take(&mut self.storage));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_is_untraced_cleared_and_bounded_across_leases() {
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        let pool = Pool::new();
        let mut leases = (0..MAX_FREE + 2).map(|_| pool.take(32)).collect::<Vec<_>>();
        let object = r.make_integer(743211);
        for lease in &mut leases {
            lease.storage.values.materialized[0] = Some(object);
        }
        r.enter(|| r.gc_collect_from_native_range(0, 0));
        assert!(unsafe { (*r.state.get()).heap.vtable_of(object) }.is_none());
        drop(leases);
        let (cached, bytes) = pool.resources();
        assert_eq!(cached, MAX_FREE);
        assert!(bytes < 8192);
        let lease = pool.take(3);
        assert!(lease.storage.values.identities.is_empty());
        assert_eq!(lease.storage.values.materialized, vec![None; 3]);
        drop(pool);
        drop(lease); // The lease owns its return destination independently.

        let pool = Pool::new();
        for results in [3, 20, 32, 2] {
            let lease = pool.take(results);
            assert!(lease.storage.values.identities.capacity() <= 32);
            assert!(lease.storage.values.materialized.capacity() <= 32);
        }
        assert_eq!(pool.resources().0, 1);
    }
}
