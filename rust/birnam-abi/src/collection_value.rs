//! Immutable native collection value policy dependency (81AD).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type SameValueV1 = unsafe extern "C" fn(*const IdApi, Oop, Oop) -> c_int;
pub type KeyHashV1 = unsafe extern "C" fn(*const IdApi, Oop) -> u32;
/// The descriptor and code are immutable and process-pinned. Operations retain
/// neither the selected IdApi nor Oops. Numeric/String/Id callbacks remain live
/// and can reenter. No public equality/hash send or built-in allocation is added.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub same_value: Option<SameValueV1>,
    pub key_hash: Option<KeyHashV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, key_hash) == 24);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV1) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 1
        || value.byte_size != size_of::<ApiV1>()
        || value.same_value.is_none()
        || value.key_hash.is_none()
    {
        return 0;
    }
    match DEPENDENCY.compare_exchange(
        ptr::null_mut(),
        api.cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, api)),
    }
}
fn operations() -> ApiV1 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !api.is_null(),
        "native Collection value dependency was not mounted"
    );
    unsafe { *api }
}
/// # Safety
/// Call synchronously on the owner thread. Keep the selected table and operands
/// live through their existing callbacks; borrowed String bytes obey the existing
/// String operation contract. Null operands preserve the native identity rules.
pub unsafe fn same_value(api: &IdApi, left: Oop, right: Oop) -> bool {
    unsafe { operations().same_value.unwrap()(api, left, right) != 0 }
}
/// # Safety
/// Call synchronously with a live selected table and key. String bytes and Symbol
/// names obey their existing borrowed-extent contracts. No pointer is retained.
pub unsafe fn key_hash(api: &IdApi, key: Oop) -> u32 {
    unsafe { operations().key_hash.unwrap()(api, key) }
}
