//! Synchronous collection orchestration over scoped owner operations.
use crate::{Finalizer, Oop, storage::Status};
use core::ffi::{c_int, c_void};

pub const VERSION: u32 = 1;
pub const IDLE: u32 = 0;
pub const MARKING: u32 = 1;
pub const FINALIZING: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeadV1 {
    pub addresses: *const usize,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DetachedV1 {
    pub object: Oop,
    pub finalizer: Option<Finalizer>,
}

/// All operations and borrowed results expire when `collect` returns. The
/// source owns one detached allocation at a time; its address is the release
/// token within that call, and cannot be reused before release.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub phase: Option<unsafe extern "C" fn(*mut c_void, u32)>,
    pub clear_marks: Option<unsafe extern "C" fn(*mut c_void)>,
    pub trace: Option<unsafe extern "C" fn(*mut c_void) -> Status>,
    /// Snapshot retirement data; answer exactly 0 or 1 for required settlement.
    pub inspect_retirement: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub settle_deferred: Option<unsafe extern "C" fn(*mut c_void)>,
    pub retire_metadata: Option<unsafe extern "C" fn(*mut c_void)>,
    pub snapshot_dead: Option<unsafe extern "C" fn(*mut c_void, *mut DeadV1)>,
    pub detach: Option<unsafe extern "C" fn(*mut c_void, usize, *mut DetachedV1) -> Status>,
    pub reclaim: Option<unsafe extern "C" fn(*mut c_void, usize) -> Status>,
    pub reclaim_deferred: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub reset_accounting: Option<unsafe extern "C" fn(*mut c_void)>,
    pub prune: Option<unsafe extern "C" fn(*mut c_void)>,
}

pub type CollectV1 = unsafe extern "C" fn(*const SourceV1, *mut usize) -> Status;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub collect: Option<CollectV1>,
}

pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
