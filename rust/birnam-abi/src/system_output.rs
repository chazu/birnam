//! System output method installation (81AL).
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
/// At System's original first two registration slots. Both tables stay pinned;
/// execution is Prelude's original selection. Behavior/selectors are the already
/// selected values from this installation, used synchronously without retention.
pub unsafe fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    system_vt: Oop,
    print: Oop,
    println: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native system_output dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution, system_vt, print, println) };
}
