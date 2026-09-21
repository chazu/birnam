//! Explicit String operations and immutable native ownership contract.
use crate::{Finalizer, Oop};
use core::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};

/// Immutable descriptor. construct and clone use completed Id/Lifetime services
/// (the admitted floor capability on the legacy path). borrow, release and
/// cleanup_owner touch only private native
/// state: no Id calls, GC, reentry or arbitrary callbacks. borrow returns null
/// for an unowned object, and a non-null stable pointer for an owned empty one.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringStorageV1 {
    pub version: u32,
    pub byte_size: usize,
    pub payload_size: usize,
    pub payload_alignment: usize,
    pub construct: unsafe extern "C" fn(*mut c_void, *const u8, usize, Option<Finalizer>) -> Oop,
    pub clone: unsafe extern "C" fn(*mut c_void, Oop, Finalizer) -> Oop,
    pub borrow: unsafe extern "C" fn(*mut c_void, Oop, *mut usize) -> *const u8,
    pub release: unsafe extern "C" fn(*mut c_void, Oop) -> c_int,
    pub cleanup: Finalizer,
    pub cleanup_owner: unsafe extern "C" fn(Finalizer) -> *mut c_void,
}

/// Copy live operations before invoking them; callbacks may be replaced during
/// reentry. Context, canonical identities and storage descriptor are immutable.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub behavior: Oop,
    pub prototype: Oop,
    pub make: unsafe extern "C" fn(*mut c_void, *const u8, usize) -> Oop,
    pub bytes: unsafe extern "C" fn(*mut c_void, Oop, *mut usize) -> *const u8,
    pub storage: *const StringStorageV1,
}
pub type StringDependencyV1 = unsafe extern "C" fn(*mut StringApiV1) -> c_int;
pub type StringPrepareV4 = unsafe extern "C" fn(
    *mut crate::IdApi,
    *const crate::namespace_export::ApiV1,
    *const crate::native_allocation::NativeAllocationV1,
    *const crate::root_registry::ApiV1,
    *const crate::payload_layout::ApiV1,
) -> *mut StringApiV1;
pub type StringProviderV1 = unsafe extern "C" fn() -> *mut StringApiV1;
static DEPENDENCY: AtomicPtr<StringApiV1> = AtomicPtr::new(core::ptr::null_mut());

/// # Safety
/// Readable descriptor from a completed String initializer.
pub unsafe fn valid(pointer: *const StringApiV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let ops = unsafe { pointer.read() };
    if ops.version != 1
        || ops.byte_size != size_of::<StringApiV1>()
        || ops.context.is_null()
        || ops.behavior.is_null()
        || ops.prototype.is_null()
        || ops.storage.is_null()
    {
        return false;
    }
    let storage = unsafe { ops.storage.read() };
    if storage.version != 1
        || storage.byte_size != size_of::<StringStorageV1>()
        || storage.payload_size == 0
        || !storage.payload_alignment.is_power_of_two()
        || storage.payload_alignment > align_of::<Oop>()
    {
        return false;
    }
    true
}
/// # Safety
/// Owner-thread admission; the table and all its code/state outlive consumers.
pub unsafe fn attach(pointer: *mut StringApiV1) -> c_int {
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
/// Owner-thread use after attachment, without concurrent table mutation.
pub unsafe fn operations() -> StringApiV1 {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native String dependency was not mounted"
    );
    unsafe { pointer.read() }
}
/// # Safety
/// Ordinary allocation/root rules; bytes must be live across the allocation.
pub unsafe fn make(bytes: *const u8, length: usize) -> Oop {
    let ops = unsafe { operations() };
    unsafe { (ops.make)(ops.context, bytes, length) }
}
/// # Safety
/// The object must own live String bytes; output length must be writable.
pub unsafe fn bytes(object: Oop, length: *mut usize) -> *const u8 {
    let ops = unsafe { operations() };
    unsafe { (ops.bytes)(ops.context, object, length) }
}
/// # Safety
/// Owner-thread use after dependency attachment.
pub unsafe fn behavior() -> Oop {
    unsafe { operations().behavior }
}
#[macro_export]
macro_rules! string_dependency {
    () => {
        /// # Safety
        /// The process-lifetime native String dependency contract applies.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_string_dependency_v1(
            dependency: *mut $crate::string::StringApiV1,
        ) -> core::ffi::c_int {
            unsafe { $crate::string::attach(dependency) }
        }
    };
}
