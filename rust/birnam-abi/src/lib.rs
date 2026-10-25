//! Opaque object-only Id boundary. Native descriptor metadata is loader input,
//! never a second category of message argument or result.
use core::{ffi::c_void, mem::align_of};
pub type Oop = *mut c_void;
pub mod id_protocol;
pub mod native_module;
pub fn size(value: usize) -> Option<Oop> {
    (value <= usize::MAX >> 1).then(|| core::ptr::without_provenance_mut((value << 1) | 1))
}
pub fn as_size(value: Oop) -> Option<usize> {
    (value.addr() & 1 != 0).then(|| value.addr() >> 1)
}
pub fn is_fault(value: Oop) -> bool {
    value.addr() & 3 == 2
}
/// Remove a native return's failure marker when handling its object as data.
/// Immediate size Oops are unchanged.
pub fn object_reference(value: Oop) -> Oop {
    if is_fault(value) { heap_pointer(value) } else { value }
}
pub fn heap_address(value: Oop) -> usize {
    value.addr() & !2
}
pub fn heap_pointer(value: Oop) -> Oop {
    value.map_addr(|p| p & !2)
}
pub fn fault_reference(value: Oop) -> Oop {
    value.map_addr(|p| p | 2)
}
/// Caller guarantees readable storage for nonzero count; shape is checked here.
pub unsafe fn borrowed_slice<'a, T>(p: *const T, n: usize) -> Option<&'a [T]> {
    if n == 0 {
        Some(&[])
    } else if p.is_null() || p.addr() % align_of::<T>() != 0 {
        None
    } else {
        Some(unsafe { core::slice::from_raw_parts(p, n) })
    }
}

// Descriptive foreign/native signature bytes; these do not add Id value carriers.
pub mod native_signature;
