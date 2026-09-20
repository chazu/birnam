//! Native Mark v1: one synchronous legacy callback scope per thread.
use crate::{Oop, trace_live::SinkV1};
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub begin: Option<unsafe extern "C" fn(*mut c_void, *const SinkV1)>,
    pub end: Option<unsafe extern "C" fn(*mut c_void)>,
    pub mark: Option<unsafe extern "C" fn(*mut c_void, Oop)>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 56 && core::mem::offset_of!(ApiV1, mark) == 48);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
/// # Safety
/// The descriptor is readable; validation invokes no callbacks.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>()
        && api.create.is_some() && api.destroy.is_some()
        && api.begin.is_some() && api.end.is_some() && api.mark.is_some()
}
static API: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Prepare before Id initialization; the immutable descriptor/code stay pinned.
pub unsafe fn attach(pointer: *const ApiV1) -> c_int {
    if !unsafe { valid(pointer) } { return 0; }
    match API.compare_exchange(ptr::null_mut(), pointer.cast_mut(), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call after the host mounts the Native Mark dependency.
pub unsafe fn operations() -> ApiV1 {
    let pointer = API.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "native mark adapter was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create() -> *mut c_void { panic!("validation invoked create"); }
    unsafe extern "C" fn destroy(_: *mut c_void) { panic!("validation invoked destroy"); }
    unsafe extern "C" fn begin(_: *mut c_void, _: *const SinkV1) { panic!("validation invoked begin"); }
    unsafe extern "C" fn end(_: *mut c_void) { panic!("validation invoked end"); }
    unsafe extern "C" fn mark(_: *mut c_void, _: Oop) { panic!("validation invoked mark"); }
    static FIRST: ApiV1 = ApiV1 { version: 1, byte_size: size_of::<ApiV1>(),
        create: Some(create), destroy: Some(destroy), begin: Some(begin), end: Some(end), mark: Some(mark) };
    static SECOND: ApiV1 = FIRST;
    #[test]
    fn attachment_is_inert_repeatable_and_cannot_retarget() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut bad = FIRST; bad.version += 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.byte_size -= 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.create = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.destroy = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.begin = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.end = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.mark = None; assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1); assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
