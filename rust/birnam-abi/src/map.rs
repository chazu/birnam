//! Immutable Map storage and ordered construction dependency (81AF).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BehaviorsV1 {
    pub map: Oop,
    pub node: Oop,
}
/// Owner-thread calls. Descriptor and code are immutable and process-pinned.
/// Traversal reads never allocate or call Id. Lookup/update receive the caller's
/// selected live table; no incoming borrowed argument carrier is retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<unsafe extern "C" fn(*mut IdApi, Oop, c_int) -> BehaviorsV1>,
    pub finish_prototype: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub set_layout: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub publish: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub empty: Option<unsafe extern "C" fn() -> Oop>,
    pub put: Option<unsafe extern "C" fn(*const IdApi, Oop, Oop, Oop) -> Oop>,
    pub find: Option<unsafe extern "C" fn(*const IdApi, Oop, Oop) -> Oop>,
    pub count: Option<unsafe extern "C" fn(Oop) -> usize>,
    pub root: Option<unsafe extern "C" fn(Oop) -> Oop>,
    pub slot_count: Option<unsafe extern "C" fn(Oop) -> usize>,
    pub slot: Option<unsafe extern "C" fn(Oop, usize) -> Oop>,
    pub behavior: Option<unsafe extern "C" fn() -> Oop>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 112);
    assert!(core::mem::offset_of!(ApiV1, behavior) == 104);
    assert!(size_of::<BehaviorsV1>() == 16);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
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
        || value.publish.is_none()
        || value.empty.is_none()
        || value.put.is_none()
        || value.find.is_none()
        || value.count.is_none()
        || value.root.is_none()
        || value.slot_count.is_none()
        || value.slot.is_none()
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
    assert!(!api.is_null(), "native Map dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// Call at Map behavior creation after Range preparation, retaining the table and parent.
pub unsafe fn prepare(api: *mut IdApi, parent: Oop, adopting: bool) -> BehaviorsV1 {
    unsafe { operations().prepare.unwrap()(api, parent, c_int::from(adopting)) }
}
/// # Safety
/// Call after prepare and Range prototype creation.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) }
}
/// # Safety
/// Apply node then Map layouts at the original construction point.
pub unsafe fn set_layout(api: *mut IdApi) {
    unsafe { operations().set_layout.unwrap()(api) }
}
/// # Safety
/// Call inside the Vector-absent branch, after Range publication; no predicate is repeated.
pub unsafe fn publish(api: *mut IdApi) {
    unsafe { operations().publish.unwrap()(api) }
}
/// # Safety
/// Call after preparation; uses the first selected table and its live allocator.
pub unsafe fn empty() -> Oop {
    unsafe { operations().empty.unwrap()() }
}
/// # Safety
/// Initialized Map receiver and nonnull key/value; keep inputs live across synchronous callbacks.
pub unsafe fn put(api: *const IdApi, object: Oop, key: Oop, value: Oop) -> Oop {
    unsafe { operations().put.unwrap()(api, object, key, value) }
}
/// # Safety
/// Initialized Map receiver; keep inputs live across synchronous hash/equality callbacks.
pub unsafe fn find(api: *const IdApi, object: Oop, key: Oop) -> Oop {
    unsafe { operations().find.unwrap()(api, object, key) }
}
/// # Safety
/// Initialized Map payload, including inherited receivers. Pure read; no Id callback.
pub unsafe fn count(object: Oop) -> usize {
    unsafe { operations().count.unwrap()(object) }
}
/// # Safety
/// Initialized Map payload. Pure read, preserving the caller's snapshot point.
pub unsafe fn root(object: Oop) -> Oop {
    unsafe { operations().root.unwrap()(object) }
}
/// # Safety
/// Nonnull initialized node; snapshot the header before traversal callbacks.
pub unsafe fn slot_count(node: Oop) -> usize {
    unsafe { operations().slot_count.unwrap()(node) }
}
/// # Safety
/// Nonnull initialized node and an in-range slot index. Pure read at the original traversal point.
pub unsafe fn slot(node: Oop, index: usize) -> Oop {
    unsafe { operations().slot.unwrap()(node, index) }
}
/// # Safety
/// Call after prepare; canonical behavior is process-owned.
pub unsafe fn behavior() -> Oop {
    unsafe { operations().behavior.unwrap()() }
}
#[macro_export]
macro_rules! map_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_map_dependency_v1(
            api: *const $crate::map::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::map::attach(api) }
        }
    };
}
