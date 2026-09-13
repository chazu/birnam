//! Shared raw storage, with a separate charged identity for each numeric result.
use super::*;
use std::rc::Rc;

pub(crate) struct Batch {
    vtable: usize,
    values: Vec<i64>,
    total_size: usize,
}

pub(crate) struct Entry {
    identity: DeferredAllocation,
    batch: Rc<Batch>,
    index: usize,
}

impl Heap {
    /// The caller establishes native Integer storage authority and excludes
    /// observation while results remain virtual. Values already live in an
    /// untraced host allocation; no populated native-stack array is introduced.
    pub(crate) fn prepare_numeric_batch(&self, vtable: Oop, values: Vec<i64>) -> Option<Rc<Batch>> {
        if values.is_empty()
            || values.capacity() > MAX_DEAD_ALLOCATIONS
            || !self.admits_virtual_allocations(values.len())
        {
            return None;
        }
        Some(Rc::new(Batch {
            vtable: vtable.addr(),
            values,
            total_size: Self::allocation_total_size(core::mem::size_of::<i64>())?,
        }))
    }

    /// Preparation charges nothing. Each executed Call charges once here,
    /// including separate fresh identities for equal values or repeated indices.
    pub(crate) fn record_numeric_result(
        &mut self,
        batch: Rc<Batch>,
        index: usize,
    ) -> Option<DeferredAllocation> {
        if index >= batch.values.len() || !self.admits_virtual_allocations(1) {
            return None;
        }
        let next = self.next_deferred_id.checked_add(1)?;
        self.bytes_since_collection = self
            .bytes_since_collection
            .checked_add(batch.total_size)
            .unwrap_or_else(|| boundary::fatal_invariant("GC allocation byte counter overflow"));
        let identity = DeferredAllocation(self.next_deferred_id);
        self.next_deferred_id = next;
        self.numeric_allocations.push(Entry {
            identity,
            batch,
            index,
        });
        Some(identity)
    }

    pub(in crate::heap) fn dead_allocation_count(&self) -> usize {
        self.dead_allocations.len() + self.numeric_allocations.len()
    }

    pub(in crate::heap) fn numeric_allocated_bytes(&self) -> usize {
        self.numeric_allocations
            .iter()
            .fold(0usize, |total, entry| {
                total.saturating_add(entry.batch.total_size)
            })
    }

    pub(super) fn materialize_numeric(&mut self, identity: DeferredAllocation) -> Option<Oop> {
        let position = self
            .numeric_allocations
            .iter()
            .position(|entry| entry.identity == identity)?;
        let entry = self.numeric_allocations.swap_remove(position);
        Some(self.materialize_numeric_entry(&entry))
    }

    pub(super) fn materialize_numeric_entry(&mut self, entry: &Entry) -> Oop {
        // Borrow the original bytes directly; copying into a stack buffer could
        // make pointer-shaped Integer bits conservative roots on a later GC.
        let value = &entry.batch.values[entry.index];
        let bytes = unsafe {
            core::slice::from_raw_parts(
                core::ptr::from_ref(value).cast::<u8>(),
                core::mem::size_of::<i64>(),
            )
        };
        self.materialize_raw(entry.batch.vtable, bytes)
    }

    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(super) fn numeric_allocation_storage_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        self.numeric_allocations.capacity() * core::mem::size_of::<Entry>()
            + self
                .numeric_allocations
                .iter()
                .filter(|entry| seen.insert(Rc::as_ptr(&entry.batch)))
                .map(|entry| {
                    core::mem::size_of::<Batch>()
                        + 2 * core::mem::size_of::<usize>()
                        + entry.batch.values.capacity() * core::mem::size_of::<i64>()
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(heap: &Heap) -> (usize, usize, usize) {
        (
            heap.allocation_count(),
            heap.allocated_bytes(),
            heap.bytes_since_collection(),
        )
    }

    #[test]
    fn mixed_journals_preserve_fresh_identity_bits_and_partial_ownership() {
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        let before = counts(&heap);
        let values = vec![i64::MIN, -1, 0, 1, i64::MAX, 1];
        let batch = heap.prepare_numeric_batch(vt, values.clone()).unwrap();
        let weak = Rc::downgrade(&batch);
        assert_eq!(counts(&heap), before);
        let snapshot = heap.prepare_dead_clone(vt).unwrap();
        let old_a = heap.record_virtual_clone(snapshot.clone()).unwrap();
        let ids = (0..values.len())
            .map(|index| heap.record_numeric_result(batch.clone(), index).unwrap())
            .collect::<Vec<_>>();
        let old_b = heap.record_virtual_clone(snapshot.clone()).unwrap();
        assert!(heap.record_dead_allocation(snapshot));
        let duplicate = heap.record_numeric_result(batch.clone(), 3).unwrap();
        let charged = counts(&heap);
        assert_eq!(charged.0, before.0 + 10);
        assert_eq!(
            heap.numeric_allocation_storage_bytes(),
            heap.numeric_allocations.capacity() * core::mem::size_of::<Entry>()
                + core::mem::size_of::<Batch>()
                + 2 * core::mem::size_of::<usize>()
                + batch.values.capacity() * 8
        );
        drop(batch);
        let a = heap.materialize_recorded_clone(old_b).unwrap();
        let mut objects = Vec::new();
        for index in [5, 0, 3, 2, 4, 1] {
            let object = heap.materialize_recorded_clone(ids[index]).unwrap();
            assert_eq!(heap.vtable_of(object), Some(vt));
            assert_eq!(
                heap.read_payload_word(object, 0),
                Some(values[index] as usize)
            );
            assert!(!objects.contains(&object));
            objects.push(object);
            assert!(heap.materialize_recorded_clone(ids[index]).is_none());
            assert_eq!(counts(&heap), charged);
        }
        assert!(weak.upgrade().is_some());
        let object = heap.materialize_recorded_clone(duplicate).unwrap();
        assert!(!objects.contains(&object));
        assert_eq!(heap.read_payload_word(object, 0), Some(1));
        assert!(weak.upgrade().is_none());
        assert_ne!(heap.materialize_recorded_clone(old_a).unwrap(), a);
        assert_eq!(counts(&heap), charged);
        assert_eq!(heap.reclaim_dead_allocations(), 1);
    }

    #[test]
    fn both_journals_share_admission_and_ids_remain_stale_after_collection() {
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        assert_eq!(heap.numeric_allocation_storage_bytes(), 0);
        assert!(heap.prepare_numeric_batch(vt, Vec::new()).is_none());
        assert!(
            heap.prepare_numeric_batch(vt, vec![0; MAX_DEAD_ALLOCATIONS + 1])
                .is_none()
        );
        let batch = heap.prepare_numeric_batch(vt, vec![7]).unwrap();
        let snapshot = heap.prepare_dead_clone(vt).unwrap();
        let first = heap.record_numeric_result(batch.clone(), 0).unwrap();
        for index in 1..MAX_DEAD_ALLOCATIONS {
            if index % 2 == 0 {
                heap.record_numeric_result(batch.clone(), 0).unwrap();
            } else {
                assert!(heap.record_dead_allocation(snapshot.clone()));
            }
        }
        let full = counts(&heap);
        assert!(!heap.admits_virtual_allocations(1));
        assert!(heap.prepare_dead_clone(vt).is_none());
        assert!(heap.prepare_virtual_payload(vt, &[vt]).is_none());
        assert!(heap.record_numeric_result(batch.clone(), 0).is_none());
        assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
        assert_eq!(counts(&heap), full);
        heap.materialize_recorded_clone(first).unwrap();
        assert!(heap.record_numeric_result(batch.clone(), 1).is_none());
        let stale = heap.record_numeric_result(batch.clone(), 0).unwrap();
        assert_eq!(heap.reclaim_dead_allocations(), MAX_DEAD_ALLOCATIONS);
        let current = heap.record_numeric_result(batch.clone(), 0).unwrap();
        assert_ne!(stale, current);
        assert!(heap.materialize_recorded_clone(stale).is_none());
        let charged = counts(&heap);
        heap.settle_dead_allocations();
        assert_eq!(counts(&heap), charged);
        assert!(heap.materialize_recorded_clone(current).is_none());
        heap.next_deferred_id = u64::MAX;
        assert!(heap.record_numeric_result(batch, 0).is_none());
        assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
        assert_eq!(counts(&heap), charged);
        assert!(heap.record_dead_allocation(snapshot));
    }

    #[test]
    fn settlement_realizes_only_charged_elements_without_double_charging() {
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        let batch = heap.prepare_numeric_batch(vt, vec![41, 999, -5]).unwrap();
        let weak = Rc::downgrade(&batch);
        let size = batch.total_size;
        heap.bytes_since_collection = usize::MAX - 2 * size;
        let first = heap.record_numeric_result(batch.clone(), 2).unwrap();
        heap.record_numeric_result(batch, 0).unwrap();
        let before = counts(&heap);
        assert_eq!(before.2, usize::MAX);
        heap.settle_dead_allocations();
        assert_eq!(counts(&heap), before);
        assert!(weak.upgrade().is_none());
        assert_eq!(heap.physical_allocation_count(), before.0);
        let mut values = heap
            .blocks
            .keys()
            .filter(|&&p| p != vt.addr())
            .map(|&p| {
                heap.read_payload_word(core::ptr::with_exposed_provenance_mut(p), 0)
                    .unwrap() as i64
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, vec![-5, 41]);
        assert!(heap.materialize_recorded_clone(first).is_none());
    }
}
