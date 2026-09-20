//! Native trace-hook descriptions and bounded inherited selection (81BM).
use crate::{Oop, TraceHook};
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Inline attachment; code must outlive every retaining vtable.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotV1 {
    pub hook: Option<TraceHook>,
}
impl SlotV1 {
    pub const EMPTY: Self = Self { hook: None };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EntryV1 {
    pub parent: Oop,
    pub slot: SlotV1,
}

/// Synchronous facts, borrowed only for one canonical selection. `read` returns
/// one for exact membership, zero for absence. It writes no output on absence,
/// invokes no user code, and cannot allocate or mutate the registry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceV1 {
    pub context: *const c_void,
    pub budget: usize,
    pub read: unsafe extern "C" fn(*const c_void, Oop, *mut EntryV1) -> c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub admit: Option<unsafe extern "C" fn(*mut c_void)>,
    /// Requires exact membership and updates the raw mirror; invokes no hooks.
    pub replace: Option<unsafe extern "C" fn(*mut c_void, Oop, SlotV1)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub set: Option<unsafe extern "C" fn(*mut c_void, Oop, TraceHook)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    pub slot: Option<unsafe extern "C" fn(TraceHook) -> SlotV1>,
    pub resolve: Option<unsafe extern "C" fn(*const SourceV1, Oop) -> Option<TraceHook>>,
}
const _: () = {
    assert!(size_of::<SlotV1>() == 8);
    assert!(size_of::<EntryV1>() == 16 && core::mem::offset_of!(EntryV1, slot) == 8);
    assert!(size_of::<SourceV1>() == 24 && core::mem::offset_of!(SourceV1, read) == 16);
    assert!(size_of::<FloorV1>() == 40 && core::mem::offset_of!(FloorV1, replace) == 32);
    assert!(size_of::<ApiV1>() == 32 && core::mem::offset_of!(ApiV1, set) == 24);
    assert!(size_of::<FactoryV1>() == 48 && core::mem::offset_of!(FactoryV1, resolve) == 40);
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
/// Owner-thread access with live descriptors, code and exact vtable.
pub unsafe fn set(pointer: *const ApiV1, vtable: Oop, hook: TraceHook) {
    let api = unsafe { *pointer };
    unsafe { api.set.unwrap()(api.context, vtable, hook) };
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 1 && factory.byte_size == size_of::<FactoryV1>()
        && factory.create.is_some() && factory.destroy.is_some()
        && factory.slot.is_some() && factory.resolve.is_some()
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
    assert!(!pointer.is_null(), "native trace-hooks factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission invoked create"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission invoked destroy"); }
    unsafe extern "C" fn slot(_: TraceHook) -> SlotV1 { panic!("admission invoked slot"); }
    unsafe extern "C" fn resolve(_: *const SourceV1, _: Oop) -> Option<TraceHook> { panic!("admission invoked resolve"); }
    static FIRST: FactoryV1 = FactoryV1 { version: 1, byte_size: size_of::<FactoryV1>(),
        create: Some(create), destroy: Some(destroy), slot: Some(slot), resolve: Some(resolve) };
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
            bad = FIRST; bad.resolve = None; assert_eq!(attach(&bad), 0);
            assert_eq!(attach(&FIRST), 1); assert_eq!(attach(&FIRST), 1);
            assert_eq!(attach(&SECOND), 0);
        }
    }
}
