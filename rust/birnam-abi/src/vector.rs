//! Immutable Vector storage and ordered construction dependency (81AB).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BehaviorsV1 {
    pub node: Oop,
    pub vector: Oop,
}
pub type PrepareV1 = unsafe extern "C" fn(*mut IdApi, Oop, c_int) -> BehaviorsV1;
pub type StageV1 = unsafe extern "C" fn(*mut IdApi);
pub type EmptyV1 = unsafe extern "C" fn() -> Oop;
pub type PushV1 = unsafe extern "C" fn(Oop, Oop) -> Oop;
pub type CountV1 = unsafe extern "C" fn(Oop) -> usize;
pub type AtV1 = unsafe extern "C" fn(Oop, usize) -> Oop;
/// Owner-thread construction stages preserve Collection's callback order.
/// Descriptor/code are immutable and process-pinned. Empty/push use the first
/// selected table's live allocator. Count/at/behavior never call Id, allocate,
/// collect or reenter; count/at require an initialized Vector payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<PrepareV1>,
    pub finish_prototype: Option<StageV1>,
    pub set_layout: Option<StageV1>,
    pub set_owner: Option<StageV1>,
    pub publish: Option<StageV1>,
    pub empty: Option<EmptyV1>,
    pub push: Option<PushV1>,
    pub count: Option<CountV1>,
    pub at: Option<AtV1>,
    pub behavior: Option<EmptyV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 96);
    assert!(core::mem::offset_of!(ApiV1, behavior) == 88);
    assert!(size_of::<BehaviorsV1>() == 16);
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
        || value.set_owner.is_none()
        || value.publish.is_none()
        || value.empty.is_none()
        || value.push.is_none()
        || value.count.is_none()
        || value.at.is_none()
        || value.behavior.is_none()
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
    assert!(!api.is_null(), "native Vector dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// At the original node/Vector preparation point, before Range. Retain the table
/// and parent; adopting is Collection's existing Vector export predicate.
pub unsafe fn prepare(api: *mut IdApi, parent: Oop, adopting: bool) -> BehaviorsV1 {
    unsafe { operations().prepare.unwrap()(api, parent, c_int::from(adopting)) }
}
/// # Safety
/// Finish an absent prototype after prepare and before Range prototype creation.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) };
}
/// # Safety
/// Apply node then Vector layouts before Range layout.
pub unsafe fn set_layout(api: *mut IdApi) {
    unsafe { operations().set_layout.unwrap()(api) };
}
/// # Safety
/// Set the Vector owner inside the Vector-absent branch, before Collection exports.
pub unsafe fn set_owner(api: *mut IdApi) {
    unsafe { operations().set_owner.unwrap()(api) };
}
/// # Safety
/// Publish after Collection exports in the same branch; do not recheck Vector.
pub unsafe fn publish(api: *mut IdApi) {
    unsafe { operations().publish.unwrap()(api) };
}
/// # Safety
/// Call after prepare, on the runtime owner thread.
pub unsafe fn empty() -> Oop {
    unsafe { operations().empty.unwrap()() }
}
/// # Safety
/// Vector has its initialized payload; keep both Oops live across allocation.
pub unsafe fn push(vector: Oop, value: Oop) -> Oop {
    unsafe { operations().push.unwrap()(vector, value) }
}
/// # Safety
/// Vector has its initialized payload, including inherited receivers.
pub unsafe fn count(vector: Oop) -> usize {
    unsafe { operations().count.unwrap()(vector) }
}
/// # Safety
/// Vector has its initialized payload; out-of-bounds access returns null.
pub unsafe fn at(vector: Oop, index: usize) -> Oop {
    unsafe { operations().at.unwrap()(vector, index) }
}
/// # Safety
/// Call after prepare; the canonical behavior remains process-owned.
pub unsafe fn behavior() -> Oop {
    unsafe { operations().behavior.unwrap()() }
}
#[macro_export]
macro_rules! vector_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_vector_dependency_v1(
            api: *const $crate::vector::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::vector::attach(api) }
        }
    };
}
