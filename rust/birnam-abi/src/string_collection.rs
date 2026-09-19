//! Late String-to-Vector method installation (81AH).
use crate::IdApi;
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Collection's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<unsafe extern "C" fn(*mut IdApi, *const IdApi)>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 24);
    assert!(core::mem::offset_of!(ApiV1, install) == 16);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread admission; immutable descriptor and code outlive the consumer.
pub unsafe fn attach(api: *const ApiV1) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 1 || value.byte_size != size_of::<ApiV1>() || value.install.is_none() {
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
/// # Safety
/// At the existing split registration point after String parenting. Both tables
/// remain process-owned; execution is Collection's original selected table,
/// including when registration reenters with a different table.
pub unsafe fn install(registration: *mut IdApi, execution: *const IdApi) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native String collection dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution) };
}
#[macro_export]
macro_rules! string_collection_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_string_collection_dependency_v1(
            api: *const $crate::string_collection::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::string_collection::attach(api) }
        }
    };
}
