//! Upper allocation accounting, collection metadata and deferred allocations.
//! ObjectMemory supplies object allocation independently of this GC bookkeeping.
#![allow(dead_code)]

use crate::object_memory::{Allocation, ObjectMemory, vtable_slot_from_payload};
pub(crate) use crate::object_memory::{BLOCK_ALIGNMENT, VTABLE_HEADER_SIZE, PAYLOAD_ALIGNMENT, PayloadWords};

use crate::abi::{Finalizer, Oop};
use birnam_abi::finalizers::SlotV1;
use crate::boundary;
use crate::raw::AddressMap;

#[cfg(feature = "ffi-test-hooks")]
pub(crate) type LayoutProbe =
    unsafe extern "C" fn(*mut u8, Oop, usize, usize, Oop) -> core::ffi::c_int;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Permanence {
    Collectable,
    Permanent,
}

// Collection policy is owned separately from physical storage. These records
// can be installed when Lifetime adopts an already populated Id allocation set.
struct BlockLifetime {
    metadata: u64,
    finalizer: SlotV1,
}

impl BlockLifetime {
    fn permanence(&self) -> Permanence {
        if self.metadata & 2 == 0 { Permanence::Collectable } else { Permanence::Permanent }
    }
    fn marked(&self) -> bool { self.metadata & 1 != 0 }
    fn set_marked(&mut self, marked: bool) {
        self.metadata = (self.metadata & !1) | u64::from(marked);
    }
}

/// A short-lived joined view, never an owner or a callback-bearing Id record.
struct Block<'a> {
    allocation: &'a Allocation,
    lifetime: &'a BlockLifetime,
}
impl core::ops::Deref for Block<'_> {
    type Target = Allocation;
    fn deref(&self) -> &Allocation { self.allocation }
}
impl Block<'_> {
    fn permanence(&self) -> Permanence { self.lifetime.permanence() }
    fn marked(&self) -> bool { self.lifetime.marked() }
}

/// An allocation detached for collection, with its finalizer.
/// The collector invokes the finalizer; releasing storage never does.
pub(crate) struct DetachedBlock {
    block: Allocation,
    finalizer: SlotV1,
}

impl DetachedBlock {
    pub(crate) fn object(&self) -> Oop {
        self.block.payload().as_ptr().cast()
    }

    pub(crate) fn finalizer(&self) -> Option<Finalizer> {
        self.finalizer.callback
    }

    pub(crate) fn reclaim(self) {
        drop(self);
    }
}

/// GC bookkeeping and deferred allocation for one runtime.
///
/// Registry keys are exact payload addresses. Hash iteration order is never
/// exposed as language behavior.
pub(crate) struct Heap {
    blocks: AddressMap<BlockLifetime>,
    memory: ObjectMemory,
    budget: crate::allocation_budget::Service,
    dead_allocations: Vec<std::rc::Rc<dead_allocations::DeadAllocation>>,
    deferred_allocations: dead_allocations::deferred_index::Index,
    numeric_allocations: Vec<dead_allocations::numeric::Entry>,
    counted_frame_indices: Option<Box<dead_allocations::frame_indices::CountedFrameIndices>>,
    next_deferred_id: crate::deferred_identity::Service,
    // Retaining records precede the service in field drop order.
    finalizers: crate::finalizers::Service,
}

mod dead_allocations;
pub(crate) use dead_allocations::numeric::Batch as PayloadBatch;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) use dead_allocations::MAX_DEAD_ALLOCATIONS;
pub(crate) use dead_allocations::{DeadAllocation, DeferredAllocation, MAX_DEAD_PAYLOAD_BYTES};

impl Heap {
    pub(crate) fn new() -> Self {
        Self::adopt(ObjectMemory::new())
    }

    /// Adopt the existing Id allocations without copying payloads or resetting
    /// identities. The caller retains early roots until the collector is ready.
    /// Construction does not collect or run finalizers.
    pub(crate) fn adopt(memory: ObjectMemory) -> Self {
        let blocks = memory.iter().map(|(address, _)| (address, BlockLifetime {
            metadata: 0, finalizer: SlotV1::EMPTY,
        })).collect();
        let mut budget = crate::allocation_budget::Service::new();
        for (_, allocation) in memory.iter() {
            boundary::require_invariant(budget.charge(allocation.total_size), "GC allocation byte counter overflow");
        }
        Self {
            blocks,
            memory,
            budget,
            dead_allocations: Vec::new(),
            numeric_allocations: Vec::new(),
            counted_frame_indices: None,
            deferred_allocations: dead_allocations::deferred_index::Index::new(),
            next_deferred_id: crate::deferred_identity::Service::new(),
            finalizers: crate::finalizers::Service::new(),
        }
    }

    #[cfg(test)]
    fn set_deferred_identity_for_test(&mut self, value: u64) { self.next_deferred_id.set(value); }

    #[cfg(test)]
    fn set_allocation_identity_for_test(&mut self, value: u64) { self.memory.set_last_identity(value); }

    pub(crate) fn allocation_total_size(payload_size: usize) -> Option<usize> {
        ObjectMemory::allocation_total_size(payload_size)
    }

    fn block(&self, address: usize) -> Option<Block<'_>> {
        Some(Block { allocation: self.memory.get(address)?, lifetime: self.blocks.get(&address)? })
    }

    fn block_views(&self) -> impl Iterator<Item = Block<'_>> {
        self.memory.iter().map(|(address, allocation)| Block {
            allocation,
            lifetime: self.blocks.get(&address).expect("Id allocation has no Lifetime record"),
        })
    }

    pub(crate) fn allocate(&mut self, payload_size: usize, permanence: Permanence) -> Oop {
        let object = self.memory.allocate(payload_size);
        let total_size = self.memory.get(object.addr()).unwrap().total_size;
        let previous = self.blocks.insert(object.addr(), BlockLifetime {
            metadata: if permanence == Permanence::Permanent { 2 } else { 0 },
            finalizer: SlotV1::EMPTY,
        });
        boundary::require_invariant(previous.is_none(), "allocator reused a live payload address");
        boundary::require_invariant(self.budget.charge(total_size), "GC allocation byte counter overflow");
        object
    }

    /// Read-only inventory; neither materializes journals nor changes marks.
    pub(crate) fn storage_allocations(&self) -> Vec<crate::abi::storage::AllocationV1> {
        use crate::abi::storage::{AllocationV1, HAS_FINALIZER, PERMANENT};
        let mut result: Vec<_> = self.block_views().map(|block| AllocationV1 {
            object: block.payload().as_ptr().cast(),
            identity: block.identity(),
            requested_size: block.requested_size(),
            padded_size: block.total_size - VTABLE_HEADER_SIZE,
            allocation_bytes: block.total_size,
            behavior: unsafe { vtable_slot_from_payload(block.payload()).read() },
            flags: if block.permanence() == Permanence::Permanent { PERMANENT } else { 0 }
                | if block.lifetime.finalizer.callback.is_some() { HAS_FINALIZER } else { 0 },
            trace_hook: None,
        }).collect();
        result.sort_unstable_by_key(|entry| entry.identity);
        result
    }

    pub(crate) fn contains(&self, object: Oop) -> bool {
        self.memory.get(object.addr()).is_some()
    }

    pub(crate) fn allocation_count(&self) -> usize {
        self.memory.len() + self.dead_allocation_count()
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        let physical = self.block_views().fold(0usize, |total, block| {
            total.saturating_add(block.total_size)
        });
        self.dead_allocations.iter().fold(
            self.numeric_allocated_bytes()
                .saturating_add(self.counted_frame_index_bytes())
                .saturating_add(physical),
            |total, allocation| total.saturating_add(allocation.total_size()),
        )
    }

    pub(crate) fn bytes_since_collection(&self) -> usize {
        self.budget.read()
    }

    pub(crate) fn reset_allocation_bytes(&mut self) {
        self.budget.replace(0);
    }

    pub(crate) fn base_address(&self, object: Oop) -> Option<usize> {
        self.memory.get(object.addr())
            .map(|block| block.base.as_ptr().addr())
    }

    pub(crate) fn payload_address(&self, object: Oop) -> Option<usize> {
        self.memory.get(object.addr())
            .map(|block| block.payload().as_ptr().addr())
    }

    /// Recover a live payload pointer without reconstructing it from its
    /// integer membership key and losing allocation provenance.
    pub(crate) fn object_at_address(&self, address: usize) -> Option<Oop> {
        self.memory.get(address)
            .map(|block| block.payload().as_ptr().cast())
    }

    /// Report the padded payload extent, matching `objectPayloadSize`.
    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.memory.get(object.addr())
            .map(|block| block.total_size - VTABLE_HEADER_SIZE)
    }

    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.permanence())
    }

    /// Vtable headers of permanent objects are strong floor edges even though
    /// permanent payloads do not participate in the mark worklist.
    pub(crate) fn permanent_vtables(&self) -> Vec<Oop> {
        self.block_views()
            .filter(|block| block.permanence() == Permanence::Permanent)
            .map(|block| {
                // SAFETY: every registered block owns one initialized vtable
                // header immediately before its payload.
                unsafe { vtable_slot_from_payload(block.payload()).read() }
            })
            .filter(|vtable| !vtable.is_null())
            .collect()
    }

    pub(crate) fn is_marked(&self, object: Oop) -> Option<bool> {
        self.blocks.get(&object.addr()).map(|block| block.marked())
    }

    /// Mark exactly one registered collectable payload address. Interior,
    /// header, foreign, null, and permanent addresses are rejected.
    pub(crate) fn mark_collectable_address(&mut self, address: usize) -> bool {
        let Some(block) = self.blocks.get_mut(&address) else {
            return false;
        };
        if block.permanence() == Permanence::Permanent || block.marked() {
            return false;
        }
        block.set_marked(true);
        true
    }

    /// Copy only the coordinates of one exact live payload for allocation-free
    /// GC scanning. The range intentionally does not borrow `Heap`, because
    /// marking must mutate registry metadata while the separate payload
    /// allocation remains stable.
    pub(crate) fn payload_words(&self, address: usize) -> Option<PayloadWords> {
        self.memory.payload_words(address)
    }

    pub(crate) fn read_payload_word(&self, object: Oop, index: usize) -> Option<usize> {
        self.memory.read_payload_word(object, index)
    }

    pub(crate) fn write_payload_word(&mut self, object: Oop, index: usize, value: usize) -> bool {
        self.memory.write_payload_word(object, index, value)
    }

    pub(crate) fn clear_marks(&mut self) {
        for block in self.blocks.values_mut() {
            block.set_marked(false);
        }
    }

    /// Snapshot unreachable collectable identities before sweep mutates the
    /// registry. Permanent blocks are never candidates.
    pub(crate) fn dead_collectable_addresses(&self) -> Vec<usize> {
        self.blocks
            .iter()
            .filter_map(|(address, block)| {
                (block.permanence() == Permanence::Collectable && !block.marked()).then_some(*address)
            })
            .collect()
    }

    /// Remove one previously identified dead block while leaving its allocation
    /// live for a finalizer. Marked and permanent blocks cannot be detached.
    pub(crate) fn detach_dead_collectable(&mut self, address: usize) -> Option<DetachedBlock> {
        let block = self.blocks.get(&address)?;
        if block.permanence() != Permanence::Collectable || block.marked() {
            return None;
        }
        let block = self
            .blocks
            .remove(&address)
            .unwrap_or_else(|| boundary::fatal_invariant("dead GC block disappeared"));
        Some(DetachedBlock {
            block: self.memory.detach(address).expect("dead Id allocation disappeared"),
            finalizer: block.finalizer,
        })
    }

    pub(crate) fn has_finalizer(&self, object: Oop) -> Option<bool> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.finalizer.callback.is_some())
    }

    pub(crate) fn set_finalizer(&mut self, object: Oop, finalizer: Finalizer) -> bool {
        let Some(block) = self.blocks.get_mut(&object.addr()) else {
            return false;
        };
        let slot = self.finalizers.slot(block.permanence(), finalizer);
        if slot.callback.is_none() {
            return false;
        }
        block.finalizer = slot;
        true
    }

    pub(crate) fn finalizers_api(&self) -> *mut birnam_abi::finalizers::ApiV1 {
        self.finalizers.api()
    }

    /// Apply an accepted attachment to the record whose fact was just read.
    pub(crate) fn replace_finalizer(&mut self, object: Oop, slot: SlotV1) {
        let block = self.blocks.get_mut(&object.addr())
            .unwrap_or_else(|| boundary::fatal_invariant("finalizer record disappeared"));
        block.finalizer = slot;
    }

    pub(crate) fn set_vtable(&mut self, object: Oop, vtable: Oop) -> bool {
        self.memory.set_vtable(object, vtable)
    }

    /// Shallow-copy one exact live object's padded payload into a fresh
    /// collectable block with the same vtable. Finalizer metadata is deliberately
    /// not copied; resource-owning families must override clone and install fresh
    /// ownership metadata.
    pub(crate) fn clone_collectable(&mut self, object: Oop) -> Option<Oop> {
        let source = self.memory.get(object.addr())?;
        let payload_size = source.total_size - VTABLE_HEADER_SIZE;
        // Copy out before mutating the registry: allocation can grow the map.
        // SAFETY: exact membership proves the source payload is live for its
        // recorded padded extent.
        let bytes = unsafe {
            std::slice::from_raw_parts(object.cast::<u8>().cast_const(), payload_size).to_vec()
        };
        let vtable = self.vtable_of(object)?;
        let clone = self.allocate(payload_size, Permanence::Collectable);
        boundary::require_invariant(
            self.set_vtable(clone, vtable),
            "cloned object block was not registered",
        );
        if payload_size != 0 {
            // SAFETY: clone owns payload_size writable bytes and bytes contains
            // exactly that many initialized source bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), clone.cast::<u8>(), payload_size)
            };
        }
        Some(clone)
    }

    pub(crate) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        self.memory.vtable_of(object)
    }

    /// Read the header on the dispatch hot path after the caller has already
    /// established that `object` is a live floor object.
    ///
    /// # Safety
    ///
    /// `object` must be the exact non-null payload address of a live block.
    pub(crate) unsafe fn vtable_of_valid_object(object: Oop) -> Oop {
        unsafe { ObjectMemory::vtable_of_valid_object(object) }
    }

    #[cfg(test)]
    pub(crate) fn reclaim_for_test(&mut self, object: Oop) -> bool {
        let Some(detached) = self.detach_dead_collectable(object.addr()) else {
            return false;
        };
        detached.reclaim();
        true
    }
}

/// Allocate one isolated block while a foreign C-layout probe inspects it.
///
/// # Safety
///
/// `probe` must access only the supplied header/payload allocation, within the
/// reported extent, and must not retain either pointer after it returns.
#[cfg(feature = "ffi-test-hooks")]
pub(crate) unsafe fn run_layout_probe(
    requested_size: usize,
    vtable: Oop,
    probe: LayoutProbe,
) -> core::ffi::c_int {
    let mut heap = Heap::new();
    let object = heap.allocate(requested_size, Permanence::Collectable);
    if !heap.set_vtable(object, vtable) {
        return 0;
    }
    let Some(base_address) = heap.base_address(object) else {
        return 0;
    };
    let Some(reported_size) = heap.payload_size(object) else {
        return 0;
    };

    // No Heap reference or collection guard survives this call: all arguments
    // are copied scalars/raw pointers, so the foreign callback cannot alias an
    // outstanding Rust borrow of runtime state.
    let accepted = unsafe {
        probe(
            base_address as *mut u8,
            object,
            requested_size,
            reported_size,
            vtable,
        )
    };
    if accepted != 1 || heap.vtable_of(object) != Some(vtable) {
        return 0;
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fake_vtable() -> Oop {
        static MARKER: u64 = 0;
        core::ptr::addr_of!(MARKER).cast_mut().cast::<c_void>()
    }

    unsafe extern "C" fn finalizer(_: Oop) {}

    static FINALIZER_CALLS: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn counting_finalizer(_: Oop) {
        FINALIZER_CALLS.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn total_size_formula_handles_unaligned_and_overflowing_sizes() {
        for requested in 0..=1025 {
            let expected = (VTABLE_HEADER_SIZE + requested + 7) & !7;
            assert_eq!(Heap::allocation_total_size(requested), Some(expected));
        }
        assert_eq!(Heap::allocation_total_size(usize::MAX), None);
    }

    #[test]
    fn allocation_bytes_accumulate_exact_layouts_and_reset_only_after_collection() {
        let mut heap = Heap::new();
        assert_eq!(heap.bytes_since_collection(), 0);
        let first = heap.allocate(0, Permanence::Collectable);
        let first_size = Heap::allocation_total_size(0).unwrap();
        let second_size = Heap::allocation_total_size(17).unwrap();
        let _second = heap.allocate(17, Permanence::Permanent);
        assert_eq!(heap.bytes_since_collection(), first_size + second_size);

        assert!(heap.reclaim_for_test(first));
        assert_eq!(heap.bytes_since_collection(), first_size + second_size);
        heap.reset_allocation_bytes();
        assert_eq!(heap.bytes_since_collection(), 0);

        let third_size = Heap::allocation_total_size(9).unwrap();
        let _third = heap.allocate(9, Permanence::Collectable);
        assert_eq!(heap.bytes_since_collection(), third_size);
    }

    #[test]
    fn blocks_have_exact_base_header_and_payload_geometry() {
        // Object allocation has no collection/finalizer fields. Lifetime
        // metadata is separate; the old combined four-word bound no longer applies.
        assert_eq!(size_of::<Allocation>(), 3 * size_of::<usize>());
        let mut heap = Heap::new();
        let sizes = [0, 1, 7, 8, 9, 15, 16, 17, 63, 64, 65, 255, 256, 4095];

        for (index, requested) in sizes.into_iter().enumerate() {
            let permanence = if index % 2 == 0 {
                Permanence::Collectable
            } else {
                Permanence::Permanent
            };
            let object = heap.allocate(requested, permanence);
            let base = heap.base_address(object).expect("registered base");
            let payload = heap.payload_address(object).expect("registered payload");
            let expected_total = (VTABLE_HEADER_SIZE + requested + 7) & !7;

            assert_eq!(base % BLOCK_ALIGNMENT, 0);
            assert_eq!(payload % PAYLOAD_ALIGNMENT, 0);
            assert_eq!(payload - base, VTABLE_HEADER_SIZE);
            assert_eq!(object.addr(), payload);
            assert_eq!(
                heap.payload_size(object),
                Some(expected_total - VTABLE_HEADER_SIZE)
            );
            let entry = heap.storage_allocations().into_iter().find(|entry| entry.object == object).unwrap();
            assert_eq!(entry.requested_size, requested);
            assert_eq!(entry.identity, index as u64 + 1);
            assert_eq!(heap.permanence_of(object), Some(permanence));
            assert_eq!(heap.is_marked(object), Some(false));
            assert_eq!(heap.has_finalizer(object), Some(false));
            assert_eq!(heap.vtable_of(object), Some(core::ptr::null_mut()));
        }

        assert_eq!(heap.allocation_count(), sizes.len());
    }

    #[test]
    fn checked_payload_word_access_preserves_bounds_and_exact_membership() {
        let mut heap = Heap::new();
        let object = heap.allocate(2 * size_of::<usize>(), Permanence::Collectable);
        assert!(heap.write_payload_word(object, 0, 17));
        assert!(heap.write_payload_word(object, 1, 23));
        assert_eq!(heap.read_payload_word(object, 0), Some(17));
        assert_eq!(heap.read_payload_word(object, 1), Some(23));
        assert!(!heap.write_payload_word(object, 2, 29));
        assert_eq!(heap.read_payload_word(object, 2), None);
        assert!(!heap.write_payload_word(core::ptr::null_mut(), 0, 31));
        assert_eq!(heap.read_payload_word(core::ptr::null_mut(), 0), None);
    }

    #[test]
    fn header_round_trips_and_padded_payload_is_zeroed_and_writable() {
        let mut heap = Heap::new();
        let vtable = fake_vtable();

        for requested in [1, 7, 8, 9, 31, 32, 33, 257] {
            let object = heap.allocate(requested, Permanence::Collectable);
            assert!(heap.set_vtable(object, vtable));
            assert_eq!(heap.vtable_of(object), Some(vtable));

            let extent = heap.payload_size(object).expect("registered payload size");
            // SAFETY: object is the live payload start and extent is the full
            // padded payload area owned by heap for this test's duration.
            let payload = unsafe { std::slice::from_raw_parts_mut(object.cast::<u8>(), extent) };
            assert!(payload.iter().all(|byte| *byte == 0));
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte = (index as u8).wrapping_mul(17).wrapping_add(3);
            }
            assert!(
                payload
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| *byte == (index as u8).wrapping_mul(17).wrapping_add(3))
            );
        }
    }

    #[test]
    fn payload_word_range_reads_live_words_without_copying_the_payload() {
        let mut heap = Heap::new();
        let object = heap.allocate(3 * size_of::<usize>(), Permanence::Collectable);
        let expected = [0x1234usize, object.addr(), usize::MAX];
        // SAFETY: object owns three aligned writable words.
        unsafe {
            object
                .cast::<usize>()
                .copy_from_nonoverlapping(expected.as_ptr(), expected.len())
        };

        let words = heap.payload_words(object.addr()).expect("live payload");
        assert_eq!(words.len(), expected.len());
        for (index, expected_word) in expected.into_iter().enumerate() {
            // SAFETY: the registered block remains live and unchanged
            // throughout this bounded scan.
            assert_eq!(unsafe { words.read(index) }, expected_word);
        }
        assert!(heap.payload_words(object.addr() + 1).is_none());
    }

    #[test]
    fn shallow_clone_copies_padded_payload_and_vtable_without_finalizer() {
        let mut heap = Heap::new();
        let vtable = fake_vtable();
        let object = heap.allocate(9, Permanence::Permanent);
        assert!(heap.set_vtable(object, vtable));
        assert!(!heap.set_finalizer(object, finalizer));
        let extent = heap.payload_size(object).unwrap();
        // SAFETY: object owns its complete reported payload extent.
        let payload = unsafe { std::slice::from_raw_parts_mut(object.cast::<u8>(), extent) };
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = index as u8 ^ 0xa5;
        }

        let clone = heap.clone_collectable(object).unwrap();
        assert_ne!(clone, object);
        assert_eq!(heap.vtable_of(clone), Some(vtable));
        assert_eq!(heap.payload_size(clone), Some(extent));
        assert_eq!(heap.permanence_of(clone), Some(Permanence::Collectable));
        assert_eq!(heap.has_finalizer(clone), Some(false));
        // SAFETY: both registered blocks own extent readable bytes.
        let original = unsafe { std::slice::from_raw_parts(object.cast::<u8>(), extent) };
        let copied = unsafe { std::slice::from_raw_parts(clone.cast::<u8>(), extent) };
        assert_eq!(copied, original);
    }

    #[test]
    fn finalizer_metadata_replaces_on_collectables_and_ignores_other_carriers() {
        let mut heap = Heap::new();
        let collectable = heap.allocate(0, Permanence::Collectable);
        let permanent = heap.allocate(0, Permanence::Permanent);
        assert!(heap.set_finalizer(collectable, finalizer));
        assert_eq!(heap.has_finalizer(collectable), Some(true));
        assert!(!heap.set_finalizer(permanent, finalizer));
        assert_eq!(heap.has_finalizer(permanent), Some(false));
        assert!(!heap.set_finalizer(core::ptr::null_mut(), finalizer));
    }

    #[test]
    fn checked_operations_reject_null_and_foreign_addresses_without_dereference() {
        let mut heap = Heap::new();
        let object = heap.allocate(8, Permanence::Collectable);
        let mut foreign_word: usize = 0;
        let foreign = core::ptr::addr_of_mut!(foreign_word).cast::<c_void>();

        for invalid in [core::ptr::null_mut(), foreign] {
            assert!(!heap.contains(invalid));
            assert_eq!(heap.base_address(invalid), None);
            assert_eq!(heap.payload_size(invalid), None);
            assert_eq!(heap.vtable_of(invalid), None);
            assert!(!heap.set_vtable(invalid, fake_vtable()));
        }
        assert!(heap.contains(object));
    }

    #[test]
    fn sweep_candidates_exclude_marked_and_permanent_blocks_before_detaching() {
        let mut heap = Heap::new();
        let first_dead = heap.allocate(8, Permanence::Collectable);
        let live = heap.allocate(0, Permanence::Collectable);
        let permanent = heap.allocate(0, Permanence::Permanent);
        let second_dead = heap.allocate(17, Permanence::Collectable);
        assert!(heap.set_finalizer(first_dead, counting_finalizer));
        assert!(heap.mark_collectable_address(live.addr()));

        let dead = heap.dead_collectable_addresses();
        assert_eq!(dead.len(), 2);
        assert!(dead.contains(&first_dead.addr()));
        assert!(dead.contains(&second_dead.addr()));
        assert!(!dead.contains(&live.addr()));
        assert!(!dead.contains(&permanent.addr()));

        FINALIZER_CALLS.store(0, Ordering::Relaxed);
        let detached = heap
            .detach_dead_collectable(first_dead.addr())
            .expect("dead block detaches");
        assert_eq!(detached.object(), first_dead);
        assert!(!heap.contains(first_dead));
        assert_eq!(heap.allocation_count(), 3);
        // The allocation remains readable while detached so its finalizer can
        // inspect or release resources named by the payload.
        assert_eq!(unsafe { detached.object().cast::<usize>().read() }, 0);
        let callback = detached.finalizer().expect("copied finalizer metadata");
        unsafe { callback(detached.object()) };
        assert_eq!(FINALIZER_CALLS.load(Ordering::Relaxed), 1);
        detached.reclaim();

        assert!(heap.detach_dead_collectable(live.addr()).is_none());
        assert!(heap.detach_dead_collectable(permanent.addr()).is_none());
        assert!(heap.detach_dead_collectable(first_dead.addr()).is_none());
    }

    #[test]
    fn exact_membership_tracks_reclamation_and_fast_path_needs_no_lookup() {
        let mut heap = Heap::new();
        let object = heap.allocate(0, Permanence::Collectable);
        let vtable = fake_vtable();
        assert!(heap.set_vtable(object, vtable));
        // SAFETY: object remains an exact live member at this call site.
        assert_eq!(unsafe { Heap::vtable_of_valid_object(object) }, vtable);

        assert!(heap.reclaim_for_test(object));
        assert!(!heap.contains(object));
        assert_eq!(heap.vtable_of(object), None);
        assert!(!heap.reclaim_for_test(object));
    }
}

#[cfg(test)]
mod allocation_budget_tests;

#[cfg(test)]
mod allocation_identity_tests;
