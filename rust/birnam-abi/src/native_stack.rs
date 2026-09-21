//! Native stack anchor, register capture and word scanning.
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub admit: Option<unsafe extern "C" fn(*mut c_void)>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub set: Option<unsafe extern "C" fn(*mut c_void, usize)>,
}
/// Run below the captured register frame. The native interval is live only
/// during this synchronous, non-unwinding callback.
pub type CapturedV1 = unsafe extern "C" fn(*mut c_void, usize, usize);
pub type CaptureV1 = unsafe extern "C" fn(usize, *mut c_void, Option<CapturedV1>) -> c_int;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV3 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    pub set: Option<unsafe extern "C" fn(*mut ApiV1, usize)>,
    pub get: Option<unsafe extern "C" fn(*const ApiV1) -> usize>,
    pub scan: Option<unsafe extern "C" fn(*const crate::trace_live::SinkV1, usize, usize)>,
    pub capture: Option<CaptureV1>,
}
const _: () = {
    assert!(size_of::<FloorV1>() == 32 && core::mem::offset_of!(FloorV1, admit) == 24);
    assert!(size_of::<ApiV1>() == 32 && core::mem::offset_of!(ApiV1, set) == 24);
    assert!(size_of::<FactoryV3>() == 64 && core::mem::offset_of!(FactoryV3, scan) == 48);
};
pub type FactoryProviderV3 = unsafe extern "C" fn() -> *const FactoryV3;
pub type DependencyV3 = unsafe extern "C" fn(*const FactoryV3) -> c_int;
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;

/// # Safety
/// Readable descriptor; validation invokes no operations.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>() && api.set.is_some()
}
/// # Safety
/// Owner-thread access with live descriptor/code/context. The host must keep
/// the chosen native boundary live throughout each collection using it.
pub unsafe fn set(pointer: *const ApiV1, address: usize) {
    let api = unsafe { *pointer };
    unsafe { api.set.unwrap()(api.context, address) };
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV3) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 3 && factory.byte_size == size_of::<FactoryV3>()
        && factory.create.is_some() && factory.destroy.is_some()
        && factory.set.is_some() && factory.get.is_some() && factory.scan.is_some() && factory.capture.is_some()
}
static FACTORY: AtomicPtr<FactoryV3> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread preparation before Id startup; factory and code stay pinned.
pub unsafe fn attach(pointer: *const FactoryV3) -> c_int {
    if !unsafe { valid_factory(pointer) } { return 0; }
    match FACTORY.compare_exchange(ptr::null_mut(), pointer.cast_mut(), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call after the host attaches the declared factory dependency.
pub unsafe fn operations() -> FactoryV3 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "native stack factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission called create"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission called destroy"); }
    unsafe extern "C" fn set(_: *mut ApiV1, _: usize) { panic!("admission called set"); }
    unsafe extern "C" fn get(_: *const ApiV1) -> usize { panic!("admission called get"); }
    unsafe extern "C" fn scan(_: *const crate::trace_live::SinkV1, _: usize, _: usize) { panic!("admission called scan"); }
    unsafe extern "C" fn capture(_: usize, _: *mut c_void, _: Option<CapturedV1>) -> c_int { panic!("admission called capture"); }
    static FIRST: FactoryV3 = FactoryV3 { version: 3, byte_size: size_of::<FactoryV3>(),
        create: Some(create), destroy: Some(destroy), set: Some(set), get: Some(get), scan: Some(scan), capture: Some(capture) };
    static SECOND: FactoryV3 = FIRST;
    #[test]
    fn factory_admission_is_inert_repeatable_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut bad = FIRST; bad.version += 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.byte_size -= 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.create = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.destroy = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.set = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.get = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.scan = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.capture = None; assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1); assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
