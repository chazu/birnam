//! Collection over completed Id, Lifetime, Trace and collection-state owners.
use crate::{storage::Status, trace_live::SinkV1};
use core::ffi::c_void;
/// Complete, scoped upper retirement operations. Before Execution exists there
/// are no upper retirement records, so InputV1 may omit this table entirely.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetirementV1 {
    pub context: *mut c_void,
    pub inspect: unsafe extern "C" fn(*mut c_void) -> i32,
    pub settle: unsafe extern "C" fn(*mut c_void),
    pub retire: unsafe extern "C" fn(*mut c_void),
    pub reclaim: unsafe extern "C" fn(*mut c_void) -> usize,
    pub prune: unsafe extern "C" fn(*mut c_void),
}
/// Callers supply all their live upper roots and retirement obligations. Native
/// intervals expire with the enclosing capture callback. Nothing is retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputV1 {
    pub start: usize,
    pub bottom: usize,
    pub context: *mut c_void,
    pub roots: Option<unsafe extern "C" fn(*mut c_void, *const SinkV1) -> Status>,
    pub retirement: *const RetirementV1,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub trace: unsafe extern "C" fn(*mut c_void, *const InputV1) -> Status,
    pub collect: unsafe extern "C" fn(*mut c_void, *const InputV1, *mut usize) -> Status,
}
const _: () = {
    assert!(size_of::<RetirementV1>() == 48);
    assert!(size_of::<InputV1>() == 40 && size_of::<ApiV1>() == 40);
};
