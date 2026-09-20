//! Native Loader method installation (81AU).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type InstallV5 = unsafe extern "C" fn(*const IdApi, *const IdApi, *const crate::namespace_presence::ApiV1, *const crate::namespace_import::ApiV1, *const crate::namespace_export::ApiV1, *const crate::vtable_enumeration::ApiV1, Oop, Oop);
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Prelude's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV5 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV5>,
}
const _: () = {
    assert!(size_of::<ApiV5>() == 24);
    assert!(core::mem::offset_of!(ApiV5, install) == 16);
};
pub type ProviderV5 = unsafe extern "C" fn() -> *const ApiV5;
pub type DependencyV5 = unsafe extern "C" fn(*const ApiV5) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV5> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread admission; immutable descriptor and code outlive the consumer.
pub unsafe fn attach(api: *const ApiV5) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 5 || value.byte_size != size_of::<ApiV5>() || value.install.is_none() {
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
    vtables: *const crate::vtable_enumeration::ApiV1,
    system_vt: Oop,
    load_module: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native loader dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution, presence, imports, exports, vtables, system_vt, load_module) };
}
#[macro_export]
macro_rules! native_loader_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_native_loader_dependency_v5(
            api: *const $crate::native_loader::ApiV5,
        ) -> core::ffi::c_int {
            unsafe { $crate::native_loader::attach(api) }
        }
    };
}
