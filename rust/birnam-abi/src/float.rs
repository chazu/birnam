//! Explicit native Float dependency, separate from the Id object-system API.
//! Ordinary operations retain the native primitive contracts. The immutable
//! storage codec is usable only under its declared pure-payload contract.
use crate::Oop;
use core::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};

pub const FLOAT_ABI_VERSION: u32 = 1;

/// Float owns this descriptor and its code for the native module lifetime.
/// Both callbacks are pure: no allocation, Id calls, collection, reentry or
/// access to mutable module state. They access only the supplied payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloatStorageV1 {
    pub version: u32,
    pub byte_size: usize,
    pub payload_size: usize,
    pub payload_alignment: usize,
    pub initialize_bits: unsafe extern "C" fn(*mut u8, u64),
    pub read_bits: unsafe extern "C" fn(*const u8) -> u64,
}

/// Live operations owned by the Float module. Read a copy before calling an
/// operation: native instrumentation may replace entries during reentry.
/// The context, identities and codec identify one immutable mounted family.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloatApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub behavior: Oop,
    pub prototype: Oop,
    pub make: unsafe extern "C" fn(*mut c_void, f64) -> Oop,
    pub read: unsafe extern "C" fn(*mut c_void, Oop) -> f64,
    pub is_exact: unsafe extern "C" fn(*mut c_void, Oop) -> c_int,
    pub storage: *const FloatStorageV1,
}

pub type FloatDependencyV1 = unsafe extern "C" fn(*mut FloatApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<FloatApiV1> = AtomicPtr::new(core::ptr::null_mut());

/// # Safety
/// The selected Float table, its context/code, and this consumer remain live
/// for the process. Mounting runs on the floor owner thread before publication.
pub unsafe fn valid(pointer: *const FloatApiV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let value = unsafe { pointer.read() };
    if value.version != FLOAT_ABI_VERSION
        || value.byte_size != size_of::<FloatApiV1>()
        || value.context.is_null()
        || value.behavior.is_null()
        || value.prototype.is_null()
        || value.storage.is_null()
    {
        return false;
    }
    let storage = unsafe { value.storage.read() };
    if storage.version != 1
        || storage.byte_size != size_of::<FloatStorageV1>()
        || storage.payload_size == 0
        || !storage.payload_alignment.is_power_of_two()
        || storage.payload_alignment > align_of::<Oop>()
    {
        return false;
    }
    true
}

/// # Safety
/// The selected table and all of its context/code remain live on the owner thread.
pub unsafe fn attach(pointer: *mut FloatApiV1) -> c_int {
    if !unsafe { valid(pointer) } { return 0; }
    match DEPENDENCY.compare_exchange(
        core::ptr::null_mut(),
        pointer,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(existing == pointer),
    }
}

/// # Safety
/// Use only after this image's declared dependency was attached, on the owner
/// thread. No table mutation may race the copied read.
pub unsafe fn operations() -> FloatApiV1 {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native Float dependency was not mounted"
    );
    unsafe { pointer.read() }
}

/// # Safety
/// Native owner-thread and allocation/root rules apply.
pub unsafe fn make(value: f64) -> Oop {
    let ops = unsafe { operations() };
    unsafe { (ops.make)(ops.context, value) }
}
/// # Safety
/// The object must have a live Float payload; its behavior may have changed.
pub unsafe fn value(object: Oop) -> f64 {
    let ops = unsafe { operations() };
    unsafe { (ops.read)(ops.context, object) }
}
/// # Safety
/// Native owner-thread rules apply; arbitrary/null candidates are permitted.
pub unsafe fn is_float(object: Oop) -> c_int {
    let ops = unsafe { operations() };
    unsafe { (ops.is_exact)(ops.context, object) }
}
/// # Safety
/// The consumer is mounted on the live owner-thread runtime.
pub unsafe fn behavior() -> Oop {
    unsafe { operations().behavior }
}

pub type FloatPrepareV6 = unsafe extern "C" fn(*mut crate::IdApi, *const crate::namespace_presence::ApiV1, *const crate::namespace_import::ApiV1, *const crate::namespace_export::ApiV1, *const crate::root_registry::ApiV1, *const crate::payload_layout::ApiV1) -> *mut FloatApiV1;
pub type FloatProviderV1 = unsafe extern "C" fn() -> *mut FloatApiV1;
