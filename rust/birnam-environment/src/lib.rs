//! System's existing environment getter, installed by Prelude (81AR).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::environment::ApiV3;
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
static EXECUTION_API: OnceLock<(usize, usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("Environment methods were not installed");
    unsafe { &*(pointer.0 as *const IdApi) }
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
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read environment") };
    };
    match env::var(name) {
        Ok(value) => unsafe { (birnam_abi::string::make)(value.as_ptr(), value.len()) },
        Err(env::VarError::NotPresent) => api.nilObj,
        Err(env::VarError::NotUnicode(_)) => unsafe {
            numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read environment")
        },
    }
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    system_vt: Oop,
    get_env: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Environment installation"
    );
    EXECUTION_API.get_or_init(|| (execution as usize, presence as usize, imports as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, get_env, system_get_env);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Environment received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_environment_provider_v3() -> *const ApiV3 {
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

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { EXECUTION_API.get().expect("methods not installed").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { EXECUTION_API.get().expect("module not installed").2 as *const birnam_abi::namespace_import::ApiV1 }
