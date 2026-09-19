//! Shared lazy Error factory dispatch and exact recognition (81AM).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::ApiV1;
use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
static READY: AtomicBool = AtomicBool::new(false);

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn is_error(api: &IdApi, value: Oop) -> bool {
    if value.is_null() || unsafe { (api.isExported)(c"ErrorVt".as_ptr()) } == 0 {
        return false;
    }
    let error_vt = unsafe { (api.import)(c"ErrorVt".as_ptr()) };
    !error_vt.is_null() && unsafe { (api.vtableOf)(value) } == error_vt
}

unsafe fn make_error(api: &IdApi, kind: Oop, message: Oop) -> Oop {
    // Error owns its representation and Object protocol in the standalone
    // Error rung. Prelude faults are intentionally lazy: before that rung is
    // loaded, the floor has no catchable Error value and answers nil, matching
    // the native prelude's `Error` import behavior.
    if unsafe { (api.isExported)(c"Error".as_ptr()) } == 0 {
        return api.nilObj;
    }
    let factory = unsafe { (api.import)(c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let args = [kind, message];
    // SAFETY: the standalone Error factory owns this selector and the stack
    // argument array remains live for the duration of the synchronous send.
    unsafe { (api.send)(factory, selector, args.as_ptr(), args.len()) }
}

unsafe fn numeric_error(api: &IdApi, kind: &[u8], message: &[u8]) -> Oop {
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    unsafe { make_error(api, kind, message) }
}

unsafe extern "C" fn exact(api: *const IdApi, value: Oop) -> c_int {
    assert!(!api.is_null(), "Error value received a null IdApi");
    c_int::from(unsafe { is_error(&*api, value) })
}
unsafe extern "C" fn from_bytes(
    api: *const IdApi,
    kind: *const u8,
    kind_length: usize,
    message: *const u8,
    message_length: usize,
) -> Oop {
    assert!(!api.is_null(), "Error value received a null IdApi");
    // The caller supplies the same valid borrowed slices used by the original
    // helper, including a terminated kind. Reconstruct without scanning/copying.
    unsafe {
        numeric_error(
            &*api,
            core::slice::from_raw_parts(kind, kind_length),
            core::slice::from_raw_parts(message, message_length),
        )
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    is_error: Some(exact),
    from_bytes: Some(from_bytes),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void) -> *mut c_void {
    assert!(!api.is_null(), "Error value received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_error_value_provider_v1() -> *const ApiV1 {
    if READY.load(Ordering::Acquire) {
        &API
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
birnam_abi::string_dependency!();
