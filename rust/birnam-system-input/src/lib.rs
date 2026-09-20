//! System's existing stdin method, installed by Prelude (81AQ).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::system_input::ApiV3;
use birnam_abi::error_value::numeric_error;
use birnam_abi::{IdApi, Oop};
use core::ffi::c_void;
use core::ptr;
use std::io::{self, BufRead};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first input installation. The host pins it.
static EXECUTION_API: OnceLock<(usize, usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("System input methods were not installed");
    unsafe { &*(pointer.0 as *const IdApi) }
}
unsafe extern "C" fn system_read_line(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut line = Vec::new();
    match stdin.read_until(b'\n', &mut line) {
        Ok(0) => api.nilObj,
        Ok(_) => {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            unsafe { (birnam_abi::string::make)(line.as_ptr(), line.len()) }
        }
        Err(_) => unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"stdin read failed") },
    }
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    system_vt: Oop,
    read_line: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid System input installation"
    );
    EXECUTION_API.get_or_init(|| (execution as usize, presence as usize, imports as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, read_line, system_read_line);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "System input received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_system_input_provider_v3() -> *const ApiV3 {
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

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { EXECUTION_API.get().expect("methods not installed").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { EXECUTION_API.get().expect("module not installed").2 as *const birnam_abi::namespace_import::ApiV1 }
