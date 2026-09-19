//! Accounting for allocations whose unexposed identity is proven dead.
//! No Oop is published for a journal entry; payload bits are intentionally not roots.
use super::*;
pub(super) mod frame_indices;
pub(super) mod numeric;

/// A process-local, single-use identity for an already charged allocation.
/// Never an Oop; collection or materialization makes the capability stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeferredAllocation(u64);

pub(crate) const MAX_DEAD_ALLOCATIONS: usize = 4096;
pub(crate) const MAX_DEAD_PAYLOAD_BYTES: usize = 1024;

#[cfg(any(test, feature = "ffi-test-hooks"))]
thread_local! {
    static SETTLED_ALLOCATIONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

// The enum uses Vec's niche without increasing the snapshot header. Small
// bytes live inside the untraced Rc allocation; larger payloads keep their Vec.
const INLINE_PAYLOAD_BYTES: usize = core::mem::size_of::<usize>();
enum Payload {
    Inline([u8; INLINE_PAYLOAD_BYTES]),
    Heap(Vec<u8>),
}

pub(crate) struct DeadAllocation {
    vtable: usize,
    payload: Payload,
    total_size: usize,
}
impl DeadAllocation {
    pub(crate) fn total_size(&self) -> usize {
        self.total_size
    }

    fn zeroed(vtable: usize, total_size: usize) -> std::rc::Rc<Self> {
        let padded = total_size - VTABLE_HEADER_SIZE;
        std::rc::Rc::new(Self {
            vtable,
            payload: if padded <= INLINE_PAYLOAD_BYTES {
                // Allocate before copying potentially pointer-shaped bytes.
                Payload::Inline([0; INLINE_PAYLOAD_BYTES])
            } else {
                Payload::Heap(vec![0; padded])
            },
            total_size,
        })
    }

    fn payload(&self) -> &[u8] {
        match &self.payload {
            Payload::Inline(bytes) => &bytes[..self.total_size - VTABLE_HEADER_SIZE],
            Payload::Heap(bytes) => bytes,
        }
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        match &mut self.payload {
            Payload::Inline(bytes) => &mut bytes[..self.total_size - VTABLE_HEADER_SIZE],
            Payload::Heap(bytes) => bytes,
        }
    }

    #[cfg(any(test, feature = "ffi-test-hooks"))]
    fn separate_payload_capacity(&self) -> usize {
        match &self.payload {
            Payload::Inline(_) => 0,
            Payload::Heap(bytes) => bytes.capacity(),
        }
    }
}

impl Heap {
    /// Exact raw payload bits, held outside the traced heap until realization.
    /// The caller proves current constructor/storage authority and excludes
    /// observation while any live identity remains virtual.
    #[cfg(test)]
    pub(crate) fn prepare_virtual_bytes(
        &self,
        vtable: Oop,
        bytes: &[u8],
    ) -> Option<std::rc::Rc<DeadAllocation>> {
        if !self.admits_virtual_allocations(1) {
            return None;
        }
        let total_size = Self::allocation_total_size(bytes.len())?;
        let padded = total_size.checked_sub(VTABLE_HEADER_SIZE)?;
        if padded > MAX_DEAD_PAYLOAD_BYTES {
            return None;
        }
        let mut allocation = DeadAllocation::zeroed(vtable.addr(), total_size);
        std::rc::Rc::get_mut(&mut allocation).unwrap().payload_mut()[..bytes.len()]
            .copy_from_slice(bytes);
        Some(allocation)
    }

    /// Prepare an exact fresh all-Oop payload in untraced host storage. The
    /// caller supplies constructor authority and proves layout/GC eligibility.
    pub(crate) fn prepare_virtual_payload(
        &self,
        vtable: Oop,
        words: &[Oop],
    ) -> Option<std::rc::Rc<DeadAllocation>> {
        if !self.admits_dead_allocations(1) {
            return None;
        }
        let bytes = words.len().checked_mul(core::mem::size_of::<Oop>())?;
        let total_size = Self::allocation_total_size(bytes)?;
        let padded = total_size.checked_sub(VTABLE_HEADER_SIZE)?;
        if padded > MAX_DEAD_PAYLOAD_BYTES {
            return None;
        }
        // Ordinary allocation zero-initializes padding before the constructor
        // stores its words. Equal payloads still receive distinct journal IDs.
        let mut allocation = DeadAllocation::zeroed(vtable.addr(), total_size);
        for (destination, word) in std::rc::Rc::get_mut(&mut allocation)
            .unwrap()
            .payload_mut()
            .chunks_exact_mut(core::mem::size_of::<Oop>())
            .zip(words)
        {
            destination.copy_from_slice(&(*word as usize).to_ne_bytes());
        }
        Some(allocation)
    }

    /// Preparation has no logical allocation effect. The caller must prove
    /// non-escape and absence of observation/collection through Frame retirement.
    pub(crate) fn prepare_dead_clone(&self, prototype: Oop) -> Option<std::rc::Rc<DeadAllocation>> {
        if !self.admits_dead_allocations(1) {
            return None;
        }
        let block = self.blocks.get(&prototype.addr())?;
        let payload_size = block.total_size - VTABLE_HEADER_SIZE;
        if payload_size > MAX_DEAD_PAYLOAD_BYTES {
            return None;
        }
        let vtable = self.vtable_of(prototype)?.addr();
        // Same padded bytes as clone_collectable, held in untraced host storage.
        let bytes = unsafe { core::slice::from_raw_parts(prototype.cast::<u8>(), payload_size) };
        if payload_size <= INLINE_PAYLOAD_BYTES {
            let mut allocation = DeadAllocation::zeroed(vtable, block.total_size);
            std::rc::Rc::get_mut(&mut allocation)
                .unwrap()
                .payload_mut()
                .copy_from_slice(bytes);
            Some(allocation)
        } else {
            Some(std::rc::Rc::new(DeadAllocation {
                vtable,
                payload: Payload::Heap(bytes.to_vec()),
                total_size: block.total_size,
            }))
        }
    }

    /// Charge the original allocation exactly once at its original position.
    pub(crate) fn record_dead_allocation(
        &mut self,
        allocation: std::rc::Rc<DeadAllocation>,
    ) -> bool {
        if !self.admits_dead_allocations(1) {
            return false;
        }
        self.bytes_since_collection = self
            .bytes_since_collection
            .checked_add(allocation.total_size)
            .unwrap_or_else(|| boundary::fatal_invariant("GC allocation byte counter overflow"));
        self.dead_allocations.push(allocation);
        true
    }

    /// Charge a private virtual clone at its original allocation position.
    /// Until it is materialized or proven dead, the caller must exclude every
    /// observation/collection and retain the payload's input identities.
    pub(crate) fn record_virtual_clone(
        &mut self,
        allocation: std::rc::Rc<DeadAllocation>,
    ) -> Option<DeferredAllocation> {
        let next = self.next_deferred_id.checked_add(1)?;
        let index = self.dead_allocations.len();
        if !self.record_dead_allocation(allocation) {
            return None;
        }
        let identity = DeferredAllocation(self.next_deferred_id);
        self.next_deferred_id = next;
        self.deferred_allocations.push((identity, index));
        Some(identity)
    }

    /// Publish physical storage for one already charged identity. No callback
    /// or collection runs here; the caller roots the answer before exposure.
    pub(crate) fn materialize_recorded_clone(
        &mut self,
        identity: DeferredAllocation,
    ) -> Option<Oop> {
        let Some(position) = self
            .deferred_allocations
            .iter()
            .position(|(key, _)| *key == identity)
        else {
            return self.materialize_numeric(identity);
        };
        let (_, index) = self.deferred_allocations.swap_remove(position);
        let displaced = self.dead_allocations.len() - 1;
        let snapshot = self.dead_allocations.swap_remove(index);
        for (_, recorded_index) in &mut self.deferred_allocations {
            if *recorded_index == displaced {
                *recorded_index = index;
            }
        }
        Some(self.materialize_snapshot(&snapshot))
    }

    fn materialize_snapshot(&mut self, allocation: &DeadAllocation) -> Oop {
        self.materialize_raw(allocation.vtable, allocation.payload())
    }

    fn materialize_raw(&mut self, vtable: usize, payload: &[u8]) -> Oop {
        let allocation_bytes = self.bytes_since_collection;
        // Construction realizes a previously charged allocation; it must not
        // overflow by charging that allocation for a second time.
        self.bytes_since_collection = 0;
        let object = self.allocate(payload.len(), Permanence::Collectable);
        boundary::require_invariant(
            self.set_vtable(object, core::ptr::with_exposed_provenance_mut(vtable)),
            "materialized allocation missing from heap",
        );
        unsafe {
            core::ptr::copy_nonoverlapping(payload.as_ptr(), object.cast::<u8>(), payload.len())
        };
        self.bytes_since_collection = allocation_bytes;
        object
    }

    pub(crate) fn has_dead_allocations(&self) -> bool {
        self.dead_allocation_count() != 0
    }

    pub(crate) fn admits_dead_allocations(&self, count: usize) -> bool {
        // This bound covers individually retained journal entries. Counted,
        // already-dead frame indices have a separate fixed-storage bound.
        count
            <= MAX_DEAD_ALLOCATIONS
                .saturating_sub(self.dead_allocations.len() + self.numeric_allocations.len())
    }

    pub(crate) fn admits_virtual_allocations(&self, count: usize) -> bool {
        self.admits_dead_allocations(count)
            && self.next_deferred_id.checked_add(count as u64).is_some()
    }

    /// Realize already-charged garbage before an observing sweep. This is not
    /// a new Id allocation: no callback runs and all accounting is unchanged.
    /// The caller must be between marking and the dead-object snapshot.
    pub(crate) fn settle_dead_allocations(&mut self) {
        #[cfg(any(test, feature = "ffi-test-hooks"))]
        SETTLED_ALLOCATIONS.set(
            SETTLED_ALLOCATIONS
                .get()
                .wrapping_add(self.dead_allocation_count()),
        );
        let allocations = core::mem::take(&mut self.dead_allocations);
        self.deferred_allocations.clear();
        for snapshot in allocations {
            self.materialize_snapshot(&snapshot);
        }
        for entry in core::mem::take(&mut self.numeric_allocations) {
            self.materialize_numeric_entry(&entry);
        }
        self.settle_dead_frame_indices();
    }

    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(crate) fn settled_allocation_count_for_test() -> usize {
        SETTLED_ALLOCATIONS.get()
    }

    /// Use only after proving that the sweep cannot observe these missing
    /// physical blocks. The returned count contributes to ordinary GC results.
    pub(crate) fn reclaim_dead_allocations(&mut self) -> usize {
        let count = self.dead_allocation_count();
        self.dead_allocations.clear();
        self.numeric_allocations.clear();
        self.deferred_allocations.clear();
        self.clear_dead_frame_indices();
        count
    }

    pub(crate) fn dead_finalizers(&self) -> impl Iterator<Item = (Oop, Finalizer)> + '_ {
        self.blocks.iter().filter_map(|(address, block)| {
            if block.permanence() == Permanence::Collectable && !block.marked() {
                block
                    .finalizer
                    .map(|finalizer| (core::ptr::with_exposed_provenance_mut(*address), finalizer))
            } else {
                None
            }
        })
    }

    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(crate) fn physical_allocation_count(&self) -> usize {
        self.blocks.len()
    }

    /// Owned journal storage, excluding allocator bookkeeping. Shared snapshots
    /// count once; retained vector capacity counts even after reclamation.
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    pub(crate) fn dead_allocation_storage_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        self.numeric_allocation_storage_bytes()
            + self.counted_frame_index_storage_bytes()
            + self.deferred_allocations.capacity()
                * core::mem::size_of::<(DeferredAllocation, usize)>()
            + self.dead_allocations.capacity() * core::mem::size_of::<std::rc::Rc<DeadAllocation>>()
            + self
                .dead_allocations
                .iter()
                .filter(|allocation| seen.insert(std::rc::Rc::as_ptr(allocation)))
                .map(|allocation| {
                    core::mem::size_of::<DeadAllocation>()
                        + 2 * core::mem::size_of::<usize>()
                        + allocation.separate_payload_capacity()
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_snapshots_preserve_padding_and_do_not_expand_the_shared_header() {
        // Larger clone/capture snapshots must not pay a larger header for this
        // numeric optimization. Small payload storage must share the Rc block.
        let previous_header = 2 * core::mem::size_of::<usize>() + core::mem::size_of::<Vec<u8>>();
        assert!(core::mem::size_of::<DeadAllocation>() <= previous_header);
        let mut heap = Heap::new();
        let vt = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(vt, vt);
        for length in (0..=17).chain([31, 32, 33, 127, 128, MAX_DEAD_PAYLOAD_BYTES]) {
            let bytes: Vec<_> = (0..length)
                .map(|i| ((i * 37 + length) & 255) as u8)
                .collect();
            let before = (
                heap.physical_allocation_count(),
                heap.allocation_count(),
                heap.bytes_since_collection(),
            );
            let snapshot = heap.prepare_virtual_bytes(vt, &bytes).unwrap();
            assert_eq!(
                (
                    heap.physical_allocation_count(),
                    heap.allocation_count(),
                    heap.bytes_since_collection()
                ),
                before
            );
            assert_eq!(&snapshot.payload()[..length], bytes.as_slice());
            assert!(snapshot.payload()[length..].iter().all(|b| *b == 0));
            if length <= INLINE_PAYLOAD_BYTES {
                let start = std::rc::Rc::as_ptr(&snapshot).addr();
                let payload = snapshot.payload().as_ptr().addr();
                assert!(
                    payload >= start
                        && payload + snapshot.payload().len()
                            <= start + core::mem::size_of::<DeadAllocation>()
                );
                assert_eq!(snapshot.separate_payload_capacity(), 0);
            }
            let expected = snapshot.payload().to_vec();
            let total = snapshot.total_size();
            let first = heap.record_virtual_clone(snapshot.clone()).unwrap();
            let second = heap.record_virtual_clone(snapshot).unwrap();
            assert_eq!(heap.bytes_since_collection(), before.2 + 2 * total);
            let a = heap.materialize_recorded_clone(second).unwrap();
            let b = heap.materialize_recorded_clone(first).unwrap();
            assert_ne!(a, b);
            for object in [a, b] {
                let payload = unsafe {
                    core::slice::from_raw_parts(
                        object.cast::<u8>(),
                        heap.payload_size(object).unwrap(),
                    )
                };
                assert_eq!(payload, expected.as_slice());
                assert_eq!(heap.vtable_of(object), Some(vt));
            }
            assert_eq!(heap.bytes_since_collection(), before.2 + 2 * total);
        }
    }

    #[test]
    fn raw_payloads_keep_exact_bits_fresh_identities_and_original_charges() {
        let mut heap = super::Heap::new();
        let vt = heap.allocate(0, super::Permanence::Permanent);
        heap.set_vtable(vt, vt);
        for raw in [i64::MIN, -1, 0, 1, i64::MAX] {
            let bytes = raw.to_ne_bytes();
            let before = heap.bytes_since_collection();
            let snapshot = heap.prepare_virtual_bytes(vt, &bytes).unwrap();
            assert_eq!(heap.bytes_since_collection(), before);
            let size = snapshot.total_size();
            let a = heap.record_virtual_clone(snapshot.clone()).unwrap();
            let b = heap.record_virtual_clone(snapshot).unwrap();
            let charged = heap.bytes_since_collection();
            assert_eq!(charged, before + 2 * size);
            let first = heap.materialize_recorded_clone(a).unwrap();
            let second = heap.materialize_recorded_clone(b).unwrap();
            assert_ne!(first, second);
            for object in [first, second] {
                assert_eq!(heap.vtable_of(object), Some(vt));
                assert_eq!(heap.read_payload_word(object, 0), Some(raw as usize));
            }
            assert_eq!(heap.bytes_since_collection(), charged);
            assert!(heap.materialize_recorded_clone(a).is_none());
        }
        let before = heap.bytes_since_collection();
        assert!(
            heap.prepare_virtual_bytes(vt, &vec![0; super::MAX_DEAD_PAYLOAD_BYTES + 1])
                .is_none()
        );
        assert_eq!(heap.bytes_since_collection(), before);
    }

    #[test]
    fn fresh_payload_recovery_matches_allocation_padding_and_accounting() {
        let mut heap = super::Heap::new();
        let vt = heap.allocate(0, super::Permanence::Permanent);
        heap.set_vtable(vt, vt);
        for count in [1usize, 2, 3, 4, 127, 128] {
            let words = vec![vt; count];
            let physical = heap.physical_allocation_count();
            let before = heap.bytes_since_collection();
            let snapshot = heap.prepare_virtual_payload(vt, &words).unwrap();
            let size = snapshot.total_size();
            let id = heap.record_virtual_clone(snapshot).unwrap();
            assert_eq!(heap.physical_allocation_count(), physical);
            assert_eq!(heap.bytes_since_collection(), before + size);
            let recovered = heap.materialize_recorded_clone(id).unwrap();
            assert_eq!(heap.bytes_since_collection(), before + size);
            let ordinary = heap.allocate(
                count * core::mem::size_of::<super::Oop>(),
                super::Permanence::Collectable,
            );
            heap.set_vtable(ordinary, vt);
            for (i, &word) in words.iter().enumerate() {
                heap.write_payload_word(ordinary, i, word as usize);
            }
            assert_ne!(ordinary, recovered);
            assert_eq!(heap.vtable_of(recovered), Some(vt));
            assert_eq!(heap.payload_size(recovered), heap.payload_size(ordinary));
            for i in 0..heap.payload_size(ordinary).unwrap() / core::mem::size_of::<super::Oop>() {
                assert_eq!(
                    heap.read_payload_word(recovered, i),
                    heap.read_payload_word(ordinary, i)
                );
            }
            assert_eq!(heap.materialize_recorded_clone(id), None);
        }
    }
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

    #[test]
    fn virtual_clone_materialization_is_once_per_identity_and_does_not_recharge() {
        let mut heap = Heap::new();
        let prototype = heap.allocate(16, Permanence::Permanent);
        heap.set_vtable(prototype, prototype);
        heap.write_payload_word(prototype, 0, 23);
        heap.write_payload_word(prototype, 1, 23);
        let snapshot = heap.prepare_dead_clone(prototype).unwrap();
        let first = heap.record_virtual_clone(snapshot.clone()).unwrap();
        assert!(heap.record_dead_allocation(snapshot.clone()));
        let second = heap.record_virtual_clone(snapshot.clone()).unwrap();
        let third = heap.record_virtual_clone(snapshot).unwrap();
        let before = (
            heap.allocation_count(),
            heap.allocated_bytes(),
            heap.bytes_since_collection(),
        );
        // Out-of-order settlement moves the last record. Its identity must
        // remain valid; the anonymous dead allocation must remain anonymous.
        let a = heap.materialize_recorded_clone(first).unwrap();
        let b = heap.materialize_recorded_clone(third).unwrap();
        let c = heap.materialize_recorded_clone(second).unwrap();
        assert_ne!(a, b);
        assert_ne!(b, c);
        for object in [a, b, c] {
            assert_eq!(heap.vtable_of(object), Some(prototype));
            assert_eq!(heap.read_payload_word(object, 0), Some(23));
            assert_eq!(heap.read_payload_word(object, 1), Some(23));
        }
        assert_eq!(heap.materialize_recorded_clone(first), None);
        assert_eq!(
            (
                heap.allocation_count(),
                heap.allocated_bytes(),
                heap.bytes_since_collection()
            ),
            before
        );
        assert_eq!(heap.physical_allocation_count(), 4);
        assert_eq!(heap.reclaim_dead_allocations(), 1);
    }

    #[test]
    fn collection_and_exhaustion_cannot_reuse_a_virtual_identity() {
        let mut heap = Heap::new();
        let prototype = heap.allocate(0, Permanence::Permanent);
        heap.set_vtable(prototype, prototype);
        let snapshot = heap.prepare_dead_clone(prototype).unwrap();
        let stale = heap.record_virtual_clone(snapshot.clone()).unwrap();
        heap.reclaim_dead_allocations();
        let current = heap.record_virtual_clone(snapshot.clone()).unwrap();
        assert_ne!(current, stale);
        assert!(heap.materialize_recorded_clone(stale).is_none());
        heap.settle_dead_allocations();
        assert!(heap.materialize_recorded_clone(current).is_none());
        heap.next_deferred_id = u64::MAX;
        let before = (heap.allocation_count(), heap.bytes_since_collection());
        assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
        assert_eq!(
            (heap.allocation_count(), heap.bytes_since_collection()),
            before
        );
        assert!(
            heap.record_dead_allocation(snapshot),
            "dead allocations need no materialization capability"
        );
    }
}
