//! Thin pass-through to the platform dynamic loader.

use core::ffi::{c_char, c_int, c_void};
mod block;
mod field;
mod numeric;
pub(crate) use block::BlockConstructionSummary;
pub(crate) use field::FieldSummary;
pub(crate) use numeric::NumericSummary;

unsafe extern "C" {
    #[link_name = "dlopen"]
    fn system_dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    #[link_name = "dlsym"]
    fn system_dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// # Safety
///
/// The path must satisfy the platform `dlopen` contract.
pub(crate) unsafe fn open(path: *const c_char, mode: c_int) -> *mut c_void {
    // SAFETY: the public IdApi caller supplies the platform contract unchanged.
    unsafe { system_dlopen(path, mode) }
}

/// # Safety
///
/// The handle and name must satisfy the platform `dlsym` contract.
pub(crate) unsafe fn symbol(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: the public IdApi caller supplies the platform contract unchanged.
    unsafe { system_dlsym(handle, name) }
}
