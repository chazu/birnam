//! Existing native object geometry shared by Id, generated code and upper
//! storage clients. This exposes no registry, ownership or collector policy.
use crate::Oop;

pub const BLOCK_ALIGNMENT: usize = 16;
pub const VTABLE_HEADER_SIZE: usize = size_of::<Oop>();
pub const PAYLOAD_ALIGNMENT: usize = align_of::<Oop>();
const _: () = assert!(PAYLOAD_ALIGNMENT == 8);

pub fn allocation_total_size(payload_size: usize) -> Option<usize> {
    VTABLE_HEADER_SIZE
        .checked_add(payload_size)?
        .checked_add(PAYLOAD_ALIGNMENT - 1)
        .map(|size| size & !(PAYLOAD_ALIGNMENT - 1))
}

pub fn valid_payload_size(size: usize) -> bool {
    allocation_total_size(size)
        .is_some_and(|total| std::alloc::Layout::from_size_align(total, BLOCK_ALIGNMENT).is_ok())
}

/// Read the existing preceding behavior word; never performs membership checks.
/// # Safety
/// `object` is the exact non-null payload pointer of a retained live Id object.
pub unsafe fn behavior_of_live_object(object: Oop) -> Oop {
    unsafe {
        object
            .cast::<u8>()
            .sub(VTABLE_HEADER_SIZE)
            .cast::<Oop>()
            .read()
    }
}
