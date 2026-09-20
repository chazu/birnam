//! Deferred Index v1: opaque row storage without journal or recovery policy.
use core::{
    ffi::{c_int, c_void},
    ptr,
};
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowV1 {
    pub identity: u64,
    pub index: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub len: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub capacity: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub push: Option<unsafe extern "C" fn(*mut *mut c_void, RowV1)>,
    pub read: Option<unsafe extern "C" fn(*const c_void, usize, *mut RowV1) -> c_int>,
    pub write: Option<unsafe extern "C" fn(*mut c_void, usize, RowV1) -> c_int>,
    pub swap_remove: Option<unsafe extern "C" fn(*mut c_void, usize, *mut RowV1) -> c_int>,
    pub clear: Option<unsafe extern "C" fn(*mut c_void)>,
}
const _: () = {
    assert!(size_of::<RowV1>() == 16 && align_of::<RowV1>() == 8);
    assert!(core::mem::offset_of!(RowV1, index) == 8);
    assert!(size_of::<FactoryV1>() == 80 && core::mem::offset_of!(FactoryV1, clear) == 72);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
/// # Safety
/// Descriptor is readable. Validation invokes no operations.
pub unsafe fn valid(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let api = unsafe { *pointer };
    api.version == 1
        && api.byte_size == size_of::<FactoryV1>()
        && api.destroy.is_some()
        && api.len.is_some()
        && api.capacity.is_some()
        && api.push.is_some()
        && api.read.is_some()
        && api.write.is_some()
        && api.swap_remove.is_some()
        && api.clear.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Prepare before Id initialization; descriptor and code remain pinned.
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
/// Call only after the host attaches the factory.
pub unsafe fn operations() -> FactoryV1 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "deferred index factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn destroy(_: *mut c_void) {
        panic!("admission called destroy")
    }
    unsafe extern "C" fn count(_: *const c_void) -> usize {
        panic!("admission called count")
    }
    unsafe extern "C" fn push(_: *mut *mut c_void, _: RowV1) {
        panic!("admission called push")
    }
    unsafe extern "C" fn read(_: *const c_void, _: usize, _: *mut RowV1) -> c_int {
        panic!("admission called read")
    }
    unsafe extern "C" fn write(_: *mut c_void, _: usize, _: RowV1) -> c_int {
        panic!("admission called write")
    }
    unsafe extern "C" fn remove(_: *mut c_void, _: usize, _: *mut RowV1) -> c_int {
        panic!("admission called remove")
    }
    static FIRST: FactoryV1 = FactoryV1 {
        version: 1,
        byte_size: 80,
        destroy: Some(destroy),
        len: Some(count),
        capacity: Some(count),
        push: Some(push),
        read: Some(read),
        write: Some(write),
        swap_remove: Some(remove),
        clear: Some(destroy),
    };
    static SECOND: FactoryV1 = FIRST;
    #[test]
    fn attachment_is_inert_repeatable_and_rejects_each_invalid_field() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            for field in 0..10 {
                let mut bad = FIRST;
                match field {
                    0 => bad.version = 2,
                    1 => bad.byte_size -= 1,
                    2 => bad.destroy = None,
                    3 => bad.len = None,
                    4 => bad.capacity = None,
                    5 => bad.push = None,
                    6 => bad.read = None,
                    7 => bad.write = None,
                    8 => bad.swap_remove = None,
                    9 => bad.clear = None,
                    _ => unreachable!(),
                }
                assert_eq!(attach(&bad), 0);
            }
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
