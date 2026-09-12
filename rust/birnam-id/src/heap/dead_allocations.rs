//! Accounting for allocations whose unexposed identity is proven dead.
//! No Oop is published for a journal entry; payload bits are intentionally not roots.
use super::*;

pub(crate) const MAX_DEAD_ALLOCATIONS: usize = 4096;
pub(crate) const MAX_DEAD_PAYLOAD_BYTES: usize = 1024;

pub(crate) struct DeadAllocation {
    vtable: usize,
    payload: Vec<u8>,
    total_size: usize,
}
impl DeadAllocation {
    pub(crate) fn total_size(&self) -> usize {
        self.total_size
    }
}

impl Heap {
    /// Preparation has no logical allocation effect. The caller must prove
    /// non-escape and absence of observation/collection through Frame retirement.
    pub(crate) fn prepare_dead_clone(&self, prototype: Oop) -> Option<std::rc::Rc<DeadAllocation>> {
        if self.dead_allocations.len() >= MAX_DEAD_ALLOCATIONS {
            return None;
        }
        let block = self.blocks.get(&prototype.addr())?;
        let payload_size = block.total_size - VTABLE_HEADER_SIZE;
        if payload_size > MAX_DEAD_PAYLOAD_BYTES {
            return None;
        }
        let vtable = self.vtable_of(prototype)?.addr();
        // Same padded bytes as clone_collectable, held in untraced host storage.
        let payload =
            unsafe { core::slice::from_raw_parts(prototype.cast::<u8>(), payload_size) }.to_vec();
        Some(std::rc::Rc::new(DeadAllocation {
            vtable,
            payload,
            total_size: block.total_size,
        }))
    }

    /// Charge the original allocation exactly once at its original position.
    pub(crate) fn record_dead_allocation(
        &mut self,
        allocation: std::rc::Rc<DeadAllocation>,
    ) -> bool {
        if self.dead_allocations.len() >= MAX_DEAD_ALLOCATIONS {
            return false;
        }
        self.bytes_since_collection = self
            .bytes_since_collection
            .checked_add(allocation.total_size)
            .unwrap_or_else(|| boundary::fatal_invariant("GC allocation byte counter overflow"));
        self.dead_allocations.push(allocation);
        true
    }

    pub(crate) fn has_dead_allocations(&self) -> bool {
        !self.dead_allocations.is_empty()
    }

    pub(crate) fn admits_dead_allocations(&self, count: usize) -> bool {
        count <= MAX_DEAD_ALLOCATIONS.saturating_sub(self.dead_allocations.len())
    }

    /// Realize already-charged garbage before an observing sweep. This is not
    /// a new Id allocation: no callback runs and all accounting is unchanged.
    /// The caller must be between marking and the dead-object snapshot.
    pub(crate) fn settle_dead_allocations(&mut self) {
        let allocations = core::mem::take(&mut self.dead_allocations);
        let allocation_bytes = self.bytes_since_collection;
        // These blocks were charged already. Use a bounded scratch counter so
        // physical settlement cannot overflow by temporarily charging twice.
        // No callback can observe it before the original counter is restored.
        self.bytes_since_collection = 0;
        for allocation in allocations {
            let object = self.allocate(allocation.payload.len(), Permanence::Collectable);
            boundary::require_invariant(
                self.set_vtable(
                    object,
                    core::ptr::with_exposed_provenance_mut(allocation.vtable),
                ),
                "settled dead allocation missing from heap",
            );
            // The new exact-size payload cannot alias the owned snapshot.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    allocation.payload.as_ptr(),
                    object.cast::<u8>(),
                    allocation.payload.len(),
                )
            };
        }
        self.bytes_since_collection = allocation_bytes;
    }

    /// Use only after proving that the sweep cannot observe these missing
    /// physical blocks. The returned count contributes to ordinary GC results.
    pub(crate) fn reclaim_dead_allocations(&mut self) -> usize {
        let count = self.dead_allocations.len();
        self.dead_allocations.clear();
        count
    }

    pub(crate) fn dead_finalizers(&self) -> impl Iterator<Item = (Oop, Finalizer)> + '_ {
        self.blocks.iter().filter_map(|(address, block)| {
            if block.permanence == Permanence::Collectable && !block.marked {
                block
                    .finalizer
                    .map(|finalizer| (core::ptr::with_exposed_provenance_mut(*address), finalizer))
            } else {
                None
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn physical_allocation_count(&self) -> usize {
        self.blocks.len()
    }

    /// Owned journal storage, excluding allocator bookkeeping. Shared snapshots
    /// count once; retained vector capacity counts even after reclamation.
    #[cfg(test)]
    pub(crate) fn dead_allocation_storage_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        self.dead_allocations.capacity() * core::mem::size_of::<std::rc::Rc<DeadAllocation>>()
            + self
                .dead_allocations
                .iter()
                .filter(|allocation| seen.insert(std::rc::Rc::as_ptr(allocation)))
                .map(|allocation| {
                    core::mem::size_of::<DeadAllocation>()
                        + 2 * core::mem::size_of::<usize>()
                        + allocation.payload.capacity()
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dead_clone_accounting_and_settlement_preserve_exact_saved_bytes() {
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        let prototype = heap.allocate(17, Permanence::Collectable);
        heap.set_vtable(prototype, vt);
        for i in 0..3 {
            assert!(heap.write_payload_word(prototype, i, 71 + i));
        }
        let before = (
            heap.allocation_count(),
            heap.allocated_bytes(),
            heap.bytes_since_collection(),
        );
        let allocation = heap.prepare_dead_clone(prototype).unwrap();
        let size = allocation.total_size();
        assert_eq!(
            (
                heap.allocation_count(),
                heap.allocated_bytes(),
                heap.bytes_since_collection()
            ),
            before
        );
        assert!(heap.record_dead_allocation(allocation));
        let expected = (before.0 + 1, before.1 + size, before.2 + size);
        assert_eq!(
            (
                heap.allocation_count(),
                heap.allocated_bytes(),
                heap.bytes_since_collection()
            ),
            expected
        );
        assert_eq!(heap.physical_allocation_count(), before.0);
        // Settlement must use the original payload and vtable, not reread a
        // prototype that ordinary code has since mutated.
        heap.write_payload_word(prototype, 0, 999);
        heap.settle_dead_allocations();
        assert_eq!(
            (
                heap.allocation_count(),
                heap.allocated_bytes(),
                heap.bytes_since_collection()
            ),
            expected
        );
        assert_eq!(heap.physical_allocation_count(), expected.0);
        let copy = heap
            .blocks
            .keys()
            .copied()
            .find(|p| *p != prototype.addr() && *p != vt.addr())
            .unwrap();
        let copy = core::ptr::with_exposed_provenance_mut(copy);
        assert_eq!(heap.vtable_of(copy), Some(vt));
        for i in 0..3 {
            assert_eq!(heap.read_payload_word(copy, i), Some(71 + i));
        }
        assert_eq!(heap.has_finalizer(copy), Some(false));
    }

    #[test]
    fn dead_clone_records_are_bounded_and_reclaim_without_physical_blocks() {
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        for _ in 0..MAX_DEAD_ALLOCATIONS {
            let allocation = heap.prepare_dead_clone(vt).unwrap();
            assert!(heap.record_dead_allocation(allocation));
        }
        assert!(heap.prepare_dead_clone(vt).is_none());
        assert_eq!(heap.physical_allocation_count(), 1);
        assert_eq!(heap.allocation_count(), MAX_DEAD_ALLOCATIONS + 1);
        assert_eq!(heap.reclaim_dead_allocations(), MAX_DEAD_ALLOCATIONS);
        assert_eq!(heap.allocation_count(), 1);
        assert!(heap.prepare_dead_clone(vt).is_some());
        let large = heap.allocate(MAX_DEAD_PAYLOAD_BYTES + 1, Permanence::Collectable);
        heap.set_vtable(large, vt);
        assert!(heap.prepare_dead_clone(large).is_none());
    }

    #[test]
    fn physical_settlement_never_double_charges_the_allocation_counter() {
        let mut heap = Heap::new();
        let prototype = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(prototype, prototype);
        let snapshot = heap.prepare_dead_clone(prototype).unwrap();
        heap.bytes_since_collection = usize::MAX - snapshot.total_size();
        assert!(heap.record_dead_allocation(snapshot));
        assert_eq!(heap.bytes_since_collection(), usize::MAX);
        heap.settle_dead_allocations();
        assert_eq!(heap.bytes_since_collection(), usize::MAX);
        assert_eq!(heap.physical_allocation_count(), 2);
    }
}
