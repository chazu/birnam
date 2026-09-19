//! Synchronous live traversal; the source owns admission and edge semantics.
use crate::storage::Status;
use core::ffi::{c_int, c_void};

pub const VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SinkV1 {
    pub context: *mut c_void,
    pub emit: unsafe extern "C" fn(*mut c_void, usize) -> Status,
}

pub type RootsV1 = unsafe extern "C" fn(*mut c_void, *const SinkV1) -> Status;
/// Return one only for a newly marked candidate, zero for every other address.
/// Admission must not emit candidates or invoke arbitrary callbacks.
pub type AdmitV1 = unsafe extern "C" fn(*mut c_void, usize) -> c_int;
pub type EdgesV1 = unsafe extern "C" fn(*mut c_void, usize, *const SinkV1) -> Status;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub roots: Option<RootsV1>,
    pub admit: Option<AdmitV1>,
    pub edges: Option<EdgesV1>,
}

pub type TraceLiveV1 = unsafe extern "C" fn(*const SourceV1) -> Status;

/// Immutable native implementation, pinned for the consumer's runtime lifetime.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub trace: Option<TraceLiveV1>,
}

pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
