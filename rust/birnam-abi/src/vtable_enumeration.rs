//! Native vtable observation above Id (81BG).
use crate::Oop;
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Transient, unrooted Id-owned identities, borrowed until registry mutation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ViewV1 {
    pub entries: *const Oop,
    pub count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV2 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub view: Option<unsafe extern "C" fn(*mut c_void) -> ViewV1>,
    pub selectors: Option<unsafe extern "C" fn(*mut c_void, Oop) -> ViewV1>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV2 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub at: Option<unsafe extern "C" fn(*mut c_void, usize) -> Oop>,
    pub selector_at: Option<unsafe extern "C" fn(*mut c_void, Oop, usize) -> Oop>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV2 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV2) -> *mut ApiV2>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV2)>,
}
const _: () = {
    assert!(size_of::<ViewV1>() == 16);
    assert!(size_of::<FloorV2>() == 40);
    assert!(size_of::<ApiV2>() == 48);
    assert!(core::mem::offset_of!(ApiV2, count) == 24);
    assert!(core::mem::offset_of!(ApiV2, at) == 32);
    assert!(core::mem::offset_of!(ApiV2, selector_at) == 40);
    assert!(size_of::<FactoryV2>() == 32);
};
pub type ProviderV2 = unsafe extern "C" fn() -> *mut ApiV2;
pub type FactoryProviderV2 = unsafe extern "C" fn() -> *const FactoryV2;
pub type DependencyV2 = unsafe extern "C" fn(*const FactoryV2) -> c_int;

/// # Safety
/// Readable descriptor; inert validation does not invoke its operations.
pub unsafe fn valid(pointer: *const ApiV2) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 2 && api.byte_size == size_of::<ApiV2>() && api.count.is_some() && api.at.is_some() && api.selector_at.is_some()
}
/// # Safety
/// Owner-thread access; selected callback/context remain valid through the call.
pub unsafe fn count(pointer: *const ApiV2) -> usize {
    let api = unsafe { *pointer };
    unsafe { api.count.unwrap()(api.context) }
}
/// # Safety
/// As for count. The returned identity is unrooted, as in the original Id API.
pub unsafe fn at(pointer: *const ApiV2, index: usize) -> Oop {
    let api = unsafe { *pointer };
    unsafe { api.at.unwrap()(api.context, index) }
}
/// # Safety
/// Owner-thread access to a live descriptor and its selected callback/context.
/// The floor requires exact vtable membership even for an out-of-range index.
pub unsafe fn selector_at(pointer: *const ApiV2, vtable: Oop, index: usize) -> Oop {
    let api = unsafe { *pointer };
    unsafe { api.selector_at.unwrap()(api.context, vtable, index) }
}
/// # Safety
/// Readable factory; validation invokes no callbacks.
pub unsafe fn valid_factory(pointer: *const FactoryV2) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 2 && api.byte_size == size_of::<FactoryV2>() && api.create.is_some() && api.destroy.is_some()
}
static FACTORY: AtomicPtr<FactoryV2> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread setup before Id initialization; immutable factory/code stay pinned.
pub unsafe fn attach(pointer: *const FactoryV2) -> c_int {
    if !unsafe { valid_factory(pointer) } { return 0; }
    match FACTORY.compare_exchange(ptr::null_mut(), pointer.cast_mut(), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call only after the host attaches the declared factory dependency.
pub unsafe fn operations() -> FactoryV2 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "native vtable enumeration factory was not mounted");
    unsafe { *pointer }
}
