//! Reachability relative to one protected, described storage graph.
use crate::Oop;
use crate::storage::{DescribedViewV1, Status};
use core::ffi::c_void;

pub const VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationRefV1 {
    pub object: Oop,
    pub allocation_identity: u64,
}

#[repr(C)]
pub struct ReportV1 {
    pub version: u32,
    pub byte_size: usize,
    pub runtime_identity: u64,
    pub observation_identity: u64,
    /// Original description, including allocation flags and deferred accounting.
    pub described: *const DescribedViewV1,
    /// Both lists are sorted by allocation identity and expire at callback return.
    pub reached: *const AllocationRefV1,
    pub reached_count: usize,
    /// Not reached in this graph; never an authorization to reclaim storage.
    pub not_reached: *const AllocationRefV1,
    pub not_reached_count: usize,
    pub coverage_gaps: u32,
}

pub type VisitorV1 = unsafe extern "C" fn(*mut c_void, *const ReportV1) -> Status;

#[repr(C)]
pub struct RequestV1 {
    pub version: u32,
    pub byte_size: usize,
    /// Zero requests complete coverage. Any unaccepted gap returns INCOMPLETE
    /// without invoking the visitor. Accepted gaps remain visible in the report.
    pub accept_gaps: u32,
    pub context: *mut c_void,
    pub visitor: Option<VisitorV1>,
}

/// Call only inside the description visitor, with that exact live view.
pub type TraceDescribedV1 =
    unsafe extern "C" fn(*const DescribedViewV1, *const RequestV1) -> Status;
