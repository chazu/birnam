//! Key Index v1: derived address-to-position storage without lookup policy.
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub is_empty: Option<unsafe extern "C" fn(*const c_void) -> c_int>,
    pub get: Option<unsafe extern "C" fn(*const c_void, usize, *mut usize) -> c_int>,
    pub insert: Option<unsafe extern "C" fn(*mut *mut c_void, usize, usize)>,
    pub clear: Option<unsafe extern "C" fn(*mut c_void)>,
}
const _: () = {
    assert!(size_of::<FactoryV1>() == 56 && core::mem::offset_of!(FactoryV1, clear) == 48);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
/// # Safety
/// Readable descriptor. Validation invokes no operations.
pub unsafe fn valid(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let api = unsafe { *pointer };
    api.version == 1
        && api.byte_size == size_of::<FactoryV1>()
        && api.destroy.is_some()
        && api.is_empty.is_some()
        && api.get.is_some()
        && api.insert.is_some()
        && api.clear.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Prepare before Id initialization; descriptor and implementation remain pinned.
pub unsafe fn attach(pointer: *const FactoryV1) -> c_int {
    if !unsafe { valid(pointer) } {
        return 0;
    }
    match FACTORY.compare_exchange(
        ptr::null_mut(),
        pointer.cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call after the host attaches the factory dependency.
pub unsafe fn operations() -> FactoryV1 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "key index factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn destroy(_: *mut c_void) {
        panic!("validation called destroy");
    }
    unsafe extern "C" fn is_empty(_: *const c_void) -> c_int {
        panic!("validation called is_empty");
    }
    unsafe extern "C" fn get(_: *const c_void, _: usize, _: *mut usize) -> c_int {
        panic!("validation called get");
    }
    unsafe extern "C" fn insert(_: *mut *mut c_void, _: usize, _: usize) {
        panic!("validation called insert");
    }
    unsafe extern "C" fn clear(_: *mut c_void) {
        panic!("validation called clear");
    }
    static FIRST: FactoryV1 = FactoryV1 {
        version: 1,
        byte_size: size_of::<FactoryV1>(),
        destroy: Some(destroy),
        is_empty: Some(is_empty),
        get: Some(get),
        insert: Some(insert),
        clear: Some(clear),
    };
    static SECOND: FactoryV1 = FIRST;
    #[test]
    fn attachment_is_inert_repeatable_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut bad = FIRST;
            bad.version += 1;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.byte_size -= 1;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.destroy = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.is_empty = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.get = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.insert = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.clear = None;
            assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
