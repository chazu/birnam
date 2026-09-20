//! The existing ordered native root-provider registry, independently owned.
use birnam_abi::root_registry::{ApiV1, CAPACITY, FactoryV1, FloorV1, SnapshotV1};
use birnam_abi::RootProvider;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Registry {
    api: ApiV1,
    floor: FloorV1,
    providers: Vec<RootProvider>,
}
unsafe extern "C" fn append(api: *mut ApiV1, provider: RootProvider) -> c_int {
    let registry = unsafe { &mut *api.cast::<Registry>() };
    if registry.providers.len() == CAPACITY { return 0; }
    registry.providers.push(provider);
    1
}
unsafe extern "C" fn register(context: *mut c_void, provider: RootProvider) {
    let owned = context.cast::<Registry>();
    let floor = unsafe { (*owned).floor };
    // No registry borrow survives admission, which can reenter registration.
    unsafe { floor.admit.unwrap()(floor.context) };
    if unsafe { append(owned.cast(), provider) } == 0 {
        unsafe { floor.overflow.unwrap()(floor.context) };
    }
}
unsafe extern "C" fn snapshot(api: *const ApiV1) -> SnapshotV1 {
    let providers = unsafe { &(*api.cast::<Registry>()).providers };
    let mut result = SnapshotV1 { count: providers.len(), entries: [None; CAPACITY] };
    for (slot, provider) in result.entries.iter_mut().zip(providers) { *slot = Some(*provider); }
    result
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() || floor.overflow.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Registry {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), register: Some(register) },
        floor, providers: Vec::new(),
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Registry>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy),
    append: Some(append), snapshot: Some(snapshot),
};
/// # Safety
/// Owner-thread operations require the declared descriptor and code lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_root_registry_factory_v1() -> *const FactoryV1 { &FACTORY }

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
    _: *const ApiV1,
) -> *mut c_void {
    assert!(!api.is_null(), "Root Registry received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }
