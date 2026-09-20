//! Native trace-hook attachments and bounded inherited selection.
use birnam_abi::trace_hooks::{ApiV1, EntryV1, FactoryV1, FloorV1, SlotV1, SourceV1};
use birnam_abi::{Oop, TraceHook};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Capability {
    api: ApiV1,
    floor: FloorV1,
}

unsafe extern "C" fn slot(hook: TraceHook) -> SlotV1 { SlotV1 { hook: Some(hook) } }

unsafe extern "C" fn resolve(source: *const SourceV1, vtable: Oop) -> Option<TraceHook> {
    let source = unsafe { *source };
    let mut current = vtable;
    let mut remaining = source.budget;
    while !current.is_null() && remaining != 0 {
        let mut entry = EntryV1 { parent: ptr::null_mut(), slot: SlotV1::EMPTY };
        if unsafe { (source.read)(source.context, current, &mut entry) } == 0 {
            return None;
        }
        if let Some(hook) = entry.slot.hook { return Some(hook); }
        current = entry.parent;
        remaining -= 1;
    }
    None
}

unsafe extern "C" fn public_set(context: *mut c_void, vtable: Oop, hook: TraceHook) {
    let floor = unsafe { (*context.cast::<Capability>()).floor };
    // No state borrow crosses admission, which may reenter this capability.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { floor.replace.unwrap()(floor.context, vtable, slot(hook)) };
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>()
        || floor.admit.is_none() || floor.replace.is_none()
    { return ptr::null_mut(); }
    let mut owned = Box::new(Capability {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), set: Some(public_set) },
        floor,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Capability>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy),
    slot: Some(slot), resolve: Some(resolve),
};
/// # Safety
/// The immutable descriptor and code outlive all retaining consumers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_hooks_factory_v1() -> *const FactoryV1 { &FACTORY }

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
    _: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Trace Hooks received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
