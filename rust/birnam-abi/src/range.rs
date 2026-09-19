//! Immutable Range construction and pure bounds dependency (81AA).
use crate::{IdApi, Method, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BoundsV1 {
    pub start: i64,
    pub end: i64,
}
pub type PrepareV1 = unsafe extern "C" fn(*mut IdApi, Oop, c_int) -> Oop;
pub type StageV1 = unsafe extern "C" fn(*mut IdApi);
pub type InstallV1 = unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop, Method, Method);
pub type ReadV1 = unsafe extern "C" fn(Oop) -> BoundsV1;

/// Stages run synchronously at Collection's existing construction points. The
/// first selected table and canonical identities persist for the process. The
/// descriptor and its code are immutable and process-pinned. `bounds` is pure:
/// no Id calls, allocation, collection, reentry or mutable family state access.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<PrepareV1>,
    pub finish_prototype: Option<StageV1>,
    pub set_layout: Option<StageV1>,
    pub install: Option<InstallV1>,
    pub publish: Option<StageV1>,
    pub bounds: Option<ReadV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 64);
    assert!(core::mem::offset_of!(ApiV1, bounds) == 56);
    assert!(size_of::<BoundsV1>() == 16);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());

/// # Safety
/// Mount on the owner thread; the immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV1) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 1
        || value.byte_size != size_of::<ApiV1>()
        || value.prepare.is_none()
        || value.finish_prototype.is_none()
        || value.set_layout.is_none()
        || value.install.is_none()
        || value.publish.is_none()
        || value.bounds.is_none()
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
/// Call with retained selectors and process-pinned Vector Method pointers at
/// the original Range registration point, after layout and before publication.
pub unsafe fn install(
    api: *mut IdApi,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
    reversed: Method,
    comma: Method,
) {
    unsafe { operations().install.unwrap()(api, value1, value2, print_string, reversed, comma) };
}
/// # Safety
/// Call only inside Collection's existing Vector-absent publication branch,
/// after Vector publication and before Map publication; no predicate is repeated.
pub unsafe fn publish(api: *mut IdApi) {
    unsafe { operations().publish.unwrap()(api) };
}
/// # Safety
/// `object` has the initialized Range payload, including inherited receivers.
pub unsafe fn bounds(object: Oop) -> (i64, i64) {
    let value = unsafe { operations().bounds.unwrap()(object) };
    (value.start, value.end)
}
#[macro_export]
macro_rules! range_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_range_dependency_v1(
            api: *const $crate::range::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::range::attach(api) }
        }
    };
}
