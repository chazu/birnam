//! Collection over completed Id, Lifetime, Trace and collection-state owners.
use crate::{storage::Status, trace_live::SinkV1};
use core::ffi::c_void;
/// Complete, scoped upper retirement operations. Before Execution exists there
/// are no upper retirement records, so InputV1 may omit this table entirely.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetirementV1 {
    pub context: *mut c_void,
    pub inspect: unsafe extern "C" fn(*mut c_void) -> i32,
    pub settle: unsafe extern "C" fn(*mut c_void),
    pub retire: unsafe extern "C" fn(*mut c_void),
    pub reclaim: unsafe extern "C" fn(*mut c_void) -> usize,
    pub prune: unsafe extern "C" fn(*mut c_void),
}
/// Callers supply a native interval and any additional scoped roots. The installed
/// extension supplies its owner's mandatory roots and retirement. Runtime rejects
/// caller-supplied retirement; that field is for the base/extension contract. Native
/// intervals expire with the enclosing capture callback. Nothing is retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputV1 {
    pub start: usize,
    pub bottom: usize,
    pub context: *mut c_void,
    pub roots: Option<unsafe extern "C" fn(*mut c_void, *const SinkV1) -> Status>,
    pub retirement: *const RetirementV1,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub trace: unsafe extern "C" fn(*mut c_void, *const InputV1) -> Status,
    pub collect: unsafe extern "C" fn(*mut c_void, *const InputV1, *mut usize) -> Status,
}
/// A completed upper owner supplies the entire dynamic extent of its roots and
/// retirement operations. The base passed to it performs collection without
/// redispatching through the extension. Neither input nor base may be retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExtensionV1 {
    pub context: *mut c_void,
    pub trace: unsafe extern "C" fn(*mut c_void, *const ApiV1, *const InputV1) -> Status,
    pub collect:
        unsafe extern "C" fn(*mut c_void, *const ApiV1, *const InputV1, *mut usize) -> Status,
}
/// All saved public entries observe the installed complete upper extension.
/// Before an upper owner exists these operations perform base collection.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV2 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub trace: unsafe extern "C" fn(*mut c_void, *const InputV1) -> Status,
    pub collect: unsafe extern "C" fn(*mut c_void, *const InputV1, *mut usize) -> Status,
    pub extend: unsafe extern "C" fn(*mut c_void, *const ExtensionV1) -> Status,
}
/// # Safety
/// The version/size header is readable. A matching descriptor has its declared
/// extent, initialized entries and live code/context. Validation calls nothing.
pub unsafe fn valid(pointer: *const ApiV2) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<ApiV2>()) {
        return false;
    }
    // Read only the common header before admitting the larger v2 descriptor.
    unsafe {
        core::ptr::addr_of!((*pointer).version).read() == 2
            && core::ptr::addr_of!((*pointer).byte_size).read() == size_of::<ApiV2>()
            && !core::ptr::addr_of!((*pointer).context).read().is_null()
    }
}
const _: () = {
    assert!(size_of::<RetirementV1>() == 48);
    assert!(size_of::<InputV1>() == 40 && size_of::<ApiV1>() == 40);
    assert!(size_of::<ExtensionV1>() == 24 && size_of::<ApiV2>() == 48);
};

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn trace(_: *mut c_void, _: *const InputV1) -> Status {
        panic!("validation invoked tracing")
    }
    unsafe extern "C" fn collect(_: *mut c_void, _: *const InputV1, _: *mut usize) -> Status {
        panic!("validation invoked collection")
    }
    unsafe extern "C" fn extend(_: *mut c_void, _: *const ExtensionV1) -> Status {
        panic!("validation installed an extension")
    }
    #[test]
    fn admission_rejects_the_shorter_descriptor_without_invoking_operations() {
        let context = core::ptr::dangling_mut();
        let old = ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context,
            trace,
            collect,
        };
        let mut current = ApiV2 {
            version: 2,
            byte_size: size_of::<ApiV2>(),
            context,
            trace,
            collect,
            extend,
        };
        unsafe {
            assert!(!valid(core::ptr::null()));
            assert!(!valid(core::ptr::from_ref(&old).cast()));
            assert!(valid(&current));
            current.byte_size = size_of::<ApiV1>();
            assert!(!valid(&current));
            current.byte_size = size_of::<ApiV2>();
            current.context = core::ptr::null_mut();
            assert!(!valid(&current));
        }
    }
}
