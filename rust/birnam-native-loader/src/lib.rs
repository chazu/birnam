//! System's existing direct native-module loader, constructed through Bootstrap (81AU).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::numeric_error;
use birnam_abi::native_loader::ApiV12;
use birnam_abi::text_view::string_bytes;
use birnam_abi::{IdApi, ModuleInitFn, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Construction supplies the original selection, even when registration reenters
// with another table before the first native loader installation. The host pins it.
#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    finalizers: usize,
    presence: usize,
    imports: usize,
    exports: usize,
    vtables: usize,
    observation: usize,
    roots: usize,
    settings: usize,
    layouts: usize,
    hooks: usize,
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
    unsafe { init(runtime().api as *mut IdApi, ptr::null_mut(), execution_presence(), execution_imports(), runtime().exports as *const birnam_abi::namespace_export::ApiV1, runtime().vtables as *const birnam_abi::vtable_enumeration::ApiV2, runtime().observation as *const birnam_abi::method_observation::ApiV1, runtime().roots as *const birnam_abi::root_registry::ApiV1, runtime().settings as *const birnam_abi::gc_settings::ApiV1, runtime().layouts as *const birnam_abi::payload_layout::ApiV1, runtime().hooks as *const birnam_abi::trace_hooks::ApiV1, runtime().finalizers as *const birnam_abi::finalizers::ApiV1) };
    self_object
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    exports: *const birnam_abi::namespace_export::ApiV1,
    vtables: *const birnam_abi::vtable_enumeration::ApiV2,
    observation: *const birnam_abi::method_observation::ApiV1,
    roots: *const birnam_abi::root_registry::ApiV1,
    settings: *const birnam_abi::gc_settings::ApiV1,
    layouts: *const birnam_abi::payload_layout::ApiV1,
    hooks: *const birnam_abi::trace_hooks::ApiV1,
    finalizers: *const birnam_abi::finalizers::ApiV1,
    system_vt: Oop,
    load_module: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Native Loader installation"
    );
    RUNTIME.get_or_init(|| Runtime { api: execution as usize, finalizers: finalizers as usize, presence: presence as usize, imports: imports as usize, exports: exports as usize, vtables: vtables as usize, observation: observation as usize, roots: roots as usize, settings: settings as usize, layouts: layouts as usize, hooks: hooks as usize });
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, load_module, system_load_module);
    }
}
static API: ApiV12 = ApiV12 {
    version: 12,
    byte_size: size_of::<ApiV12>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Native Loader received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_native_loader_provider_v12() -> *const ApiV12 {
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

// The native constructor returns only after the System methods are installed.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static NATIVE_SELECTION: OnceLock<(usize, usize)> = OnceLock::new();
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, roots, system, presence, imports, exports, vtables, observation, settings, layouts, hooks, finalizers, errors, text]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let Some(api) = api.as_word() else { return invalid_native(); };
    let Some(roots) = roots.as_word() else { return invalid_native(); };
    let Some(presence) = presence.as_word() else { return invalid_native(); };
    let Some(imports) = imports.as_word() else { return invalid_native(); };
    let Some(exports) = exports.as_word() else { return invalid_native(); };
    let Some(vtables) = vtables.as_word() else { return invalid_native(); };
    let Some(observation) = observation.as_word() else { return invalid_native(); };
    let Some(settings) = settings.as_word() else { return invalid_native(); };
    let Some(layouts) = layouts.as_word() else { return invalid_native(); };
    let Some(hooks) = hooks.as_word() else { return invalid_native(); };
    let Some(finalizers) = finalizers.as_word() else { return invalid_native(); };
    let Some(errors) = errors.as_word() else { return invalid_native(); };
    let Some(text) = text.as_word() else { return invalid_native(); };
    let api = api as *const IdApi;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let Some(system) = system.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver || !birnam_abi::root_registry::valid_owned(roots) } { return invalid_native(); }
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    if !unsafe { birnam_abi::namespace_presence::valid(presence) } { return invalid_native(); }
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    if !unsafe { birnam_abi::namespace_import::valid(imports) } { return invalid_native(); }
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    if !unsafe { birnam_abi::namespace_export::valid(exports) } { return invalid_native(); }
    let vtables = vtables as *const birnam_abi::vtable_enumeration::ApiV2;
    if !unsafe { birnam_abi::vtable_enumeration::valid(vtables) } { return invalid_native(); }
    let observation = observation as *const birnam_abi::method_observation::ApiV1;
    if !unsafe { birnam_abi::method_observation::valid(observation) } { return invalid_native(); }
    let settings = settings as *const birnam_abi::gc_settings::ApiV1;
    if !unsafe { birnam_abi::gc_settings::valid(settings) } { return invalid_native(); }
    let layouts = layouts as *const birnam_abi::payload_layout::ApiV1;
    if !unsafe { birnam_abi::payload_layout::valid(layouts) } { return invalid_native(); }
    let hooks = hooks as *const birnam_abi::trace_hooks::ApiV1;
    if !unsafe { birnam_abi::trace_hooks::valid(hooks) } { return invalid_native(); }
    let finalizers = finalizers as *const birnam_abi::finalizers::ApiV1;
    if !unsafe { birnam_abi::finalizers::valid(finalizers) } { return invalid_native(); }
    if unsafe { birnam_abi::error_value::attach(errors as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::text_view::attach(text as _) } != 1 { return invalid_native(); }
    if NATIVE_SELECTION.get().is_some_and(|selected| *selected != (api.addr(), system.addr())) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::word((&API as *const _ as *const c_void).addr())), _ => return invalid_native(),
    }
    assert!(NATIVE_SELECTION.set((api.addr(), system.addr())).is_ok());
    READY.store(true, Ordering::Release);
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let system_vt = unsafe { ((*api).vtableOf)(system) };
    let s0 = unsafe { retained.keep(((*api).intern)(c"loadModule:".as_ptr())) };
    unsafe { install(api, api, presence, imports, exports, vtables, observation, (*roots).api, settings, layouts, hooks, finalizers, system_vt, s0) };
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const _ as *const c_void).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
