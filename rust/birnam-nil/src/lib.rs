//! Existing Object/nil protocol, independently loaded from Collection (81V).
//! Singleton representation remains in Id for this checkpoint.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

static API: OnceLock<usize> = OnceLock::new();
fn abort(message: &str) -> ! {
    eprintln!("birnam nil: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let api = *API.get().unwrap_or_else(|| abort("module_init was not called"));
    // The host pins this module and the selected floor table for process life.
    unsafe { &*(api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // Callers supply static NUL-terminated selectors.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe extern "C" fn object_is_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.falseObj } else { api.nilObj }
}

unsafe extern "C" fn object_not_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.trueObj } else { api.nilObj }
}

unsafe extern "C" fn nil_is_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.trueObj } else { api.nilObj }
}

unsafe extern "C" fn nil_not_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.falseObj } else { api.nilObj }
}

unsafe extern "C" fn nil_print_string(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 {
        unsafe { (birnam_abi::string::make)(b"nil".as_ptr(), 3) }
    } else {
        api.nilObj
    }
}

/// Install the existing Object/nil protocol on the currently selected behaviors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = API.get_or_init(|| api_pointer as usize);
    unsafe {
        (api.method)(api.object_vt, intern(api, b"isNil\0"), object_is_nil);
        (api.method)(api.object_vt, intern(api, b"notNil\0"), object_not_nil);
        let nil_vt = (api.vtableOf)(api.nilObj);
        (api.method)(nil_vt, intern(api, b"isNil\0"), nil_is_nil);
        (api.method)(nil_vt, intern(api, b"notNil\0"), nil_not_nil);
        (api.method)(nil_vt, intern(api, b"printString\0"), nil_print_string);
    }
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::string_dependency!();
