//! OS mapping only. Policy, Oop construction and initialization are Id messages.
use core::ffi::{c_char, c_int, c_void};
use std::ffi::CString;
#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dlopen(p: *const c_char, f: c_int) -> *mut c_void;
    fn dlsym(h: *mut c_void, n: *const c_char) -> *mut c_void;
    fn dlclose(h: *mut c_void) -> c_int;
}
pub unsafe fn open(path: &[u8]) -> Option<*mut c_void> {
    let path = CString::new(path).ok()?;
    if path.as_bytes().is_empty() {
        return None;
    }
    let p = unsafe {
        dlopen(
            path.as_ptr(),
            2 | if cfg!(target_os = "macos") { 4 } else { 0 },
        )
    };
    (!p.is_null()).then_some(p)
}
pub unsafe fn symbol(h: *mut c_void, n: &std::ffi::CStr) -> *mut c_void {
    unsafe { dlsym(h, n.as_ptr()) }
}
pub unsafe fn close(h: *mut c_void) {
    unsafe { dlclose(h) };
}
