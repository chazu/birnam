//! Allocation Budget v1: callback-free per-Heap charge ownership.
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub read: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub replace: Option<unsafe extern "C" fn(*mut c_void, usize)>,
    pub charge: Option<unsafe extern "C" fn(*mut c_void, usize) -> c_int>,
    pub preview: Option<unsafe extern "C" fn(*const c_void, usize, *mut usize) -> c_int>,
}
const _: () = {
    assert!(size_of::<FactoryV1>() == 64 && core::mem::offset_of!(FactoryV1, preview) == 56);
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
        && api.create.is_some()
        && api.destroy.is_some()
        && api.read.is_some()
        && api.replace.is_some()
        && api.charge.is_some()
        && api.preview.is_some()
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
    assert!(
        !pointer.is_null(),
        "allocation budget factory was not mounted"
    );
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create() -> *mut c_void {
        panic!("validation called create");
    }
    unsafe extern "C" fn destroy(_: *mut c_void) {
        panic!("validation called destroy");
    }
    unsafe extern "C" fn read(_: *const c_void) -> usize {
        panic!("validation called read");
    }
    unsafe extern "C" fn replace(_: *mut c_void, _: usize) {
        panic!("validation called replace");
    }
    unsafe extern "C" fn charge(_: *mut c_void, _: usize) -> c_int {
        panic!("validation called charge");
    }
    unsafe extern "C" fn preview(_: *const c_void, _: usize, _: *mut usize) -> c_int {
        panic!("validation called preview");
    }
    static FIRST: FactoryV1 = FactoryV1 {
        version: 1,
        byte_size: size_of::<FactoryV1>(),
        create: Some(create),
        destroy: Some(destroy),
        read: Some(read),
        replace: Some(replace),
        charge: Some(charge),
        preview: Some(preview),
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
            bad.create = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.destroy = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.read = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.replace = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.charge = None;
            assert_eq!(attach(&bad), 0);
            bad = FIRST;
            bad.preview = None;
            assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
