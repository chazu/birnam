//! Native root-anchor state and word scanning. Physical capture stays in Id.
use birnam_abi::native_stack::{ApiV1, FactoryV2, FloorV1};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Anchor {
    api: ApiV1,
    floor: FloorV1,
    address: usize,
}
unsafe extern "C" fn set(api: *mut ApiV1, address: usize) {
    unsafe { (*api.cast::<Anchor>()).address = address };
}
unsafe extern "C" fn get(api: *const ApiV1) -> usize {
    unsafe { (*api.cast::<Anchor>()).address }
}
unsafe extern "C" fn public_set(context: *mut c_void, address: usize) {
    let owned = context.cast::<Anchor>();
    let floor = unsafe { (*owned).floor };
    // Admission may reenter; retain no reference to the anchor across it.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { set(owned.cast(), address) };
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Anchor {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), set: Some(public_set) },
        floor, address: 0,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Anchor>())) };
}
// Capture and interval validation remain in Id. This loop runs below that frame.
unsafe extern "C" fn scan(pointer: *const birnam_abi::trace_live::SinkV1, start: usize, bottom: usize) {
    let sink = unsafe { &*pointer };
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address.checked_add(size_of::<usize>()).is_some_and(|end| end <= bottom) {
        // Read and emit one complete word at a time, preserving callback effects.
        let candidate = unsafe { (address as *const usize).read() };
        let status = unsafe { (sink.emit)(sink.context, candidate) };
        if status != birnam_abi::storage::OK {
            eprintln!("birnam-id invariant failed: GC tracer rejected a candidate");
            std::process::abort();
        }
        address += size_of::<usize>();
    }
}
static FACTORY: FactoryV2 = FactoryV2 {
    version: 2, byte_size: size_of::<FactoryV2>(), create: Some(create), destroy: Some(destroy),
    set: Some(set), get: Some(get), scan: Some(scan),
};
/// # Safety
/// Owner-thread operations require the declared descriptor and code lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_stack_factory_v2() -> *const FactoryV2 { &FACTORY }

/// # Safety
/// All descriptors and code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi, _: *mut c_void,
    _: *const birnam_abi::namespace_presence::ApiV1,
    _: *const birnam_abi::namespace_import::ApiV1,
    _: *const birnam_abi::namespace_export::ApiV1,
    _: *const birnam_abi::vtable_enumeration::ApiV2,
    _: *const birnam_abi::method_observation::ApiV1,
    _: *const birnam_abi::root_registry::ApiV1,
    _: *const birnam_abi::gc_settings::ApiV1,
    _: *const birnam_abi::payload_layout::ApiV1,
    _: *const birnam_abi::trace_hooks::ApiV1,
    _: *const birnam_abi::finalizers::ApiV1,
) -> *mut c_void {
    assert!(!api.is_null(), "Native Stack received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
