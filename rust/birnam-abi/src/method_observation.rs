//! Native direct-method overwrite observation above Id (81BI).
use crate::Oop;
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type HookV1 = unsafe extern "C" fn(Oop, Oop);
pub type NotifyV1 = unsafe extern "C" fn(*mut ApiV1, c_int, Oop, Oop);
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
    pub replace: Option<unsafe extern "C" fn(*mut c_void, Option<HookV1>)>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    pub notify: Option<NotifyV1>,
}
const _: () = {
    assert!(size_of::<HookV1>() == 8 && size_of::<Option<HookV1>>() == 8);
    assert!(size_of::<FloorV1>() == 32 && size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, replace) == 24);
    assert!(size_of::<FactoryV1>() == 40);
    assert!(core::mem::offset_of!(FactoryV1, notify) == 32);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;
/// # Safety
/// The descriptor is readable. Validation invokes no operations.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>() && api.replace.is_some()
}
/// # Safety
/// Owner-thread access; descriptor, selected operation/context and any retained
/// hook's native code remain live. Callback arguments are borrowed identities.
pub unsafe fn replace(pointer: *const ApiV1, hook: Option<HookV1>) {
    let api = unsafe { *pointer };
    unsafe { api.replace.unwrap()(api.context, hook) };
}
/// # Safety
/// Readable immutable factory. Validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<FactoryV1>() && api.create.is_some() && api.destroy.is_some() && api.notify.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread setup before Id initialization; immutable factory/code stay pinned.
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
    assert!(!pointer.is_null(), "native method observation factory was not mounted");
    unsafe { *pointer }
}
