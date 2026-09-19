//! Construction-only dependency for the existing Range protocol (81Z).
//! The descriptor and both native modules stay pinned for the process lifetime.
use crate::{IdApi, Method, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type InstallV1 = unsafe extern "C" fn(*mut IdApi, Oop, Oop, Oop, Oop, Method, Method);

/// Install Range's eight entries at the original construction point. The caller
/// supplies the retained behavior/selectors and two direct Vector-backed methods.
/// Calls run synchronously on Id's owner thread, before Range publication. The
/// first Id table/selectors are retained for ordinary method calls; subsequent
/// installs use their supplied table only for registration, as Collection did.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub install: Option<InstallV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 24);
    assert!(core::mem::offset_of!(ApiV1, install) == 16);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());

/// # Safety
/// `api` points to an immutable descriptor whose code remains live for this
/// consumer's lifetime. Call only on the native stack's owner thread.
pub unsafe fn attach(api: *const ApiV1) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 1 || value.byte_size != size_of::<ApiV1>() || value.install.is_none() {
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
/// Mount the dependency first. All arguments follow `ApiV1`'s construction
/// contract and the retained Id table/selectors stay live for the process.
pub unsafe fn install(
    api: *mut IdApi,
    behavior: Oop,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
    reversed: Method,
    comma: Method,
) {
    let dependency = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !dependency.is_null(),
        "native Range methods dependency was not mounted"
    );
    let install = unsafe { (*dependency).install }.expect("validated Range installer");
    unsafe { install(api, behavior, value1, value2, print_string, reversed, comma) };
}

#[macro_export]
macro_rules! range_methods_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_range_methods_dependency_v1(
            api: *const $crate::range_methods::ApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::range_methods::attach(api) }
        }
    };
}
