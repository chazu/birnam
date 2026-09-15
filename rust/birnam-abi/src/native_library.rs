//! Stateless acquisition of the native image containing a supplied address.
//! Callers own caching, symbol lookup, error adaptation and handle retention.
use crate::IdApi;
use core::ffi::{c_char, c_int, c_void};
use core::ptr;

#[repr(C)]
struct DlInfo {
    filename: *const c_char,
    base: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}

#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
}

/// Acquire a local loader reference to the image owning `address`.
/// A null result denotes failed address discovery or acquisition. This helper
/// neither caches nor closes the returned handle.
///
/// # Safety
/// `api` must be the live process API and `address` must identify the intended
/// loaded image. The caller must keep that image mapped through acquisition.
pub unsafe fn open_containing(api: &IdApi, address: *const c_void) -> *mut c_void {
    let mut info = DlInfo {
        filename: ptr::null(),
        base: ptr::null_mut(),
        symbol_name: ptr::null(),
        symbol_address: ptr::null_mut(),
    };
    if unsafe { dladdr(address, &mut info) } == 0 || info.filename.is_null() {
        return ptr::null_mut();
    }
    #[cfg(target_os = "macos")]
    const RTLD_NOW_LOCAL: c_int = 2 | 4;
    #[cfg(not(target_os = "macos"))]
    const RTLD_NOW_LOCAL: c_int = 2;
    unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) }
}
