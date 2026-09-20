//! System's existing direct native-module loader, installed by Prelude (81AU).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::numeric_error;
use birnam_abi::native_loader::ApiV3;
use birnam_abi::text_view::string_bytes;
use birnam_abi::{IdApi, ModuleInitFn, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first native loader installation. The host pins it.
#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    presence: usize,
    imports: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("Native Loader method was not installed")
}
unsafe fn api_ref() -> &'static IdApi {
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe extern "C" fn system_load_module(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const RTLD_LAZY: i32 = 1;
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(path_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, path_object) }) else {
        return api.nilObj;
    };
    let Ok(path) = CString::new(path_bytes) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not load module dylib") };
    };
    let handle = unsafe { (api.dlopen)(path.as_ptr(), RTLD_LAZY) };
    if handle.is_null() {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not load module dylib") };
    }
    let symbol = unsafe { (api.dlsym)(handle, c"module_init".as_ptr()) };
    if symbol.is_null() {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"notAModule\0", b"dylib has no module_init") };
    }
    // SAFETY: dlsym returned the module_init symbol from a live handle; all
    // Birnam module initializers share this frozen C ABI and receive the
    // process-owned IdApi with no state payload for this loader path.
    let init: ModuleInitFn = unsafe { core::mem::transmute(symbol) };
    unsafe { init(runtime().api as *mut IdApi, ptr::null_mut(), execution_presence(), execution_imports()) };
    self_object
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    system_vt: Oop,
    load_module: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Native Loader installation"
    );
    RUNTIME.get_or_init(|| Runtime { api: execution as usize, presence: presence as usize, imports: imports as usize });
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, load_module, system_load_module);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Native Loader received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_native_loader_provider_v3() -> *const ApiV3 {
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
birnam_abi::error_value_dependency!();
birnam_abi::text_view_dependency!();

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { runtime().presence as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { runtime().imports as *const birnam_abi::namespace_import::ApiV1 }
