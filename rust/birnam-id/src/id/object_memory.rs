//! Id object allocation and identities. Allocation works before GC or any
//! prepared runtime service exists.
use core::ptr::NonNull;
use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};

use crate::{abi::Oop, boundary, raw::AddressMap};

// Allocation geometry is private to Id; other modules query Id by message.
const BLOCK_ALIGNMENT: usize = 16;
const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();

/// Owns exactly one allocation, including when detached from the registry.
/// Dropping it releases storage and never invokes a language callback.
pub(crate) struct Allocation {
    pub(crate) base: NonNull<u8>,
    total_size: usize,
    // Preserve the existing identity bound and requested-size geometry. Low
    // two bits are unused; GC state belongs to its owner.
    pub(crate) geometry: u64,
}

impl Allocation {
    pub(crate) fn object(&self) -> Oop {
        self.payload().as_ptr().cast()
    }
    pub(crate) fn identity(&self) -> u64 {
        self.geometry >> 5
    }
    pub(crate) fn requested_size(&self) -> usize {
        self.total_size - VTABLE_HEADER_SIZE - ((self.geometry >> 2) & 7) as usize
    }
    pub(crate) fn payload(&self) -> NonNull<u8> {
        // SAFETY: this owner retains the nonempty header allocation.
        unsafe { NonNull::new_unchecked(self.base.as_ptr().add(VTABLE_HEADER_SIZE)) }
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.total_size, BLOCK_ALIGNMENT)
            .expect("stored block layout must remain valid");
        // SAFETY: allocation and detachment transfer this sole owner, retaining
        // the exact layout originally passed to alloc_zeroed.
        unsafe { dealloc(self.base.as_ptr(), layout) };
    }
}

pub(crate) struct ObjectMemory {
    allocations: AddressMap<Allocation>,
    next_identity: u64,
    allocated_bytes: usize,
}

impl ObjectMemory {
    pub(crate) fn new() -> Self {
        Self {
            allocations: AddressMap::default(),
            next_identity: 0,
            allocated_bytes: 0,
        }
    }

    pub(crate) fn allocation_total_size(payload_size: usize) -> Option<usize> {
        let alignment = align_of::<Oop>();
        VTABLE_HEADER_SIZE
            .checked_add(payload_size)?
            .checked_add(alignment - 1)
            .map(|size| size & !(alignment - 1))
    }

    pub(crate) fn valid_payload_size(payload_size: usize) -> bool {
        Self::allocation_total_size(payload_size)
            .is_some_and(|total| Layout::from_size_align(total, BLOCK_ALIGNMENT).is_ok())
    }

    pub(crate) fn allocate(&mut self, payload_size: usize) -> Oop {
        let total_size = Self::allocation_total_size(payload_size)
            .unwrap_or_else(|| boundary::fatal_invariant("object allocation size overflow"));
        self.allocated_bytes = self.allocated_bytes.checked_add(total_size)
            .unwrap_or_else(|| boundary::fatal_invariant("allocation byte counter exhausted"));
        let layout = Layout::from_size_align(total_size, BLOCK_ALIGNMENT)
            .unwrap_or_else(|_| boundary::fatal_invariant("invalid object allocation layout"));
        // SAFETY: header size is nonzero and alignment is valid.
        let base = NonNull::new(unsafe { alloc_zeroed(layout) })
            .unwrap_or_else(|| handle_alloc_error(layout));
        self.next_identity = self
            .next_identity
            .checked_add(1)
            .filter(|identity| *identity <= u64::MAX >> 5)
            .unwrap_or_else(|| boundary::fatal_invariant("allocation identity exhausted"));
        let allocation = Allocation {
            base,
            total_size,
            geometry: (self.next_identity << 5)
                | (((total_size - VTABLE_HEADER_SIZE - payload_size) as u64) << 2),
        };
        let object: Oop = allocation.payload().as_ptr().cast();
        let previous = self.allocations.insert(object.addr(), allocation);
        boundary::require_invariant(
            previous.is_none(),
            "allocator reused a live payload address",
        );
        object
    }

    pub(crate) fn get(&self, address: usize) -> Option<&Allocation> {
        self.allocations.get(&(address & !2))
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, &Allocation)> {
        self.allocations
            .iter()
            .map(|(address, allocation)| (*address, allocation))
    }
    pub(crate) fn allocated_bytes(&self) -> usize { self.allocated_bytes }
    pub(crate) fn allocation_epoch(&self) -> u64 {
        self.next_identity
    }
    pub(crate) fn len(&self) -> usize {
        self.allocations.len()
    }
    pub(crate) fn detach(&mut self, address: usize) -> Option<Allocation> {
        self.allocations.remove(&(address & !2))
    }
}

/// # Safety
/// `payload` is the exact payload start of a live Id allocation.
pub(crate) unsafe fn vtable_slot_from_payload(payload: NonNull<u8>) -> *mut Oop {
    // SAFETY: the caller supplies the allocation's live payload provenance.
    unsafe { payload.as_ptr().sub(VTABLE_HEADER_SIZE).cast() }
}

impl ObjectMemory {
    pub(crate) fn set_vtable(&mut self, object: Oop, vtable: Oop) -> bool {
        if self.get(object.addr()).is_none() {
            return false;
        }
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this ObjectMemory.
        let payload =
            unsafe { NonNull::new_unchecked(crate::abi::heap_pointer(object).cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this ObjectMemory.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(
            slot.addr(),
            crate::abi::heap_address(object) - VTABLE_HEADER_SIZE
        );
        // SAFETY: slot addresses the live, pointer-aligned header word.
        unsafe { slot.write(vtable) };
        true
    }

    pub(crate) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        self.get(object.addr())?;
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this ObjectMemory.
        let payload =
            unsafe { NonNull::new_unchecked(crate::abi::heap_pointer(object).cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this ObjectMemory.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(
            slot.addr(),
            crate::abi::heap_address(object) - VTABLE_HEADER_SIZE
        );
        // SAFETY: slot addresses the initialized header word. alloc_zeroed
        // makes an unset header null.
        Some(unsafe { slot.read() })
    }

    pub(crate) unsafe fn vtable_of_valid_object(object: Oop) -> Oop {
        // SAFETY: the caller provides exact live-object provenance.
        let payload =
            unsafe { NonNull::new_unchecked(crate::abi::heap_pointer(object).cast::<u8>()) };
        // SAFETY: the same provenance establishes the preceding header word.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        // SAFETY: the live block's header is initialized (possibly to null).
        unsafe { slot.read() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_bytes_include_headers_and_do_not_decrease_on_release() {
        let mut memory=ObjectMemory::new();
        let first=memory.allocate(3);let second=memory.allocate(24);
        let expected=ObjectMemory::allocation_total_size(3).unwrap()+ObjectMemory::allocation_total_size(24).unwrap();
        assert_eq!(memory.allocated_bytes(),expected);
        drop(memory.detach(first.addr()).unwrap());drop(memory.detach(second.addr()).unwrap());
        assert_eq!(memory.len(),0);assert_eq!(memory.allocated_bytes(),expected);
        memory.allocate(0);assert_eq!(memory.allocated_bytes(),expected+ObjectMemory::allocation_total_size(0).unwrap());
    }

    #[test]
    fn allocation_geometry_preserves_extent_identity_and_record_size() {
        assert_eq!(size_of::<Allocation>(), 24);
        let mut h = ObjectMemory::new();
        for n in 0..128 {
            let o = h.allocate(n);
            let id = h.get(o.addr()).unwrap().identity();
            let a = h.get(o.addr()).unwrap();
            assert_eq!(a.requested_size(), n);
            assert_eq!(a.identity(), id);
            assert_eq!(a.object(), o);
        }
    }
}
