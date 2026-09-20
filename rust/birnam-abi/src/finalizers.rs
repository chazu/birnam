//! Native finalizer registration and inline attachments (81BN).
use crate::{Finalizer, Oop};
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Inline attachment. Callback code must outlive every retaining allocation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotV1 {
    pub callback: Option<Finalizer>,
}
impl SlotV1 {
    /// A non-applicable construction result, not a request to clear a callback.
    pub const EMPTY: Self = Self { callback: None };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub admit: Option<unsafe extern "C" fn(*mut c_void)>,
    /// Exact membership: zero means missing and leaves output untouched; one
    /// writes permanence (zero collectable, one permanent). Never dereferences
    /// an unregistered address. Fact/write calls cannot run user code or GC.
    pub read_permanent: Option<unsafe extern "C" fn(*mut c_void, Oop, *mut c_int) -> c_int>,
    /// Attach a populated slot to the exact record just read.
    pub replace: Option<unsafe extern "C" fn(*mut c_void, Oop, SlotV1)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub set: Option<unsafe extern "C" fn(*mut c_void, Oop, Finalizer)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    /// Canonical, inert attachment construction from a known permanence fact.
    pub slot: Option<unsafe extern "C" fn(c_int, Finalizer) -> SlotV1>,
}
const _: () = {
    assert!(size_of::<SlotV1>() == 8);
    assert!(size_of::<FloorV1>() == 48 && core::mem::offset_of!(FloorV1, replace) == 40);
    assert!(size_of::<ApiV1>() == 32 && core::mem::offset_of!(ApiV1, set) == 24);
    assert!(size_of::<FactoryV1>() == 40 && core::mem::offset_of!(FactoryV1, slot) == 32);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;

/// # Safety
/// Readable descriptor; validation invokes no callbacks.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>() && api.set.is_some()
}
/// # Safety
/// Owner-thread access with live descriptors and callback code.
pub unsafe fn set(pointer: *const ApiV1, object: Oop, finalizer: Finalizer) {
    let api = unsafe { *pointer };
    unsafe { api.set.unwrap()(api.context, object, finalizer) };
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 1 && factory.byte_size == size_of::<FactoryV1>()
        && factory.create.is_some() && factory.destroy.is_some() && factory.slot.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread preparation before Id startup; factory and code stay pinned.
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
    assert!(!pointer.is_null(), "native finalizers factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission invoked create"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission invoked destroy"); }
    unsafe extern "C" fn slot(_: c_int, _: Finalizer) -> SlotV1 { panic!("admission invoked slot"); }
    static FIRST: FactoryV1 = FactoryV1 { version: 1, byte_size: size_of::<FactoryV1>(),
        create: Some(create), destroy: Some(destroy), slot: Some(slot) };
    static SECOND: FactoryV1 = FIRST;
    #[test]
    fn factory_admission_is_inert_repeatable_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut bad = FIRST; bad.version += 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.byte_size -= 1; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.create = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.destroy = None; assert_eq!(attach(&bad), 0);
            bad = FIRST; bad.slot = None; assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1); assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
