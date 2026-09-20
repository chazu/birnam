//! Immutable Map construction and method installation dependency (81AG).
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
/// Owner-thread construction and installation preserve Collection's callback order.
/// Descriptor/code are immutable and process-pinned. The first selected table
/// and selectors remain process-owned; reinstallation only reapplies bindings.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV3 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<unsafe extern "C" fn(*mut IdApi, *const crate::namespace_import::ApiV1, Oop, c_int) -> BehaviorsV1>,
    pub finish_prototype: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub set_layout: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub publish: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub install: Option<unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop)>,
}
const _: () = {
    assert!(size_of::<ApiV3>() == 56);
    assert!(core::mem::offset_of!(ApiV3, install) == 48);
    assert!(size_of::<BehaviorsV1>() == 16);
};
pub type ProviderV3 = unsafe extern "C" fn() -> *const ApiV3;
pub type DependencyV3 = unsafe extern "C" fn(*const ApiV3) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV3> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV3) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 3
        || value.byte_size != size_of::<ApiV3>()
        || value.prepare.is_none()
        || value.finish_prototype.is_none()
        || value.set_layout.is_none()
        || value.publish.is_none()
        || value.install.is_none()
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
fn operations() -> ApiV3 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(!api.is_null(), "native Map dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// Call at Map behavior creation after Range preparation, retaining the table and parent.
pub unsafe fn prepare(api: *mut IdApi, imports: *const crate::namespace_import::ApiV1, parent: Oop, adopting: bool) -> BehaviorsV1 {
    unsafe { operations().prepare.unwrap()(api, imports, parent, c_int::from(adopting)) }
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
/// Call after Range method installation, using the retained value/value:value:/
/// printString selectors. They and the selected IdApi remain process-owned.
pub unsafe fn install(api: *mut IdApi, value: Oop, value2: Oop, print_string: Oop) {
    unsafe { operations().install.unwrap()(api, value, value2, print_string) }
}
#[macro_export]
macro_rules! map_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_map_dependency_v3(
            api: *const $crate::map::ApiV3,
        ) -> core::ffi::c_int {
            unsafe { $crate::map::attach(api) }
        }
    };
}
