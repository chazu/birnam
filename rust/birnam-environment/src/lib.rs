//! System's existing environment getter, installed by Prelude (81AR).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::environment::ApiV1;
use birnam_abi::error_value::numeric_error;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use birnam_abi::text_view::string_bytes;
use core::ffi::c_void;
use core::ptr;
use std::{env, str};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first environment installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("Environment methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe extern "C" fn system_get_env(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(name_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(name_bytes) = (unsafe { string_bytes(api, name_object) }) else {
        return api.nilObj;
    };
    let Ok(name) = str::from_utf8(name_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read environment") };
    };
    match env::var(name) {
        Ok(value) => unsafe { (birnam_abi::string::make)(value.as_ptr(), value.len()) },
        Err(env::VarError::NotPresent) => api.nilObj,
        Err(env::VarError::NotUnicode(_)) => unsafe {
            numeric_error(api, b"ioError\0", b"could not read environment")
        },
    }
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    system_vt: Oop,
    get_env: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Environment installation"
    );
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, get_env, system_get_env);
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void) -> *mut c_void {
    assert!(!api.is_null(), "Environment received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_environment_provider_v1() -> *const ApiV1 {
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
birnam_abi::error_value_dependency!();
birnam_abi::text_view_dependency!();
