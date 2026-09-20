//! Lazy Error factory dispatch and exact recognition (81AM).
use crate::namespace_import::ApiV1 as ImportV1;
use crate::namespace_presence::ApiV1 as PresenceV1;
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type IsErrorV3 =
    unsafe extern "C" fn(*const IdApi, *const PresenceV1, *const ImportV1, Oop) -> c_int;
pub type FromBytesV3 = unsafe extern "C" fn(
    *const IdApi,
    *const PresenceV1,
    *const ImportV1,
    *const u8,
    usize,
    *const u8,
    usize,
) -> Oop;
/// Immutable and process-pinned. Operations use the supplied live table every
/// call and retain no API pointer, objects, selectors or byte buffers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV3 {
    pub version: u32,
    pub byte_size: usize,
    pub is_error: Option<IsErrorV3>,
    pub from_bytes: Option<FromBytesV3>,
}
const _: () = {
    assert!(size_of::<ApiV3>() == 32);
    assert!(core::mem::offset_of!(ApiV3, from_bytes) == 24);
};
pub type ProviderV3 = unsafe extern "C" fn() -> *const ApiV3;
pub type DependencyV3 = unsafe extern "C" fn(*const ApiV3) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV3> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV3) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 3
        || value.byte_size != size_of::<ApiV3>()
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
fn operations() -> ApiV3 {
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
pub unsafe fn is_error(api: &IdApi, presence: *const PresenceV1, imports: *const ImportV1, value: Oop) -> bool {
    unsafe { operations().is_error.unwrap()(api, presence, imports, value) != 0 }
}
/// # Safety
/// The kind slice is NUL-terminated, as required by IdApi intern. Both borrowed
/// extents remain valid and unchanged through callbacks, which may reenter and
/// collect. No roots or owned byte buffers are added by this adapter.
pub unsafe fn numeric_error(
    api: &IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    kind: &[u8],
    message: &[u8],
) -> Oop {
    unsafe {
        operations().from_bytes.unwrap()(
            api,
            presence,
            imports,
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
        pub unsafe extern "C" fn birnam_error_value_dependency_v3(
            api: *const $crate::error_value::ApiV3,
        ) -> core::ffi::c_int {
            unsafe { $crate::error_value::attach(api) }
        }
    };
}
