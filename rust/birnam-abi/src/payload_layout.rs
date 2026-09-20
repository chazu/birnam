//! Exact-vtable payload descriptions and explicit ownership transfer (81BL).
use crate::Oop;
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Borrowed bytes, valid only until replacement or destruction of the record.
/// Queries are synchronous, callback-free and do not retain this view.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ViewV1 {
    pub kinds: *const u8,
    pub count: usize,
    pub rest: u8,
    pub is_set: u8,
}
impl ViewV1 {
    pub const UNSET: Self = Self { kinds: ptr::null(), count: 0, rest: 0, is_set: 0 };
}
/// Linear ownership of bytes allocated by the selected factory. Transfer once
/// into an exact floor record or release once through that same factory.
/// Unlike a view, this carrier must never be copied or released by the caller's
/// allocator. The inline unset carrier needs no backing allocation.
#[repr(C)]
pub struct OwnedV1 {
    pub view: ViewV1,
}
impl OwnedV1 {
    pub const UNSET: Self = Self { view: ViewV1::UNSET };
}
pub type ReleaseV1 = unsafe extern "C" fn(OwnedV1);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub admit: Option<unsafe extern "C" fn(*mut c_void)>,
    /// Takes ownership, requires exact membership, releases the old description
    /// and synchronizes the raw mirror. No callbacks or repeated admission.
    pub replace: Option<unsafe extern "C" fn(*mut c_void, Oop, OwnedV1)>,
    /// Exact membership is required; no callbacks or repeated admission.
    pub view: Option<unsafe extern "C" fn(*mut c_void, Oop) -> ViewV1>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub set: Option<unsafe extern "C" fn(*mut c_void, Oop, *const u8, usize, u8)>,
    pub slot_kind: Option<unsafe extern "C" fn(*mut c_void, Oop, usize) -> u8>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    /// Copies before any floor membership lookup; allocation only, no callbacks.
    pub copy: Option<unsafe extern "C" fn(*const u8, usize, u8) -> OwnedV1>,
    pub release: Option<ReleaseV1>,
    pub slot_kind: Option<unsafe extern "C" fn(ViewV1, usize) -> u8>,
    pub has_layout: Option<unsafe extern "C" fn(ViewV1) -> c_int>,
    pub all_oop: Option<unsafe extern "C" fn(ViewV1, usize) -> c_int>,
}
const _: () = {
    assert!(size_of::<ViewV1>() == 24 && core::mem::offset_of!(ViewV1, rest) == 16);
    assert!(size_of::<OwnedV1>() == 24);
    assert!(size_of::<FloorV1>() == 48 && core::mem::offset_of!(FloorV1, view) == 40);
    assert!(size_of::<ApiV1>() == 40 && core::mem::offset_of!(ApiV1, slot_kind) == 32);
    assert!(size_of::<FactoryV1>() == 72 && core::mem::offset_of!(FactoryV1, all_oop) == 64);
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
        && api.set.is_some() && api.slot_kind.is_some()
}
/// # Safety
/// Owner-thread access; descriptor/context/code stay live and input is readable.
pub unsafe fn set(pointer: *const ApiV1, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
    let api = unsafe { *pointer };
    unsafe { api.set.unwrap()(api.context, vtable, kinds, count, rest) };
}
/// # Safety
/// Owner-thread access; descriptor/context/code and exact vtable stay live.
pub unsafe fn slot_kind(pointer: *const ApiV1, vtable: Oop, word: usize) -> u8 {
    let api = unsafe { *pointer };
    unsafe { api.slot_kind.unwrap()(api.context, vtable, word) }
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 1 && factory.byte_size == size_of::<FactoryV1>()
        && factory.create.is_some() && factory.destroy.is_some()
        && factory.copy.is_some() && factory.release.is_some()
        && factory.slot_kind.is_some() && factory.has_layout.is_some() && factory.all_oop.is_some()
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
    assert!(!pointer.is_null(), "native payload-layout factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission invoked create"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission invoked destroy"); }
    unsafe extern "C" fn copy(_: *const u8, _: usize, _: u8) -> OwnedV1 { panic!("admission invoked copy"); }
    unsafe extern "C" fn release(_: OwnedV1) { panic!("admission invoked release"); }
    unsafe extern "C" fn slot(_: ViewV1, _: usize) -> u8 { panic!("admission invoked slot"); }
    unsafe extern "C" fn has(_: ViewV1) -> c_int { panic!("admission invoked has"); }
    unsafe extern "C" fn all(_: ViewV1, _: usize) -> c_int { panic!("admission invoked all"); }
    static FIRST: FactoryV1 = FactoryV1 { version: 1, byte_size: size_of::<FactoryV1>(),
        create: Some(create), destroy: Some(destroy), copy: Some(copy), release: Some(release),
        slot_kind: Some(slot), has_layout: Some(has), all_oop: Some(all) };
    static SECOND: FactoryV1 = FIRST;
    #[test]
    fn factory_validation_is_inert_repeats_same_pointer_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut bad = FIRST; bad.version = 2; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.byte_size -= 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.create = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.destroy = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.copy = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.release = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.slot_kind = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.has_layout = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.all_oop = None; assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1); assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
