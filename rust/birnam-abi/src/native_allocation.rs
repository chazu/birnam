//! Immutable floor allocation capability for native-owned payloads.
use crate::{Finalizer, Oop};
use core::ffi::c_void;

/// Stack-scoped construction recipe. The floor first performs its ordinary
/// pre-allocation collection check, then calls prepare, allocates/associates
/// the block, calls initialize, and installs finalizer metadata directly.
/// Neither callback may call Id, collect, reenter, or invoke arbitrary code.
/// They may allocate/copy private native memory. They must not retain context.
/// All Oops/borrowed bytes in the context obey the caller's usual root rules.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAllocationPlanV1 {
    pub context: *mut c_void,
    pub behavior: Oop,
    pub payload_size: usize,
    pub finalizer: Option<Finalizer>,
    pub prepare: unsafe extern "C" fn(*mut c_void),
    pub initialize: unsafe extern "C" fn(*mut c_void, Oop),
}

/// The descriptor, context and code remain valid for their mounting's lifetime.
/// This is an immutable capability, not a replaceable ordinary Id operation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAllocationV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub allocate: unsafe extern "C" fn(*mut c_void, *const NativeAllocationPlanV1) -> Oop,
}
pub type NativeAllocationProviderV1 = unsafe extern "C" fn() -> *const NativeAllocationV1;
