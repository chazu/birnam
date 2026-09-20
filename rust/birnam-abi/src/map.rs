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
pub struct ApiV6 {
    pub version: u32,
    pub byte_size: usize,
    pub prepare: Option<unsafe extern "C" fn(*mut IdApi, *const crate::namespace_import::ApiV1, *const crate::root_registry::ApiV1, Oop, c_int) -> BehaviorsV1>,
    pub finish_prototype: Option<unsafe extern "C" fn(*mut IdApi)>,
    pub set_layout: Option<unsafe extern "C" fn(*mut IdApi, *const crate::payload_layout::ApiV1)>,
    pub publish: Option<unsafe extern "C" fn(*mut IdApi, *const crate::namespace_export::ApiV1)>,
    pub install: Option<unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop)>,
}
const _: () = {
    assert!(size_of::<ApiV6>() == 56);
    assert!(core::mem::offset_of!(ApiV6, install) == 48);
    assert!(size_of::<BehaviorsV1>() == 16);
};
pub type ProviderV6 = unsafe extern "C" fn() -> *const ApiV6;
pub type DependencyV6 = unsafe extern "C" fn(*const ApiV6) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV6> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
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
fn operations() -> ApiV6 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(!api.is_null(), "native Map dependency was not mounted");
    unsafe { *api }
}
/// # Safety
/// Call at Map behavior creation after Range preparation, retaining the table and parent.
pub unsafe fn prepare(api: *mut IdApi, imports: *const crate::namespace_import::ApiV1, roots: *const crate::root_registry::ApiV1, parent: Oop, adopting: bool) -> BehaviorsV1 {
    unsafe { operations().prepare.unwrap()(api, imports, roots, parent, c_int::from(adopting)) }
}
/// # Safety
/// Call after prepare and Range prototype creation.
pub unsafe fn finish_prototype(api: *mut IdApi) {
    unsafe { operations().finish_prototype.unwrap()(api) }
}
/// # Safety
/// Apply node then Map layouts at the original construction point.
pub unsafe fn set_layout(api: *mut IdApi, layouts: *const crate::payload_layout::ApiV1) {
    unsafe { operations().set_layout.unwrap()(api, layouts) }
}
/// # Safety
/// Call inside the Vector-absent branch, after Range publication; no predicate is repeated.
pub unsafe fn publish(api: *mut IdApi, exports: *const crate::namespace_export::ApiV1) {
    unsafe { operations().publish.unwrap()(api, exports) }
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
        pub unsafe extern "C" fn birnam_map_dependency_v6(
            api: *const $crate::map::ApiV6,
        ) -> core::ffi::c_int {
            unsafe { $crate::map::attach(api) }
        }
    };
}
