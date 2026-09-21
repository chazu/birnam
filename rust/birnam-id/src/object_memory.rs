//! Id object allocation and identities. Allocation works before GC or any
//! prepared runtime service exists.
use core::ptr::NonNull;
use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};

use crate::{abi::Oop, boundary, raw::AddressMap};

pub(crate) const BLOCK_ALIGNMENT: usize = 16;
pub(crate) const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();
pub(crate) const PAYLOAD_ALIGNMENT: usize = align_of::<Oop>();
const _: () = assert!(PAYLOAD_ALIGNMENT == 8);

/// Owns exactly one allocation, including when detached from the registry.
/// Dropping it releases storage and never invokes a language callback.
pub(crate) struct Allocation {
    pub(crate) base: NonNull<u8>,
    pub(crate) total_size: usize,
    // Preserve the existing identity bound and requested-size geometry. Low
    // two bits are unused here; collection state belongs to the upper owner.
    pub(crate) geometry: u64,
}

impl Allocation {
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
}

impl ObjectMemory {
    pub(crate) fn new() -> Self {
        Self {
            allocations: AddressMap::default(),
            next_identity: 0,
        }
    }

    pub(crate) fn allocation_total_size(payload_size: usize) -> Option<usize> {
        VTABLE_HEADER_SIZE
            .checked_add(payload_size)?
            .checked_add(PAYLOAD_ALIGNMENT - 1)
            .map(|size| size & !(PAYLOAD_ALIGNMENT - 1))
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
        self.allocations.get(&address)
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, &Allocation)> {
        self.allocations
            .iter()
            .map(|(address, allocation)| (*address, allocation))
    }
    pub(crate) fn len(&self) -> usize {
        self.allocations.len()
    }
    pub(crate) fn detach(&mut self, address: usize) -> Option<Allocation> {
        self.allocations.remove(&address)
    }
    #[cfg(test)]
    pub(crate) fn last_identity(&self) -> u64 {
        self.next_identity
    }
    #[cfg(test)]
    pub(crate) fn set_last_identity(&mut self, identity: u64) {
        self.next_identity = identity;
    }
}

/// # Safety
/// `payload` is the exact payload start of a live Id allocation.
pub(crate) unsafe fn vtable_slot_from_payload(payload: NonNull<u8>) -> *mut Oop {
    // SAFETY: the caller supplies the allocation's live payload provenance.
    unsafe { payload.as_ptr().sub(VTABLE_HEADER_SIZE).cast() }
}

/// Copied coordinates for scanning one live object's pointer-sized payload
/// words without allocating a per-object snapshot.
///
/// This deliberately carries no Rust borrow of `ObjectMemory`: the collector must
/// mutate mark metadata while reading the separately allocated object payload.
#[derive(Clone, Copy)]
pub(crate) struct PayloadWords {
    pointer: *const usize,
    length: usize,
}

impl PayloadWords {
    /// # Safety
    /// The caller retains this exact aligned live payload and its complete extent.
    pub(crate) unsafe fn from_raw_parts(pointer: *const usize, length: usize) -> Self {
        Self { pointer, length }
    }
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

impl ObjectMemory {
    pub(crate) fn write_payload_word(&mut self, object: Oop, index: usize, value: usize) -> bool {
        let Some(block) = self.get(object.addr()) else {
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

    pub(crate) fn set_vtable(&mut self, object: Oop, vtable: Oop) -> bool {
        if self.get(object.addr()).is_none() {
            return false;
        }
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this ObjectMemory.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this ObjectMemory.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(slot.addr(), object.addr() - VTABLE_HEADER_SIZE);
        // SAFETY: slot addresses the live, pointer-aligned header word.
        unsafe { slot.write(vtable) };
        true
    }

    pub(crate) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        self.get(object.addr())?;
        // SAFETY: exact registry membership proves this non-null address is the
        // payload start of the live floor allocation owned by this ObjectMemory.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: exact registry membership proves that payload is the start of
        // the live floor allocation owned by this ObjectMemory.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        debug_assert_eq!(slot.addr(), object.addr() - VTABLE_HEADER_SIZE);
        // SAFETY: slot addresses the initialized header word. alloc_zeroed
        // makes an unset header null.
        Some(unsafe { slot.read() })
    }

    pub(crate) unsafe fn vtable_of_valid_object(object: Oop) -> Oop {
        // SAFETY: the caller provides exact live-object provenance.
        let payload = unsafe { NonNull::new_unchecked(object.cast::<u8>()) };
        // SAFETY: the same provenance establishes the preceding header word.
        let slot = unsafe { vtable_slot_from_payload(payload) };
        // SAFETY: the live block's header is initialized (possibly to null).
        unsafe { slot.read() }
    }
}
