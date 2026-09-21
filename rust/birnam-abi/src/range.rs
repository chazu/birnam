//! Immutable Range construction dependency (81AC).
use crate::{IdApi, Method, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type PrepareV6 = unsafe extern "C" fn(*mut IdApi, *const crate::namespace_import::ApiV1, *const crate::root_registry::ApiV1, Oop, c_int) -> Oop;
pub type PublishV6 = unsafe extern "C" fn(*mut IdApi, *const crate::namespace_export::ApiV1);
pub type StageV6 = unsafe extern "C" fn(*mut IdApi);
pub type InstallV6 = unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop, Method);

/// Stages run synchronously at Collection's existing construction points. The
/// first selected table and canonical identities persist for the process. The
/// descriptor and its code are immutable and process-pinned. The supplied Vector
/// comma Method is a direct native callback retained at initial installation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV6 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<PrepareV6>,
    pub finish_prototype: Option<StageV6>,
    pub set_layout: Option<unsafe extern "C" fn(*mut IdApi, *const crate::payload_layout::ApiV1)>,
    pub install: Option<InstallV6>,
    pub publish: Option<PublishV6>,
}
const _: () = {
    assert!(size_of::<ApiV6>() == 56);
    assert!(core::mem::offset_of!(ApiV6, publish) == 48);
};
pub type ProviderV6 = unsafe extern "C" fn() -> *const ApiV6;
pub type DependencyV6 = unsafe extern "C" fn(*const ApiV6) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV6> = AtomicPtr::new(ptr::null_mut());

/// # Safety
/// Mount on the owner thread; the immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV6) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 6
        || value.byte_size != size_of::<ApiV6>()
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
fn operations() -> ApiV6 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(!api.is_null(), "native Range dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// Call once at the original Range behavior creation/import position. The
/// selected table and parent remain live; `adopting` is the Vector export test.
pub unsafe fn prepare(api: *mut IdApi, imports: *const crate::namespace_import::ApiV1, roots: *const crate::root_registry::ApiV1, parent: Oop, adopting: bool) -> Oop {
    unsafe { operations().prepare.unwrap()(api, imports, roots, parent, c_int::from(adopting)) }
}
/// # Safety
/// Call after prepare, at the original Range prototype allocation point.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) };
}
/// # Safety
/// Apply the owned Range layout at its original construction point.
pub unsafe fn set_layout(api: *mut IdApi, layouts: *const crate::payload_layout::ApiV1) {
    unsafe { operations().set_layout.unwrap()(api, layouts) };
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
pub unsafe fn publish(api: *mut IdApi, exports: *const crate::namespace_export::ApiV1) {
    unsafe { operations().publish.unwrap()(api, exports) };
}
