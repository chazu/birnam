//! Id object allocation and identities. Allocation works before GC or any
//! prepared runtime service exists.
use core::ptr::NonNull;
use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};

use crate::{abi::Oop, boundary, raw::AddressMap};

// Allocation geometry is private to Id; other modules query Id by message.
const BLOCK_ALIGNMENT: usize = 16;
const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();

// Storage layout is independent of the object's replaceable vtable. The low
// three bits of the rounded extent are free, so this adds no allocation-record
// field and does not reduce the allocation identity range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Storage {
    Opaque = 0,
    Name = 1,
    Method = 2,
    Fault = 3,
    References = 4,
}

/// Owns exactly one allocation, including when detached from the registry.
/// Dropping it releases storage and never invokes a language callback.
pub(crate) struct Allocation {
    pub(crate) base: NonNull<u8>,
    extent_and_storage: usize,
    // Preserve the existing identity bound and requested-size geometry. Low
    // two bits are unused here; collection state belongs to the upper owner.
    pub(crate) geometry: u64,
}

impl Allocation {
    pub(crate) fn object(&self) -> Oop {
        let object = self.payload().as_ptr().cast();
        if self.storage() == Storage::Fault {
            crate::abi::fault_reference(object)
        } else {
            object
        }
    }
    pub(crate) fn storage(&self) -> Storage {
        match self.extent_and_storage & 7 {
            0 => Storage::Opaque,
            1 => Storage::Name,
            2 => Storage::Method,
            3 => Storage::Fault,
            4 => Storage::References,
            _ => unreachable!("owned storage kind"),
        }
    }
    fn total_size(&self) -> usize {
        self.extent_and_storage & !7
    }
    pub(crate) fn identity(&self) -> u64 {
        self.geometry >> 5
    }
    pub(crate) fn requested_size(&self) -> usize {
        self.total_size() - VTABLE_HEADER_SIZE - ((self.geometry >> 2) & 7) as usize
    }
    pub(crate) fn payload(&self) -> NonNull<u8> {
        // SAFETY: this owner retains the nonempty header allocation.
        unsafe { NonNull::new_unchecked(self.base.as_ptr().add(VTABLE_HEADER_SIZE)) }
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.total_size(), BLOCK_ALIGNMENT)
            .expect("stored block layout must remain valid");
        // SAFETY: allocation and detachment transfer this sole owner, retaining
        // the exact layout originally passed to alloc_zeroed.
        unsafe { dealloc(self.base.as_ptr(), layout) };
    }
}

pub(crate) struct ObjectMemory {
    allocations: AddressMap<Allocation>,
    next_identity: u64,
}

impl ObjectMemory {
    pub(crate) fn new() -> Self {
        Self {
            allocations: AddressMap::default(),
            next_identity: 0,
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
            extent_and_storage: total_size,
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
    // Called only after the owner has initialized the payload, before it can
    // escape or any message can reenter the runtime.
    pub(crate) fn set_storage(&mut self, object: Oop, storage: Storage) {
        let record = self
            .allocations
            .get_mut(&crate::abi::heap_address(object))
            .expect("owned storage");
        assert_eq!(
            record.storage(),
            Storage::Opaque,
            "storage layout is immutable"
        );
        record.extent_and_storage |= storage as usize;
    }
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
    fn storage_layout_does_not_change_extents_identities_or_record_size() {
        assert_eq!(size_of::<Allocation>(), 24);
        let mut heap = ObjectMemory::new();
        for bytes in 0..128 {
            let object = heap.allocate(bytes);
            let identity = heap.get(object.addr()).unwrap().identity();
            heap.set_storage(object, Storage::Name);
            let record = heap.get(object.addr()).unwrap();
            assert_eq!(record.requested_size(), bytes);
            assert_eq!(record.identity(), identity);
            assert_eq!(record.object(), object);
            assert_eq!(record.storage(), Storage::Name);
        }
    }
}
