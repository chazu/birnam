//! Exact multiplicities for already-dead canonical Frame slot-index Integers.
//! No identity is exposed, and all value bytes live in untraced host storage.
use super::*;

pub(crate) const MAX_FRAME_INDEX: usize = 128;
pub(crate) const MAX_COUNTED_FRAME_INDICES: usize = 1_048_576;

pub(in crate::heap) struct CountedFrameIndices {
    batch: std::rc::Rc<super::numeric::Batch>,
    total: usize,
    counts: [usize; MAX_FRAME_INDEX],
}

impl Heap {
    /// The caller proves these are canonical raw Integer indices whose
    /// identities are already dead, retains their vtable, and checks the full
    /// original collection budget before entering the no-observation region.
    pub(crate) fn record_dead_payload_indices(
        &mut self,
        batch: std::rc::Rc<super::numeric::Batch>,
        indices: &[u32],
    ) -> bool {
        if indices.is_empty() {
            return true;
        }
        let size = batch.total_size;
        let previous = self.counted_frame_index_count();
        let Some(bytes) = indices.len().checked_mul(size) else {
            return false;
        };
        let Some(charged) = self.budget.preview(bytes) else {
            return false;
        };
        if !self.contains(core::ptr::with_exposed_provenance_mut(batch.vtable))
            || batch.count() != MAX_FRAME_INDEX
            || indices.len() > MAX_COUNTED_FRAME_INDICES.saturating_sub(previous)
            || indices
                .iter()
                .any(|&index| index as usize >= MAX_FRAME_INDEX)
            || self.counted_frame_indices.as_ref().is_some_and(|ledger| {
                ledger.total != 0 && !std::rc::Rc::ptr_eq(&ledger.batch, &batch)
            })
        {
            return false;
        }
        // Every refusal precedes allocation, charging and counter mutation.
        let ledger = self.counted_frame_indices.get_or_insert_with(|| {
            Box::new(CountedFrameIndices {
                batch: batch.clone(),
                total: 0,
                counts: [0; MAX_FRAME_INDEX],
            })
        });
        ledger.batch = batch;
        for &index in indices {
            ledger.counts[index as usize] += 1;
        }
        ledger.total += indices.len();
        self.budget.replace(charged);
        true
    }

    #[cfg(test)]
    pub(crate) fn record_dead_frame_indices(&mut self, vtable: Oop, indices: &[u32]) -> bool {
        if indices.is_empty() {
            return true;
        }
        let batch = match self
            .counted_frame_indices
            .as_ref()
            .filter(|l| l.batch.vtable == vtable.addr())
        {
            Some(ledger) => ledger.batch.clone(),
            None => {
                let mut batch = super::numeric::Batch::zeroed(vtable, 8, MAX_FRAME_INDEX).unwrap();
                for i in 0..MAX_FRAME_INDEX {
                    batch
                        .payload_mut(i)
                        .copy_from_slice(&(i as i64).to_ne_bytes());
                }
                std::rc::Rc::new(batch)
            }
        };
        self.record_dead_payload_indices(batch, indices)
    }

    pub(crate) fn retained_payload_indices(
        &self,
        vtable: Oop,
        payload_size: usize,
    ) -> Option<std::rc::Rc<super::numeric::Batch>> {
        self.counted_frame_indices
            .as_ref()
            .filter(|ledger| {
                ledger.batch.vtable == vtable.addr() && ledger.batch.payload_size == payload_size
            })
            .map(|ledger| ledger.batch.clone())
    }

    pub(crate) fn counted_frame_index_count(&self) -> usize {
        self.counted_frame_indices
            .as_ref()
            .map_or(0, |ledger| ledger.total)
    }

    pub(in crate::heap) fn counted_frame_index_bytes(&self) -> usize {
        self.counted_frame_indices
            .as_ref()
            .map_or(0, |ledger| ledger.total * ledger.batch.total_size)
    }

    pub(super) fn settle_dead_frame_indices(&mut self) {
        let Some(ledger) = self.counted_frame_indices.take() else {
            return;
        };
        for (index, &count) in ledger.counts.iter().enumerate() {
            for _ in 0..count {
                self.materialize_raw(ledger.batch.vtable, ledger.batch.payload(index));
            }
        }
    }

    pub(super) fn clear_dead_frame_indices(&mut self) {
        if let Some(ledger) = &mut self.counted_frame_indices {
            ledger.counts.fill(0);
            ledger.total = 0;
        }
    }

    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(super) fn counted_frame_index_storage_bytes(&self) -> usize {
        self.counted_frame_indices.as_ref().map_or(0, |ledger| {
            size_of::<CountedFrameIndices>()
                + size_of::<super::numeric::Batch>()
                + 2 * size_of::<usize>()
                + ledger.batch.storage_bytes()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vtable(heap: &mut Heap) -> Oop {
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        vt
    }

    fn counts(heap: &Heap) -> (usize, usize, usize, usize) {
        (
            heap.allocation_count(),
            heap.physical_allocation_count(),
            heap.allocated_bytes(),
            heap.bytes_since_collection(),
        )
    }

    #[test]
    fn counted_indices_preserve_logical_charges_with_bounded_storage() {
        let mut heap = Heap::new();
        let vt = vtable(&mut heap);
        let before = counts(&heap);
        assert_eq!(heap.counted_frame_index_storage_bytes(), 0);
        let indices = (0..MAX_FRAME_INDEX as u32).collect::<Vec<_>>();
        for _ in 0..128 {
            assert!(heap.record_dead_frame_indices(vt, &indices));
            assert!(heap.counted_frame_index_storage_bytes() < 4096);
        }
        let count = 128 * MAX_FRAME_INDEX;
        let size = Heap::allocation_total_size(8).unwrap();
        assert_eq!(
            counts(&heap),
            (
                before.0 + count,
                before.1,
                before.2 + count * size,
                before.3 + count * size
            )
        );
        // Counted multiplicities do not consume individually stored identity
        // entries. Their own bounded ledger cannot extend the old entry limit.
        let batch = heap.prepare_numeric_batch(vt, vec![7]).unwrap();
        for _ in 0..MAX_DEAD_ALLOCATIONS {
            assert!(heap.record_numeric_result(batch.clone(), 0).is_some());
        }
        assert!(!heap.admits_virtual_allocations(1));
        assert_eq!(
            heap.reclaim_dead_allocations(),
            count + MAX_DEAD_ALLOCATIONS
        );
        assert_eq!(heap.allocation_count(), before.0);
        assert_eq!(heap.allocated_bytes(), before.2);
        assert!(heap.counted_frame_index_storage_bytes() < 4096);
        assert!(!heap.has_dead_allocations());
        assert!(heap.record_dead_frame_indices(vt, &[127]));
        assert_eq!(heap.reclaim_dead_allocations(), 1);
    }

    #[test]
    fn settlement_preserves_exact_values_fresh_objects_and_mixed_journals() {
        let mut heap = Heap::new();
        let vt = vtable(&mut heap);
        let prototype = heap.allocate(8, Permanence::Collectable);
        heap.set_vtable(prototype, vt);
        heap.write_payload_word(prototype, 0, 77);
        let original: std::collections::HashSet<_> = heap.blocks.keys().copied().collect();
        let clone = heap.prepare_dead_clone(prototype).unwrap();
        let clone_id = heap.record_virtual_clone(clone).unwrap();
        let batch = heap.prepare_numeric_batch(vt, vec![17]).unwrap();
        let first = heap.record_numeric_result(batch.clone(), 0).unwrap();
        let second = heap.record_numeric_result(batch, 0).unwrap();
        assert!(heap.record_dead_frame_indices(vt, &[0, 1, 127, 1]));
        let before = counts(&heap);
        let realized = heap.materialize_recorded_clone(first).unwrap();
        heap.settle_dead_allocations();
        assert_eq!(counts(&heap), (before.0, before.0, before.2, before.3));
        assert!(!heap.has_dead_allocations());
        assert!(heap.materialize_recorded_clone(first).is_none());
        assert!(heap.materialize_recorded_clone(second).is_none());
        assert!(heap.materialize_recorded_clone(clone_id).is_none());
        let fresh = heap
            .blocks
            .keys()
            .filter(|p| !original.contains(p))
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(fresh.len(), 7);
        assert!(fresh.contains(&realized.addr()));
        let mut values = fresh
            .iter()
            .map(|&address| {
                let oop = core::ptr::with_exposed_provenance_mut(address);
                assert_eq!(heap.vtable_of(oop), Some(vt));
                heap.read_payload_word(oop, 0).unwrap()
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, [0, 1, 1, 17, 17, 77, 127]);
        assert_eq!(heap.reclaim_dead_allocations(), 0);
    }

    #[test]
    fn counted_index_refusal_is_atomic_and_empty_ledgers_can_change_vtable() {
        let mut heap = Heap::new();
        let first = vtable(&mut heap);
        let second = vtable(&mut heap);
        let before = counts(&heap);
        assert!(heap.record_dead_frame_indices(first, &[]));
        assert!(!heap.record_dead_frame_indices(core::ptr::null_mut(), &[1]));
        assert!(!heap.record_dead_frame_indices(first, &[1, MAX_FRAME_INDEX as u32]));
        assert_eq!(counts(&heap), before);
        assert_eq!(heap.counted_frame_index_storage_bytes(), 0);
        assert!(heap.record_dead_frame_indices(first, &[1]));
        let recorded = counts(&heap);
        assert!(!heap.record_dead_frame_indices(second, &[2]));
        assert_eq!(counts(&heap), recorded);
        assert_eq!(heap.reclaim_dead_allocations(), 1);
        assert!(heap.record_dead_frame_indices(second, &[2]));
        heap.settle_dead_allocations();
        assert_eq!(heap.physical_allocation_count(), before.1 + 1);
        assert_eq!(heap.counted_frame_index_storage_bytes(), 0);
        assert!(heap.record_dead_frame_indices(first, &vec![7; MAX_COUNTED_FRAME_INDICES]));
        let full = counts(&heap);
        assert!(!heap.record_dead_frame_indices(first, &[0]));
        assert_eq!(counts(&heap), full);
        assert_eq!(heap.reclaim_dead_allocations(), MAX_COUNTED_FRAME_INDICES);
        heap.budget.replace(usize::MAX);
        assert!(!heap.record_dead_frame_indices(first, &[0]));
        assert_eq!(heap.counted_frame_index_count(), 0);
        assert_eq!(heap.bytes_since_collection(), usize::MAX);
    }
}
