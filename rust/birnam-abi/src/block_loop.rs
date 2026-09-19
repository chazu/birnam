//! Block Loop method installation (81AT).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type InstallV1 = unsafe extern "C" fn(*const IdApi, *const IdApi, Oop, Oop, Oop);
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Prelude's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV1>,
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
/// At Block's original while registration slots. Both tables stay pinned;
/// execution is Prelude's original selection. Behavior/selectors are the already
/// selected values from this installation, used synchronously without retention.
pub unsafe fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    block_vt: Oop,
    while_true: Oop,
    while_false: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native block_loop dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution, block_vt, while_true, while_false) };
}
#[macro_export]
macro_rules! block_loop_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_block_loop_dependency_v1(
            api: *const $crate::block_loop::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::block_loop::attach(api) }
        }
    };
}
