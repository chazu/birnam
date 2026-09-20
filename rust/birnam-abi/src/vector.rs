//! Immutable Vector storage, ordered construction and method installation (81AE).
use crate::{IdApi, Method, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BehaviorsV1 {
    pub node: Oop,
    pub vector: Oop,
}
pub type PrepareV6 = unsafe extern "C" fn(*mut IdApi, *const crate::namespace_import::ApiV1, *const crate::root_registry::ApiV1, Oop, c_int) -> BehaviorsV1;
pub type StageV1 = unsafe extern "C" fn(*mut IdApi);
pub type EmptyV1 = unsafe extern "C" fn() -> Oop;
pub type PushV1 = unsafe extern "C" fn(Oop, Oop) -> Oop;
pub type InstallV6 = unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop) -> Method;
/// Owner-thread construction stages preserve Collection's callback order.
/// Descriptor/code are immutable and process-pinned. Empty/push use the first
/// selected table's live allocator. Behavior never calls Id. Installation uses
/// the supplied table and retains the first selectors; Range retains the returned
/// native comma callback. Native iteration requires prior method installation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV6 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<PrepareV6>,
    pub finish_prototype: Option<StageV1>,
    pub set_layout: Option<unsafe extern "C" fn(*mut IdApi, *const crate::payload_layout::ApiV1)>,
    pub set_owner: Option<StageV1>,
    pub publish: Option<unsafe extern "C" fn(*mut IdApi, *const crate::namespace_export::ApiV1)>,
    pub empty: Option<EmptyV1>,
    pub push: Option<PushV1>,
    pub behavior: Option<EmptyV1>,
    pub install: Option<InstallV6>,
    pub for_each: Option<Method>,
    pub inject: Option<Method>,
}
const _: () = {
    assert!(size_of::<ApiV6>() == 104);
    assert!(core::mem::offset_of!(ApiV6, behavior) == 72);
    assert!(core::mem::offset_of!(ApiV6, install) == 80);
    assert!(core::mem::offset_of!(ApiV6, inject) == 96);
    assert!(size_of::<BehaviorsV1>() == 16);
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
        || value.set_owner.is_none()
        || value.publish.is_none()
        || value.empty.is_none()
        || value.push.is_none()
        || value.behavior.is_none()
        || value.install.is_none()
        || value.for_each.is_none()
        || value.inject.is_none()
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
    assert!(!api.is_null(), "native Vector dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// At the original node/Vector preparation point, before Range. Retain the table
/// and parent; adopting is Collection's existing Vector export predicate.
pub unsafe fn prepare(api: *mut IdApi, imports: *const crate::namespace_import::ApiV1, roots: *const crate::root_registry::ApiV1, parent: Oop, adopting: bool) -> BehaviorsV1 {
    unsafe { operations().prepare.unwrap()(api, imports, roots, parent, c_int::from(adopting)) }
}
/// # Safety
/// Finish an absent prototype after prepare and before Range prototype creation.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) };
}
/// # Safety
/// Apply node then Vector layouts before Range layout.
pub unsafe fn set_layout(api: *mut IdApi, layouts: *const crate::payload_layout::ApiV1) {
    unsafe { operations().set_layout.unwrap()(api, layouts) };
}
/// # Safety
/// Set the Vector owner inside the Vector-absent branch, before Collection exports.
pub unsafe fn set_owner(api: *mut IdApi) {
    unsafe { operations().set_owner.unwrap()(api) };
}
/// # Safety
/// Publish after Collection exports in the same branch; do not recheck Vector.
pub unsafe fn publish(api: *mut IdApi, exports: *const crate::namespace_export::ApiV1) {
    unsafe { operations().publish.unwrap()(api, exports) };
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
/// At the original Vector registration point; selectors remain process-owned.
/// Returns the native comma Method supplied directly to Range installation.
pub unsafe fn install(api: *mut IdApi, value1: Oop, value2: Oop, print_string: Oop) -> Method {
    unsafe { operations().install.unwrap()(api, value1, value2, print_string) }
}
/// # Safety
/// Initialized Vector receiver and a borrowed synchronous argument carrier.
/// Map iteration calls the native body, preserving its private dispatch.
pub unsafe fn for_each(vector: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { operations().for_each.unwrap()(vector, args, count) }
}
/// # Safety
/// Initialized Vector receiver and a borrowed synchronous argument carrier.
pub unsafe fn inject(vector: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { operations().inject.unwrap()(vector, args, count) }
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
        pub unsafe extern "C" fn birnam_vector_dependency_v6(
            api: *const $crate::vector::ApiV6,
        ) -> core::ffi::c_int {
            unsafe { $crate::vector::attach(api) }
        }
    };
}
