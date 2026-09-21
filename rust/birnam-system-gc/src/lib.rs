//! System's existing GC control methods, constructed through Bootstrap (81AJ).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Construction supplies the original selection, even when registration reenters
// with another table before the first GC control installation. The host pins it.
static EXECUTION_API: OnceLock<(usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let (pointer, _) = *EXECUTION_API
        .get()
        .expect("System GC control methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
fn execution_settings() -> *const birnam_abi::gc_settings::ApiV1 { EXECUTION_API.get().expect("System GC control methods were not installed").1 as *const birnam_abi::gc_settings::ApiV1 }
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
    unsafe { birnam_abi::gc_settings::set_auto(execution_settings(), if value == api.trueObj { 1 } else { 0 }) };
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
    unsafe { birnam_abi::gc_settings::set_precise(execution_settings(), if value == api.trueObj { 1 } else { 0 }) };
    self_object
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    settings: *const birnam_abi::gc_settings::ApiV1,
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
    EXECUTION_API.get_or_init(|| (execution as usize, settings as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, gc, system_gc);
        (api.method)(system_vt, live_objects, system_live_objects);
        (api.method)(system_vt, auto_gc, system_auto_gc);
        (api.method)(system_vt, precise_gc, system_precise_gc);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

// The native constructor returns only after the System methods are installed.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static NATIVE_SELECTION: OnceLock<(usize, usize)> = OnceLock::new();
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, roots, system, settings, integer]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let Some(api) = api.as_word() else { return invalid_native(); };
    let Some(roots) = roots.as_word() else { return invalid_native(); };
    let Some(settings) = settings.as_word() else { return invalid_native(); };
    let Some(integer) = integer.as_word() else { return invalid_native(); };
    let api = api as *const IdApi;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let Some(system) = system.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver || !birnam_abi::root_registry::valid_owned(roots) } { return invalid_native(); }
    let settings = settings as *const birnam_abi::gc_settings::ApiV1;
    if !unsafe { birnam_abi::gc_settings::valid(settings) } { return invalid_native(); }
    if unsafe { birnam_abi::integer::attach(integer as _) } != 1 { return invalid_native(); }
    if NATIVE_SELECTION.get().is_some_and(|selected| *selected != (api.addr(), system.addr())) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::unit()), _ => return invalid_native(),
    }
    assert!(NATIVE_SELECTION.set((api.addr(), system.addr())).is_ok());
    READY.store(true, Ordering::Release);
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let system_vt = unsafe { ((*api).vtableOf)(system) };
    let s0 = unsafe { retained.keep(((*api).intern)(c"gc".as_ptr())) };
    let s1 = unsafe { retained.keep(((*api).intern)(c"liveObjects".as_ptr())) };
    let s2 = unsafe { retained.keep(((*api).intern)(c"autoGc:".as_ptr())) };
    let s3 = unsafe { retained.keep(((*api).intern)(c"preciseGc:".as_ptr())) };
    unsafe { install(api, api, settings, system_vt, s0, s1, s2, s3) };
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::unit())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
