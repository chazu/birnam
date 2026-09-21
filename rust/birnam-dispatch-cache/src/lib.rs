//! Bounded inherited dictionary answers, without roots or executable leases.
mod guard_verdict;
pub use guard_verdict::birnam_guard_verdict_factory_v1;
use birnam_abi::dispatch_cache::FactoryV2;
use birnam_abi::id_protocol::NativeEntry;
use core::{cell::Cell, ffi::{c_int, c_void}, ptr};

const SLOTS: usize = 64;
#[derive(Clone, Copy)]
struct Entry {
    epoch: u64,
    selector: usize,
    shape: usize,
    method: NativeEntry,
}
const EMPTY: Entry = Entry { epoch: u64::MAX, selector: 0, shape: 0, method: NativeEntry { method: None, context: ptr::null_mut() } };
struct Cache([Cell<Entry>; SLOTS]);
fn slot(selector: usize) -> usize { ((selector >> 4) ^ (selector >> 10)) & (SLOTS - 1) }
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cache(core::array::from_fn(|_| Cell::new(EMPTY))))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cache>())) };
}
unsafe extern "C" fn lookup(context: *const c_void, epoch: u64, shape: usize, selector: usize) -> NativeEntry {
    let cache = unsafe { &*context.cast::<Cache>() };
    let entry = cache.0[slot(selector)].get();
    if entry.epoch == epoch && entry.shape == shape && entry.selector == selector { entry.method } else { EMPTY.method }
}
unsafe extern "C" fn insert(context: *mut c_void, epoch: u64, shape: usize, selector: usize, method: NativeEntry) {
    let cache = unsafe { &*context.cast::<Cache>() };
    cache.0[slot(selector)].set(Entry { epoch, shape, selector, method });
}
unsafe extern "C" fn clear(context: *mut c_void) {
    let cache = unsafe { &*context.cast::<Cache>() };
    for entry in &cache.0 { entry.set(EMPTY); }
}
static FACTORY: FactoryV2 = FactoryV2 {
    version: 2, byte_size: size_of::<FactoryV2>(), create: Some(create), destroy: Some(destroy),
    lookup: Some(lookup), insert: Some(insert), clear: Some(clear),
};
/// # Safety
/// The host pins this immutable descriptor and implementation for every owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_dispatch_cache_factory_v2() -> *const FactoryV2 { &FACTORY }

#[cfg(test)]
mod tests;

/// # Safety
/// All descriptors and code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi,
    _: *mut c_void,
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
    assert!(!api.is_null(), "Dispatch Cache received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int {
    1
}

