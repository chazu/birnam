//! Existing String methods, independently loaded from Collection (81W).
//! Owns canonical representation and external byte buffers (81X).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, MarkFn, Oop};
use core::ffi::c_void;
use core::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;
mod native;
mod text;
static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
static PHASE: AtomicU8 = AtomicU8::new(0);

fn abort(message: &str) -> ! {
    eprintln!("birnam String: {message}");
    std::process::abort();
}

unsafe extern "C" fn storage_roots(mark: MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() { unsafe { (*instance).trace(mark) }; }
}
unsafe extern "C" fn cleanup(object: Oop) {
    let ops = unsafe { birnam_string_provider_v1().read() };
    assert_eq!(unsafe { ((*ops.storage).release)(ops.context, object) }, 1, "String finalizer lost byte ownership");
}
unsafe extern "C" fn cleanup_owner(finalizer: birnam_abi::Finalizer) -> *mut c_void {
    if finalizer as usize == cleanup as *const () as usize {
        INSTANCE.load(Ordering::Acquire).cast()
    } else { ptr::null_mut() }
}
unsafe extern "C" fn string_clone(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    let ops = unsafe { birnam_string_provider_v1().read() };
    unsafe { ((*ops.storage).clone)(ops.context, receiver, cleanup) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_string_provider_v1() -> *mut birnam_abi::string::StringApiV1 {
    if PHASE.load(Ordering::Acquire) < 2 { return ptr::null_mut(); }
    unsafe { (*INSTANCE.load(Ordering::Acquire)).operations() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_string_prepare_v4(api: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1, allocation: *const birnam_abi::native_allocation::NativeAllocationV1, roots: *const birnam_abi::root_registry::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) -> *mut birnam_abi::string::StringApiV1 {
    if api.is_null() { abort("preparation received a null IdApi"); }
    if PHASE.load(Ordering::Acquire) != 0 {
        let instance = INSTANCE.load(Ordering::Acquire);
        if instance.is_null() { abort("String preparation has no instance"); }
        let same = unsafe { (*instance).native_access().map_or((*instance).api() == api, |access| access.id.receiver == (*api).vtable_vt) };
        if !same { abort("another IdApi is already mounted"); }
        if PHASE.load(Ordering::Acquire) == 1 { abort("recursive String preparation"); }
        return unsafe { birnam_string_provider_v1() };
    }
    if allocation.is_null() { abort("native allocation capability missing"); }
    PHASE.store(1, Ordering::Release);
    let instance = Box::into_raw(unsafe { storage::Instance::empty(api, exports, layouts, allocation.read(), cleanup, cleanup_owner) });
    INSTANCE.store(instance, Ordering::Release);
    unsafe { birnam_abi::root_registry::register(roots, storage_roots); (*instance).prepare(string_clone); }
    let ops = unsafe { (*instance).operations() };
    if unsafe { birnam_abi::string::attach(ops) } != 1 { abort("invalid String table"); }
    PHASE.store(2, Ordering::Release);
    ops
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
    let start = bytes
        .iter()
        .position(|byte| !is_space(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_space(*byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
