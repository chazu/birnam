//! Immutable Range construction dependency (81AC).
use crate::{IdApi, Method, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type PrepareV2 = unsafe extern "C" fn(*mut IdApi, Oop, c_int) -> Oop;
pub type StageV2 = unsafe extern "C" fn(*mut IdApi);
pub type InstallV2 = unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop, Method);

/// Stages run synchronously at Collection's existing construction points. The
/// first selected table and canonical identities persist for the process. The
/// descriptor and its code are immutable and process-pinned. The supplied Vector
/// comma Method is a direct native callback retained at initial installation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV2 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<PrepareV2>,
    pub finish_prototype: Option<StageV2>,
    pub set_layout: Option<StageV2>,
    pub install: Option<InstallV2>,
    pub publish: Option<StageV2>,
}
const _: () = {
    assert!(size_of::<ApiV2>() == 56);
    assert!(core::mem::offset_of!(ApiV2, publish) == 48);
};
pub type ProviderV2 = unsafe extern "C" fn() -> *const ApiV2;
pub type DependencyV2 = unsafe extern "C" fn(*const ApiV2) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV2> = AtomicPtr::new(ptr::null_mut());

/// # Safety
/// Mount on the owner thread; the immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV2) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 2
        || value.byte_size != size_of::<ApiV2>()
        || value.prepare.is_none()
        || value.finish_prototype.is_none()
        || value.set_layout.is_none()
        || value.install.is_none()
        || value.publish.is_none()
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
    assert!(!api.is_null(), "native Range dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// Call once at the original Range behavior creation/import position. The
/// selected table and parent remain live; `adopting` is the Vector export test.
pub unsafe fn prepare(api: *mut IdApi, parent: Oop, adopting: bool) -> Oop {
    unsafe { operations().prepare.unwrap()(api, parent, c_int::from(adopting)) }
}
/// # Safety
/// Call after prepare, at the original Range prototype allocation point.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) };
}
/// # Safety
/// Apply the owned Range layout at its original construction point.
pub unsafe fn set_layout(api: *mut IdApi) {
    unsafe { operations().set_layout.unwrap()(api) };
}
/// # Safety
/// Call with retained selectors and the process-pinned native Vector comma at
/// the original Range registration point, after layout and before publication.
pub unsafe fn install(
    api: *mut IdApi,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
    vector_comma: Method,
) {
    unsafe { operations().install.unwrap()(api, value1, value2, print_string, vector_comma) };
}
/// # Safety
/// Call only inside Collection's existing Vector-absent publication branch,
/// after Vector publication and before Map publication; no predicate is repeated.
pub unsafe fn publish(api: *mut IdApi) {
    unsafe { operations().publish.unwrap()(api) };
}
#[macro_export]
macro_rules! range_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_range_dependency_v2(
            api: *const $crate::range::ApiV2,
        ) -> core::ffi::c_int {
            unsafe { $crate::range::attach(api) }
        }
    };
}
