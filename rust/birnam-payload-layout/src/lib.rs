//! Owned exact-vtable payload descriptions shared by GC, Image and recovery.
use birnam_abi::payload_layout::{ApiV1, FactoryV1, FloorV1, OwnedV1, ViewV1};
use birnam_abi::{Oop, SLOT_OOP, SLOT_UNKNOWN};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Capability {
    api: ApiV1,
    floor: FloorV1,
}

unsafe extern "C" fn copy(kinds: *const u8, count: usize, rest: u8) -> OwnedV1 {
    if count != 0 && kinds.is_null() {
        // Preserve the existing public diagnostic and its pre-membership order.
        eprintln!("birnam-id invariant failed: setLayout kinds is null with nonzero count");
        std::process::abort();
    }
    let kinds = if count == 0 {
        ptr::null()
    } else {
        let bytes = unsafe { core::slice::from_raw_parts(kinds, count) }.to_vec().into_boxed_slice();
        Box::into_raw(bytes).cast::<u8>().cast_const()
    };
    OwnedV1 { view: ViewV1 { kinds, count, rest, is_set: 1 } }
}
unsafe extern "C" fn release(owned: OwnedV1) {
    if owned.view.count != 0 {
        let slice = ptr::slice_from_raw_parts_mut(owned.view.kinds.cast_mut(), owned.view.count);
        unsafe { drop(Box::from_raw(slice)) };
    }
}
unsafe extern "C" fn slot_kind(view: ViewV1, word: usize) -> u8 {
    if view.is_set == 0 { return SLOT_UNKNOWN; }
    if word >= view.count { return view.rest; }
    unsafe { *view.kinds.add(word) }
}
unsafe extern "C" fn has_layout(view: ViewV1) -> c_int { c_int::from(view.is_set != 0) }
unsafe extern "C" fn all_oop(view: ViewV1, words: usize) -> c_int {
    if view.is_set == 0 { return 0; }
    for index in 0..words.min(view.count) {
        if unsafe { *view.kinds.add(index) } != SLOT_OOP { return 0; }
    }
    c_int::from(words <= view.count || view.rest == SLOT_OOP)
}
unsafe extern "C" fn public_set(context: *mut c_void, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
    let floor = unsafe { (*context.cast::<Capability>()).floor };
    // Admission can reenter. No state borrow or input copy spans that callback.
    unsafe { floor.admit.unwrap()(floor.context) };
    let owned = unsafe { copy(kinds, count, rest) };
    unsafe { floor.replace.unwrap()(floor.context, vtable, owned) };
}
unsafe extern "C" fn public_slot_kind(context: *mut c_void, vtable: Oop, word: usize) -> u8 {
    let floor = unsafe { (*context.cast::<Capability>()).floor };
    unsafe { floor.admit.unwrap()(floor.context) };
    let view = unsafe { floor.view.unwrap()(floor.context, vtable) };
    unsafe { slot_kind(view, word) }
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>()
        || floor.admit.is_none() || floor.replace.is_none() || floor.view.is_none()
    { return ptr::null_mut(); }
    let mut owned = Box::new(Capability {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(),
            set: Some(public_set), slot_kind: Some(public_slot_kind) },
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
    copy: Some(copy), release: Some(release), slot_kind: Some(slot_kind),
    has_layout: Some(has_layout), all_oop: Some(all_oop),
};
/// # Safety
/// The immutable descriptor/code outlive every carrier and retaining consumer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_payload_layout_factory_v1() -> *const FactoryV1 { &FACTORY }

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
    _: *const birnam_abi::root_registry::ApiV1,
    _: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Payload Layout received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
