//! Native Loader method installation (81AU).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type InstallV7 = unsafe extern "C" fn(*const IdApi, *const IdApi, *const crate::namespace_presence::ApiV1, *const crate::namespace_import::ApiV1, *const crate::namespace_export::ApiV1, *const crate::vtable_enumeration::ApiV2, *const crate::method_observation::ApiV1, Oop, Oop);
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Prelude's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV7 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV7>,
}
const _: () = {
    assert!(size_of::<ApiV7>() == 24);
    assert!(core::mem::offset_of!(ApiV7, install) == 16);
};
pub type ProviderV7 = unsafe extern "C" fn() -> *const ApiV7;
pub type DependencyV7 = unsafe extern "C" fn(*const ApiV7) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV7> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread admission; immutable descriptor and code outlive the consumer.
pub unsafe fn attach(api: *const ApiV7) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 7 || value.byte_size != size_of::<ApiV7>() || value.install.is_none() {
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
/// At System's original loadModule: registration slot. Both tables stay pinned;
/// execution is Prelude's original selection. Behavior/selector are the already
/// selected values from this installation, used synchronously without retention.
/// The live presence descriptor and its selected context/code stay pinned for
/// the same lifetime as the retained execution API.
pub unsafe fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const crate::namespace_presence::ApiV1,
    imports: *const crate::namespace_import::ApiV1,
    exports: *const crate::namespace_export::ApiV1,
    vtables: *const crate::vtable_enumeration::ApiV2,
    observation: *const crate::method_observation::ApiV1,
    system_vt: Oop,
    load_module: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native loader dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution, presence, imports, exports, vtables, observation, system_vt, load_module) };
}
#[macro_export]
macro_rules! native_loader_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_native_loader_dependency_v7(
            api: *const $crate::native_loader::ApiV7,
        ) -> core::ffi::c_int {
            unsafe { $crate::native_loader::attach(api) }
        }
    };
}
