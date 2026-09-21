//! Completed upper operations for generated native methods owned by an object.
use crate::{IdApi, Oop};
use core::ffi::c_void;
#[derive(Clone, Copy)]
pub struct Api {
    /// Register before publishing any binding. The owning object is live, and
    /// the function remains valid until that object's native finalization.
    pub register: unsafe extern "C" fn(*const c_void, Oop) -> i32,
    /// Forget weak native records before releasing the owner or reusing storage.
    pub forget: unsafe extern "C" fn(Oop),
}
/// # Safety
/// The selected Runtime and its code remain live for the consumer's lifetime.
pub unsafe fn select(api: &IdApi) -> Option<Api> {
    let image = unsafe {
        crate::private::native_library::open_containing(api, api.dlsym as *const () as _)
    };
    if image.is_null() {
        return None;
    }
    let register =
        unsafe { (api.dlsym)(image, c"birnam_runtime_register_method_owner_v1".as_ptr()) };
    let forget = unsafe { (api.dlsym)(image, c"birnam_runtime_forget_method_owner_v1".as_ptr()) };
    if register.is_null() || forget.is_null() {
        return None;
    }
    Some(Api {
        register: unsafe { core::mem::transmute(register) },
        forget: unsafe { core::mem::transmute(forget) },
    })
}
