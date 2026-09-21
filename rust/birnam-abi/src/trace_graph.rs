//! Completed live graph access above Id, constructed before Runtime/Execution.
use crate::{
    Oop,
    storage::Status,
    trace_live::{SinkV1, TraceLiveV1},
};
use core::ffi::c_void;

pub const CORE_ONLY: u32 = 1;
pub const PRECISE: u32 = 2;

/// Borrowed for the lifetime of the root, Lifetime and Object Model owners.
/// All calls are synchronous on the constructing thread, during a collection
/// or trusted bootstrap observation. Sinks perform only first-mark admission.
/// The enclosing policy excludes reclamation and controls mutations by hooks.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GraphV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub roots: unsafe extern "C" fn(*mut c_void, *const SinkV1) -> Status,
    pub edges: unsafe extern "C" fn(*mut c_void, Oop, u32, *const SinkV1) -> Status,
    /// Check all current Id edges and payload description coverage, without
    /// marking, invoking native hooks, or authorizing reclamation.
    pub validate: unsafe extern "C" fn(*mut c_void) -> Status,
    pub trace: TraceLiveV1,
}
const _: () = assert!(size_of::<GraphV1>() == 56);
/// # Safety
/// The descriptor is readable and its context and native code are live.
pub unsafe fn valid(pointer: *const GraphV1) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<GraphV1>()) {
        return false;
    }
    let graph = unsafe { *pointer };
    graph.version == 1 && graph.byte_size == size_of::<GraphV1>() && !graph.context.is_null()
}
