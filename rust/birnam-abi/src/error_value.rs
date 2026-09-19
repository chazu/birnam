//! Lazy Error factory dispatch and exact recognition (81AM).
use crate::{IdApi, Oop};
use crate::namespace_presence::ApiV1 as PresenceV1;
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type IsErrorV2 = unsafe extern "C" fn(*const IdApi, *const PresenceV1, Oop) -> c_int;
pub type FromBytesV2 =
    unsafe extern "C" fn(*const IdApi, *const PresenceV1, *const u8, usize, *const u8, usize) -> Oop;
/// Immutable and process-pinned. Operations use the supplied live table every
/// call and retain no API pointer, objects, selectors or byte buffers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV2 {
    pub version: u32,
    pub byte_size: usize,
    pub is_error: Option<IsErrorV2>,
    pub from_bytes: Option<FromBytesV2>,
}
const _: () = {
    assert!(size_of::<ApiV2>() == 32);
    assert!(core::mem::offset_of!(ApiV2, from_bytes) == 24);
};
pub type ProviderV2 = unsafe extern "C" fn() -> *const ApiV2;
pub type DependencyV2 = unsafe extern "C" fn(*const ApiV2) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV2> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV2) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 2
        || value.byte_size != size_of::<ApiV2>()
        || value.is_error.is_none()
        || value.from_bytes.is_none()
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
fn operations() -> ApiV2 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !api.is_null(),
        "native Error value dependency was not mounted"
    );
    unsafe { *api }
}
/// # Safety
/// Owner-thread call using the live selected table and a valid candidate Oop.
/// Export/import/vtable callbacks may reenter and collect as before.
pub unsafe fn is_error(api: &IdApi, presence: *const PresenceV1, value: Oop) -> bool {
    unsafe { operations().is_error.unwrap()(api, presence, value) != 0 }
}
/// # Safety
/// The kind slice is NUL-terminated, as required by IdApi intern. Both borrowed
/// extents remain valid and unchanged through callbacks, which may reenter and
/// collect. No roots or owned byte buffers are added by this adapter.
pub unsafe fn numeric_error(api: &IdApi, presence: *const PresenceV1, kind: &[u8], message: &[u8]) -> Oop {
    unsafe {
        operations().from_bytes.unwrap()(
            api,
            presence,
            kind.as_ptr(),
            kind.len(),
            message.as_ptr(),
            message.len(),
        )
    }
}
#[macro_export]
macro_rules! error_value_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_error_value_dependency_v2(
            api: *const $crate::error_value::ApiV2,
        ) -> core::ffi::c_int {
            unsafe { $crate::error_value::attach(api) }
        }
    };
}
