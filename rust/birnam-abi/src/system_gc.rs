//! System GC control method installation (81AJ).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type InstallV2 = unsafe extern "C" fn(*const IdApi, *const IdApi, *const crate::gc_settings::ApiV1, Oop, Oop, Oop, Oop, Oop);
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Prelude's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV2 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV2>,
}
const _: () = {
    assert!(size_of::<ApiV2>() == 24);
    assert!(core::mem::offset_of!(ApiV2, install) == 16);
};
pub type ProviderV2 = unsafe extern "C" fn() -> *const ApiV2;
pub type DependencyV2 = unsafe extern "C" fn(*const ApiV2) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV2> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread admission; immutable descriptor and code outlive the consumer.
pub unsafe fn attach(api: *const ApiV2) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 2 || value.byte_size != size_of::<ApiV2>() || value.install.is_none() {
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
/// At System's original final four registration slots. Both tables stay pinned;
/// execution is Prelude's original selection. Behavior/selectors are the already
/// selected values from this installation, used synchronously without retention.
pub unsafe fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    settings: *const crate::gc_settings::ApiV1,
    system_vt: Oop,
    gc: Oop,
    live_objects: Oop,
    auto_gc: Oop,
    precise_gc: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native system_gc dependency was not mounted"
    );
    unsafe {
        (*pointer).install.unwrap()(
            registration,
            execution,
            settings,
            system_vt,
            gc,
            live_objects,
            auto_gc,
            precise_gc,
        )
    };
}
