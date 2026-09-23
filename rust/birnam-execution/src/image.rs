//! Resident-image pinning using Runtime's retained Method lifetime algorithm.
use core::{
    ffi::{c_char, c_int, c_void},
    ptr::NonNull,
};

#[repr(C)]
#[derive(Default)]
struct DlInfo {
    filename: *const c_char,
    base: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}
unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}
pub(crate) struct Image(NonNull<c_void>);
impl Image {
    pub(crate) fn retain(address: *const c_void) -> Option<Self> {
        let mut owner = DlInfo::default();
        if unsafe { dladdr(address, &mut owner) } == 0 || owner.filename.is_null() {
            return None;
        }
        NonNull::new(unsafe { dlopen(owner.filename, 0x1 | 0x4 | 0x10) }).map(Self)
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            dlclose(self.0.as_ptr());
        }
    }
}
