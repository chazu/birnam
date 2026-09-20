//! The existing single native pre-overwrite observer, independently owned.
use birnam_abi::method_observation::{ApiV1, FactoryV1, FloorV1, HookV1};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Observation {
    api: ApiV1,
    floor: FloorV1,
    hook: Option<HookV1>,
}
unsafe extern "C" fn replace(context: *mut c_void, hook: Option<HookV1>) {
    let owned = context.cast::<Observation>();
    let floor = unsafe { (*owned).floor };
    // End the borrow before admission; replacement still happens after it.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { (*owned).hook = hook };
}
unsafe extern "C" fn notify(api: *mut ApiV1, direct: c_int, vtable: birnam_abi::Oop, selector: birnam_abi::Oop) {
    let hook = unsafe { (*api.cast::<Observation>()).hook };
    if direct != 0 {
        if let Some(hook) = hook {
            // No borrow of the owned service survives this callback.
            unsafe { hook(vtable, selector) };
        }
    }
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Observation {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), replace: Some(replace) },
        floor, hook: None,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Observation>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy), notify: Some(notify),
};
/// # Safety
/// Factory operations require owner-thread access and their declared lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_method_observation_factory_v1() -> *const FactoryV1 { &FACTORY }

/// # Safety
/// All borrowed descriptors and code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi, _: *mut c_void,
    _: *const birnam_abi::namespace_presence::ApiV1,
    _: *const birnam_abi::namespace_import::ApiV1,
    _: *const birnam_abi::namespace_export::ApiV1,
    _: *const birnam_abi::vtable_enumeration::ApiV2,
    _: *const ApiV1,
) -> *mut c_void {
    assert!(!api.is_null(), "Method Observation received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }
