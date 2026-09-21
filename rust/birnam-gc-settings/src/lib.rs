//! The existing automatic-collection and precise-tracing settings, independently owned.
use birnam_abi::gc_settings::{ApiV1, FactoryV1, FloorV1, AUTO_COLLECTION_THRESHOLD};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Settings {
    api: ApiV1,
    floor: FloorV1,
    auto_requested: bool,
    precise: bool,
}
unsafe extern "C" fn set_auto(api: *mut ApiV1, enabled: c_int) {
    unsafe { (*api.cast::<Settings>()).auto_requested = enabled != 0 };
}
unsafe extern "C" fn set_precise(api: *mut ApiV1, enabled: c_int) {
    unsafe { (*api.cast::<Settings>()).precise = enabled != 0 };
}
unsafe extern "C" fn automatic_due(api: *const ApiV1, stack_ready: c_int, allocated: usize) -> c_int {
    c_int::from(unsafe { (*api.cast::<Settings>()).auto_requested }
        && stack_ready != 0 && allocated >= AUTO_COLLECTION_THRESHOLD)
}
unsafe extern "C" fn precise(api: *const ApiV1) -> c_int {
    c_int::from(unsafe { (*api.cast::<Settings>()).precise })
}
unsafe extern "C" fn public_auto(context: *mut c_void, enabled: c_int) {
    let owned = context.cast::<Settings>();
    let floor = unsafe { (*owned).floor };
    // Admission may reenter; do not retain a settings borrow across it.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { set_auto(owned.cast(), enabled) };
}
unsafe extern "C" fn public_precise(context: *mut c_void, enabled: c_int) {
    let owned = context.cast::<Settings>();
    let floor = unsafe { (*owned).floor };
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { set_precise(owned.cast(), enabled) };
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Settings {
        api: ApiV1 { version:1, byte_size:size_of::<ApiV1>(), context:ptr::null_mut(),
            set_auto:Some(public_auto), set_precise:Some(public_precise) },
        floor, auto_requested:true, precise:false,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Settings>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version:1, byte_size:size_of::<FactoryV1>(), create:Some(create), destroy:Some(destroy),
    set_auto:Some(set_auto), set_precise:Some(set_precise),
    automatic_due:Some(automatic_due), precise:Some(precise),
};
/// # Safety
/// Owner-thread operations require the declared descriptor and code lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_gc_settings_factory_v1() -> *const FactoryV1 { &FACTORY }

/// # Safety
/// All descriptors/code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi, _: *mut c_void,
    _: *const birnam_abi::namespace_presence::ApiV1,
    _: *const birnam_abi::namespace_import::ApiV1,
    _: *const birnam_abi::namespace_export::ApiV1,
    _: *const birnam_abi::vtable_enumeration::ApiV2,
    _: *const birnam_abi::method_observation::ApiV1,
    _: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "GC Settings received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;

/// Publish complete primitive operations through Id before Runtime exists.
unsafe extern "C" fn initialize_native(_: *const birnam_abi::id_protocol::Protocol, _: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    if count != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    Outcome::loaded(Operand::word(&FACTORY as *const FactoryV1 as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32, initialize: Some(initialize_native),
};
