//! Completed checked language-send entry, selected during upper construction.
use crate::{IdApi, Oop};
pub type Send = unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop;
/// # Safety
/// `api` belongs to the selected live Runtime; its image remains loaded.
pub unsafe fn select(api: &IdApi) -> Option<Send> {
    let handle = unsafe {
        crate::private::native_library::open_containing(api, api.dlsym as *const () as _)
    };
    if handle.is_null() {
        return None;
    }
    let address = unsafe { (api.dlsym)(handle, c"birnam_runtime_send_v1".as_ptr()) };
    if address.is_null() {
        None
    } else {
        Some(unsafe { core::mem::transmute(address) })
    }
}
