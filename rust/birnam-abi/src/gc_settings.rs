//! Runtime-local collection settings above Id (81BK).
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub const AUTO_COLLECTION_THRESHOLD: usize = 16 << 20;

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
    pub set_auto: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub set_precise: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
}
pub type SetV1 = unsafe extern "C" fn(*mut ApiV1, c_int);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    pub set_auto: Option<SetV1>,
    pub set_precise: Option<SetV1>,
    pub automatic_due: Option<unsafe extern "C" fn(*const ApiV1, c_int, usize) -> c_int>,
    pub precise: Option<unsafe extern "C" fn(*const ApiV1) -> c_int>,
}
const _: () = {
    assert!(size_of::<FloorV1>() == 32 && core::mem::offset_of!(FloorV1, admit) == 24);
    assert!(size_of::<ApiV1>() == 40 && core::mem::offset_of!(ApiV1, set_precise) == 32);
    assert!(size_of::<FactoryV1>() == 64 && core::mem::offset_of!(FactoryV1, precise) == 56);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;

/// # Safety
/// Readable descriptor; validation invokes no operations.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>()
        && api.set_auto.is_some() && api.set_precise.is_some()
}
/// # Safety
/// Owner-thread access; the selected descriptor, context and code stay live.
pub unsafe fn set_auto(pointer: *const ApiV1, enabled: c_int) {
    let api = unsafe { *pointer };
    unsafe { api.set_auto.unwrap()(api.context, enabled) };
}
/// # Safety
/// Owner-thread access; the selected descriptor, context and code stay live.
pub unsafe fn set_precise(pointer: *const ApiV1, enabled: c_int) {
    let api = unsafe { *pointer };
    unsafe { api.set_precise.unwrap()(api.context, enabled) };
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 1 && factory.byte_size == size_of::<FactoryV1>()
        && factory.create.is_some() && factory.destroy.is_some()
        && factory.set_auto.is_some() && factory.set_precise.is_some()
        && factory.automatic_due.is_some() && factory.precise.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread preparation before Id initialization; factory/code stay pinned.
pub unsafe fn attach(pointer: *const FactoryV1) -> c_int {
    if !unsafe { valid_factory(pointer) } { return 0; }
    match FACTORY.compare_exchange(ptr::null_mut(), pointer.cast_mut(), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call after the host attaches the declared factory dependency.
pub unsafe fn operations() -> FactoryV1 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "native GC settings factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission called create"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission called destroy"); }
    unsafe extern "C" fn set(_: *mut ApiV1, _: c_int) { panic!("admission called set"); }
    unsafe extern "C" fn due(_: *const ApiV1, _: c_int, _: usize) -> c_int { panic!("admission called due"); }
    unsafe extern "C" fn precise(_: *const ApiV1) -> c_int { panic!("admission called precise"); }
    static FIRST: FactoryV1 = FactoryV1 { version:1, byte_size:size_of::<FactoryV1>(),
        create:Some(create), destroy:Some(destroy), set_auto:Some(set), set_precise:Some(set),
        automatic_due:Some(due), precise:Some(precise) };
    static SECOND: FactoryV1 = FIRST;
    #[test]
    fn factory_validation_is_inert_repeats_same_pointer_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()),0);
            let mut bad=FIRST;
            bad.version=2; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.byte_size-=1; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.create=None; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.destroy=None; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.set_auto=None; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.set_precise=None; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.automatic_due=None; assert_eq!(attach(&bad),0);
            bad=FIRST; bad.precise=None; assert_eq!(attach(&bad),0);
            assert_eq!(attach(&FIRST),1); assert_eq!(attach(&FIRST),1);
            assert_eq!(attach(&SECOND),0);
        }
    }
}
