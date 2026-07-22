//! Raw aligned object blocks and their exact-address registry.
//!
//! R2.1 lands this private mechanism before runtime bootstrap wires it in.
#![allow(dead_code)]

use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};
use std::collections::HashMap;
use std::ptr::NonNull;

use crate::abi::{Finalizer, Oop};
use crate::boundary;

pub(crate) const BLOCK_ALIGNMENT: usize = 16;
pub(crate) const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();
pub(crate) const PAYLOAD_ALIGNMENT: usize = align_of::<Oop>();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Permanence {
    Collectable,
    Permanent,
}

struct Block {
    base: NonNull<u8>,
    payload: NonNull<u8>,
    total_size: usize,
    permanence: Permanence,
    marked: bool,
    finalizer: Option<Finalizer>,
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
    blocks: HashMap<usize, Block>,
}

impl Heap {
    pub(crate) fn new() -> Self {
        Self {
            blocks: HashMap::new(),
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
                payload,
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
        payload.as_ptr().cast()
    }

    pub(crate) fn contains(&self, object: Oop) -> bool {
        !object.is_null() && self.blocks.contains_key(&object.addr())
    }

    pub(crate) fn allocation_count(&self) -> usize {
        self.blocks.len()
    }

    pub(crate) fn base_address(&self, object: Oop) -> Option<usize> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.base.as_ptr().addr())
    }

    pub(crate) fn payload_address(&self, object: Oop) -> Option<usize> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.payload.as_ptr().addr())
    }

    /// Report the padded payload extent, matching Zig `objectPayloadSize`.
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

    pub(crate) fn is_marked(&self, object: Oop) -> Option<bool> {
        self.blocks.get(&object.addr()).map(|block| block.marked)
    }

    pub(crate) fn has_finalizer(&self, object: Oop) -> Option<bool> {
        self.blocks
            .get(&object.addr())
            .map(|block| block.finalizer.is_some())
    }

    pub(crate) fn set_vtable(&mut self, object: Oop, vtable: Oop) -> bool {
        let Some(block) = self.blocks.get(&object.addr()) else {
            return false;
        };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this Heap.
        let slot = unsafe { vtable_slot_from_payload(block.payload) };
        debug_assert_eq!(slot.addr(), block.base.as_ptr().addr());
        // SAFETY: slot addresses the live, pointer-aligned header word.
        unsafe { slot.write(vtable) };
        true
    }

    pub(crate) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        let block = self.blocks.get(&object.addr())?;
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this Heap.
        let slot = unsafe { vtable_slot_from_payload(block.payload) };
        debug_assert_eq!(slot.addr(), block.base.as_ptr().addr());
        // SAFETY: slot addresses the initialized header word. alloc_zeroed
        // makes an unset header null.
        Some(unsafe { slot.read() })
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
            // SAFETY: each base was returned by alloc_zeroed with this exact
            // layout, remains live, and is deallocated exactly once here.
            unsafe { dealloc(block.base.as_ptr(), layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;

    fn fake_vtable() -> Oop {
        static MARKER: u64 = 0;
        core::ptr::addr_of!(MARKER).cast_mut().cast::<c_void>()
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
    fn blocks_have_exact_base_header_and_payload_geometry() {
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
}
