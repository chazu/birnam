//! Raw aligned object blocks and their exact-address registry.
//!
//! R2.1 lands this private mechanism before runtime bootstrap wires it in.
#![allow(dead_code)]

use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};
use std::ptr::NonNull;

use crate::abi::{Finalizer, Oop};
use crate::boundary;
use crate::raw::AddressMap;

#[cfg(feature = "ffi-test-hooks")]
pub(crate) type LayoutProbe =
    unsafe extern "C" fn(*mut u8, Oop, usize, usize, Oop) -> core::ffi::c_int;

pub(crate) const BLOCK_ALIGNMENT: usize = 16;
pub(crate) const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();
pub(crate) const PAYLOAD_ALIGNMENT: usize = align_of::<Oop>();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Permanence {
    Collectable,
    Permanent,
}

struct Block {
    /// Provenance-carrying allocation base returned by `alloc_zeroed`.
    base: NonNull<u8>,
    total_size: usize,
    permanence: Permanence,
    marked: bool,
    finalizer: Option<Finalizer>,
}

impl Block {
    fn payload(&self) -> NonNull<u8> {
        // SAFETY: a registered or detached Block retains its allocation base
        // until reclaim. Derivation preserves provenance without storing a
        // second pointer in every heap-registry entry.
        unsafe { payload_from_base(self.base) }
    }
}

/// One unreachable allocation detached from the registry before foreign code
/// runs. Its payload remains live until `reclaim` consumes it.
pub(crate) struct DetachedBlock {
    block: Block,
}

/// Copied coordinates for scanning one live object's pointer-sized payload
/// words without allocating a per-object snapshot.
///
/// This deliberately carries no Rust borrow of `Heap`: the collector must
/// mutate mark metadata while reading the separately allocated object payload.
#[derive(Clone, Copy)]
pub(crate) struct PayloadWords {
    pointer: *const usize,
    length: usize,
}

impl PayloadWords {
    pub(crate) fn len(self) -> usize {
        self.length
    }

    /// Read one word from the described live payload.
    ///
    /// # Safety
    ///
    /// The originating block must remain registered and its payload must not
    /// be mutated for the duration of the scan. `index` must be below `len`.
    pub(crate) unsafe fn read(self, index: usize) -> usize {
        debug_assert!(index < self.length);
        // SAFETY: forwarded caller contract keeps the live aligned allocation
        // stable and bounds index to this payload range.
        unsafe { self.pointer.add(index).read() }
    }
}

impl DetachedBlock {
    pub(crate) fn object(&self) -> Oop {
        self.block.payload().as_ptr().cast()
    }

    pub(crate) fn finalizer(&self) -> Option<Finalizer> {
        self.block.finalizer
    }

    pub(crate) fn reclaim(self) {
        drop(self);
    }
}

impl Drop for DetachedBlock {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.block.total_size, BLOCK_ALIGNMENT)
            .unwrap_or_else(|_| boundary::fatal_invariant("stored block layout became invalid"));
        // SAFETY: detaching transferred this still-live allocation out of the
        // Heap registry. DetachedBlock is its sole owner and drops exactly once
        // with the same layout used by allocate.
        unsafe { dealloc(self.block.base.as_ptr(), layout) };
    }
}

/// Convert an owned block base to its payload address.
///
/// # Safety
///
/// `base` must identify a live allocation of at least `VTABLE_HEADER_SIZE`
/// bytes with the floor's block layout.
unsafe fn payload_from_base(base: NonNull<u8>) -> NonNull<u8> {
    // SAFETY: guaranteed by the caller; one-past is valid for zero payload.
    unsafe { NonNull::new_unchecked(base.as_ptr().add(VTABLE_HEADER_SIZE)) }
}

/// Recover the preceding vtable word from a validated payload address.
///
/// # Safety
///
/// `payload` must be the exact payload start of a live floor allocation.
unsafe fn vtable_slot_from_payload(payload: NonNull<u8>) -> *mut Oop {
    // SAFETY: guaranteed by the caller; the header is one word immediately
    // before every exact floor payload address.
    unsafe { payload.as_ptr().sub(VTABLE_HEADER_SIZE).cast() }
}

/// Single-runtime owner of raw object blocks.
///
/// Registry keys are exact payload addresses. Hash iteration order is never
/// exposed as language behavior.
pub(crate) struct Heap {
    blocks: AddressMap<Block>,
    bytes_since_collection: usize,
    dead_allocations: Vec<std::rc::Rc<dead_allocations::DeadAllocation>>,
    deferred_allocations: Vec<(DeferredAllocation, usize)>,
    next_deferred_id: u64,
}

mod dead_allocations;
pub(crate) use dead_allocations::{DeadAllocation, DeferredAllocation, MAX_DEAD_PAYLOAD_BYTES};

impl Heap {
    pub(crate) fn new() -> Self {
        Self {
            blocks: AddressMap::default(),
            bytes_since_collection: 0,
            dead_allocations: Vec::new(),
            deferred_allocations: Vec::new(),
            next_deferred_id: 0,
        }
    }

    pub(crate) fn allocation_total_size(payload_size: usize) -> Option<usize> {
        VTABLE_HEADER_SIZE
            .checked_add(payload_size)?
            .checked_add(PAYLOAD_ALIGNMENT - 1)
            .map(|size| size & !(PAYLOAD_ALIGNMENT - 1))
    }

    pub(crate) fn allocate(&mut self, payload_size: usize, permanence: Permanence) -> Oop {
        let total_size = Self::allocation_total_size(payload_size)
            .unwrap_or_else(|| boundary::fatal_invariant("object allocation size overflow"));
        let layout = Layout::from_size_align(total_size, BLOCK_ALIGNMENT)
            .unwrap_or_else(|_| boundary::fatal_invariant("invalid object allocation layout"));

        // SAFETY: layout has nonzero size (the header is one word) and a valid
        // power-of-two alignment. The returned allocation is owned by this Heap.
        let base = unsafe { alloc_zeroed(layout) };
        let base = NonNull::new(base).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: total_size is at least VTABLE_HEADER_SIZE and base owns the
        // newly allocated floor block.
        let payload = unsafe { payload_from_base(base) };
        let payload_address = payload.as_ptr().addr();

        let previous = self.blocks.insert(
            payload_address,
            Block {
                base,
                total_size,
                permanence,
                marked: false,
                finalizer: None,
            },
        );
        boundary::require_invariant(
            previous.is_none(),
            "allocator reused a live payload address",
        );
        self.bytes_since_collection = self
            .bytes_since_collection
            .checked_add(total_size)
            .unwrap_or_else(|| boundary::fatal_invariant("GC allocation byte counter overflow"));
        payload.as_ptr().cast()
    }

    pub(crate) fn contains(&self, object: Oop) -> bool {
        !object.is_null() && self.blocks.contains_key(&object.addr())
    }

    pub(crate) fn allocation_count(&self) -> usize {
        self.blocks.len() + self.dead_allocations.len()
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        let physical = self.blocks.values().fold(0usize, |total, block| {
            total.saturating_add(block.total_size)
        });
        self.dead_allocations
            .iter()
            .fold(physical, |total, allocation| {
                total.saturating_add(allocation.total_size())
            })
    }

    pub(crate) fn bytes_since_collection(&self) -> usize {
        self.bytes_since_collection
    }

    pub(crate) fn reset_allocation_bytes(&mut self) {
        self.bytes_since_collection = 0;
    }

    pub(crate) fn base_address(&self, object: Oop) -> Option<usize> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.base.as_ptr().addr())
    }

    pub(crate) fn payload_address(&self, object: Oop) -> Option<usize> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.payload().as_ptr().addr())
    }

    /// Recover a live payload pointer without reconstructing it from its
    /// integer membership key and losing allocation provenance.
    pub(crate) fn object_at_address(&self, address: usize) -> Option<Oop> {
        self.blocks
            .get(&address)
            .map(|block| block.payload().as_ptr().cast())
    }

    /// Report the padded payload extent, matching `objectPayloadSize`.
    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.total_size - VTABLE_HEADER_SIZE)
    }

    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.permanence)
    }

    /// Vtable headers of permanent objects are strong floor edges even though
    /// permanent payloads do not participate in the mark worklist.
    pub(crate) fn permanent_vtables(&self) -> Vec<Oop> {
        self.blocks
            .values()
            .filter(|block| block.permanence == Permanence::Permanent)
            .map(|block| {
                // SAFETY: every registered block owns one initialized vtable
                // header immediately before its payload.
                unsafe { vtable_slot_from_payload(block.payload()).read() }
            })
            .filter(|vtable| !vtable.is_null())
            .collect()
    }

    pub(crate) fn is_marked(&self, object: Oop) -> Option<bool> {
        self.blocks.get(&object.addr()).map(|block| block.marked)
    }

    /// Mark exactly one registered collectable payload address. Interior,
    /// header, foreign, null, and permanent addresses are rejected.
    pub(crate) fn mark_collectable_address(&mut self, address: usize) -> bool {
        let Some(block) = self.blocks.get_mut(&address) else {
            return false;
        };
        if block.permanence == Permanence::Permanent || block.marked {
            return false;
        }
        block.marked = true;
        true
    }

    /// Copy only the coordinates of one exact live payload for allocation-free
    /// GC scanning. The range intentionally does not borrow `Heap`, because
    /// marking must mutate registry metadata while the separate payload
    /// allocation remains stable.
    pub(crate) fn payload_words(&self, address: usize) -> Option<PayloadWords> {
        let block = self.blocks.get(&address)?;
        let payload_size = block.total_size - VTABLE_HEADER_SIZE;
        debug_assert_eq!(payload_size % size_of::<usize>(), 0);
        Some(PayloadWords {
            pointer: block.payload().as_ptr().cast_const().cast(),
            length: payload_size / size_of::<usize>(),
        })
    }

    pub(crate) fn read_payload_word(&self, object: Oop, index: usize) -> Option<usize> {
        let words = self.payload_words(object.addr())?;
        if index >= words.len() {
            return None;
        }
        // SAFETY: exact membership keeps the block live, and the bounds check
        // above confines the read to its pointer-sized payload.
        Some(unsafe { words.read(index) })
    }

    pub(crate) fn write_payload_word(&mut self, object: Oop, index: usize, value: usize) -> bool {
        let Some(block) = self.blocks.get(&object.addr()) else {
            return false;
        };
        let payload_size = block.total_size - VTABLE_HEADER_SIZE;
        let Some(offset) = index.checked_mul(size_of::<usize>()) else {
            return false;
        };
        if offset >= payload_size {
            return false;
        }
        // SAFETY: exact registry membership retains allocation provenance;
        // offset was checked to name one complete aligned payload word.
        unsafe {
            block
                .payload()
                .as_ptr()
                .add(offset)
                .cast::<usize>()
                .write(value)
        };
        true
    }

    pub(crate) fn clear_marks(&mut self) {
        for block in self.blocks.values_mut() {
            block.marked = false;
        }
    }

    /// Snapshot unreachable collectable identities before sweep mutates the
    /// registry. Permanent blocks are never candidates.
    pub(crate) fn dead_collectable_addresses(&self) -> Vec<usize> {
        self.blocks
            .iter()
            .filter_map(|(address, block)| {
                (block.permanence == Permanence::Collectable && !block.marked).then_some(*address)
            })
            .collect()
    }

    /// Remove one previously identified dead block while leaving its allocation
    /// live for a finalizer. Marked and permanent blocks cannot be detached.
    pub(crate) fn detach_dead_collectable(&mut self, address: usize) -> Option<DetachedBlock> {
        let block = self.blocks.get(&address)?;
        if block.permanence != Permanence::Collectable || block.marked {
            return None;
        }
        let block = self
            .blocks
            .remove(&address)
            .unwrap_or_else(|| boundary::fatal_invariant("dead GC block disappeared"));
        Some(DetachedBlock { block })
    }

    pub(crate) fn has_finalizer(&self, object: Oop) -> Option<bool> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.finalizer.is_some())
    }

    pub(crate) fn set_finalizer(&mut self, object: Oop, finalizer: Finalizer) -> bool {
        let Some(block) = self.blocks.get_mut(&object.addr()) else {
            return false;
        };
        if block.permanence == Permanence::Permanent {
            return false;
        }
        block.finalizer = Some(finalizer);
        true
    }

    pub(crate) fn set_vtable(&mut self, object: Oop, vtable: Oop) -> bool {
        if !self.blocks.contains_key(&object.addr()) {
            return false;
        }
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this Heap.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this Heap.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(slot.addr(), object.addr() - VTABLE_HEADER_SIZE);
        // SAFETY: slot addresses the live, pointer-aligned header word.
        unsafe { slot.write(vtable) };
        true
    }

    /// Shallow-copy one exact live object's padded payload into a fresh
    /// collectable block with the same vtable. Finalizer metadata is deliberately
    /// not copied; resource-owning families must override clone and install fresh
    /// ownership metadata.
    pub(crate) fn clone_collectable(&mut self, object: Oop) -> Option<Oop> {
        let source = self.blocks.get(&object.addr())?;
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
        self.blocks.get(&object.addr())?;
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this Heap.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this Heap.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(slot.addr(), object.addr() - VTABLE_HEADER_SIZE);
        // SAFETY: slot addresses the initialized header word. alloc_zeroed
        // makes an unset header null.
        Some(unsafe { slot.read() })
    }

    /// Read the header on the dispatch hot path after the caller has already
    /// established that `object` is a live floor object.
    ///
    /// # Safety
    ///
    /// `object` must be the exact non-null payload address of a live block.
    pub(crate) unsafe fn vtable_of_valid_object(object: Oop) -> Oop {
        // SAFETY: the caller provides exact live-object provenance.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: the same provenance establishes the preceding header word.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        // SAFETY: the live block's header is initialized (possibly to null).
        unsafe { slot.read() }
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

impl Drop for Heap {
    fn drop(&mut self) {
        // Harness/runtime-owner teardown only: this is not language GC and does
        // not invoke finalizers. The process-lifetime runtime does not drop
        // while Birnam objects remain observable.
        for block in self.blocks.values() {
            let layout = Layout::from_size_align(block.total_size, BLOCK_ALIGNMENT)
                .expect("stored block layout must remain valid");
            // SAFETY: each stored base was returned by alloc_zeroed with this exact
            // layout, remains live, and is deallocated exactly once here.
            unsafe { dealloc(block.base.as_ptr(), layout) };
        }
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
        // Metadata stays within four words despite preserving finalizers and
        // exact membership. The former redundant payload pointer cost five.
        assert!(size_of::<Block>() <= 4 * size_of::<usize>());
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
