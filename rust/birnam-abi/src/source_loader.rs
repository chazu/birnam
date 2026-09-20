//! Source Loader method installation (81AV).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub type InstallV3 = unsafe extern "C" fn(*const IdApi, *const IdApi, *const crate::namespace_presence::ApiV1, *const crate::namespace_import::ApiV1, Oop, Oop);
/// Immutable, process-pinned descriptor. Installation runs on the owner thread
/// at Prelude's existing point; the execution table is its original selection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV3 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV3>,
}
const _: () = {
    assert!(size_of::<ApiV3>() == 24);
    assert!(core::mem::offset_of!(ApiV3, install) == 16);
};
pub type ProviderV3 = unsafe extern "C" fn() -> *const ApiV3;
pub type DependencyV3 = unsafe extern "C" fn(*const ApiV3) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV3> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread admission; immutable descriptor and code outlive the consumer.
pub unsafe fn attach(api: *const ApiV3) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 3 || value.byte_size != size_of::<ApiV3>() || value.install.is_none() {
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
/// At System's original load: registration slot. Both tables stay pinned;
/// execution is Prelude's original selection. Behavior/selector are the already
/// selected values from this installation, used synchronously without retention.
/// The live presence descriptor and its selected context/code stay pinned for
/// the same lifetime as the retained execution API.
pub unsafe fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const crate::namespace_presence::ApiV1,
    imports: *const crate::namespace_import::ApiV1,
    system_vt: Oop,
    load: Oop,
) {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native source_loader dependency was not mounted"
    );
    unsafe { (*pointer).install.unwrap()(registration, execution, presence, imports, system_vt, load) };
}
#[macro_export]
macro_rules! source_loader_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_source_loader_dependency_v3(
            api: *const $crate::source_loader::ApiV3,
        ) -> core::ffi::c_int {
            unsafe { $crate::source_loader::attach(api) }
        }
    };
}
