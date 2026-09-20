//! System's existing GC control methods, installed by Prelude (81AJ).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::system_gc::ApiV1;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first GC control installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("System GC control methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe extern "C" fn system_gc(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)((api.gcCollect)() as i64) }
}

unsafe extern "C" fn system_live_objects(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)((api.gcLiveCount)() as i64) }
}

unsafe extern "C" fn system_auto_gc(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if value != api.trueObj && value != api.falseObj {
        return api.nilObj;
    }
    unsafe { (api.gcSetAuto)(if value == api.trueObj { 1 } else { 0 }) };
    self_object
}

unsafe extern "C" fn system_precise_gc(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if value != api.trueObj && value != api.falseObj {
        return api.nilObj;
    }
    unsafe { (api.gcSetPrecise)(if value == api.trueObj { 1 } else { 0 }) };
    self_object
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    system_vt: Oop,
    gc: Oop,
    live_objects: Oop,
    auto_gc: Oop,
    precise_gc: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid System GC installation"
    );
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, gc, system_gc);
        (api.method)(system_vt, live_objects, system_live_objects);
        (api.method)(system_vt, auto_gc, system_auto_gc);
        (api.method)(system_vt, precise_gc, system_precise_gc);
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "System GC received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_system_gc_provider_v1() -> *const ApiV1 {
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
birnam_abi::integer_dependency!();
