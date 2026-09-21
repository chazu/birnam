//! Completed live graph access above Id, constructed before Runtime/Execution.
use crate::{
    Oop,
    storage::Status,
    trace_live::{SinkV1, TraceLiveV1},
};
use core::ffi::c_void;

/// Copied physical facts, borrowed only during `inspect`'s callback. No payload
/// scanning, root-provider invocation, marking or deferred materialization.
#[repr(C)]
pub struct SnapshotV1 {
    pub allocations: *const crate::storage::AllocationV1,
    pub allocation_count: usize,
    pub roots: *const Oop,
    pub root_count: usize,
    pub edges: *const crate::storage::EdgeV1,
    pub edge_count: usize,
    pub symbols: *const Oop,
    pub symbol_count: usize,
}
pub type InspectV1 = unsafe extern "C" fn(*mut c_void, *const SnapshotV1) -> Status;

pub const CORE_ONLY: u32 = 1;
pub const PRECISE: u32 = 2;

/// Borrowed for the lifetime of the root, Lifetime and Object Model owners.
/// All calls are synchronous on the constructing thread, during a collection
/// or trusted bootstrap observation. Sinks perform only first-mark admission.
/// The enclosing policy excludes reclamation and controls mutations by hooks.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GraphV2 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub roots: unsafe extern "C" fn(*mut c_void, *const SinkV1) -> Status,
    pub edges: unsafe extern "C" fn(*mut c_void, Oop, u32, *const SinkV1) -> Status,
    /// Check all current Id edges and payload description coverage, without
    /// marking, invoking native hooks, or authorizing reclamation.
    pub validate: unsafe extern "C" fn(*mut c_void) -> Status,
    pub trace: TraceLiveV1,
    /// Inventory Id storage and completed representation records. The trusted
    /// caller excludes mutation/reclamation throughout the synchronous call.
    pub inspect: unsafe extern "C" fn(*mut c_void, *mut c_void, Option<InspectV1>) -> Status,
}
const _: () = assert!(size_of::<GraphV2>() == 64 && size_of::<SnapshotV1>() == 64);
/// # Safety
/// The descriptor is readable and its context and native code are live.
pub unsafe fn valid(pointer: *const GraphV2) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<GraphV2>()) {
        return false;
    }
    // Admit the common header before touching any added descriptor fields.
    unsafe {
        core::ptr::addr_of!((*pointer).version).read() == 2
            && core::ptr::addr_of!((*pointer).byte_size).read() == size_of::<GraphV2>()
            && !core::ptr::addr_of!((*pointer).context).read().is_null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn roots(_: *mut c_void, _: *const SinkV1) -> Status { panic!("admission called roots") }
    unsafe extern "C" fn edges(_: *mut c_void, _: Oop, _: u32, _: *const SinkV1) -> Status { panic!("admission called edges") }
    unsafe extern "C" fn validate(_: *mut c_void) -> Status { panic!("admission called validate") }
    unsafe extern "C" fn trace(_: *const crate::trace_live::SourceV1) -> Status { panic!("admission called trace") }
    unsafe extern "C" fn inspect(_: *mut c_void, _: *mut c_void, _: Option<InspectV1>) -> Status { panic!("admission called inspect") }
    #[test]
    fn descriptor_admission_rejects_an_old_header_without_reading_its_tail() {
        #[repr(C)]
        struct Header { version: u32, byte_size: usize }
        let old = Header { version: 1, byte_size: 56 };
        let mut current = GraphV2 {
            version: 2, byte_size: size_of::<GraphV2>(), context: core::ptr::dangling_mut(),
            roots, edges, validate, trace, inspect,
        };
        unsafe {
            assert!(!valid(core::ptr::null()));
            assert!(!valid(core::ptr::from_ref(&old).cast()));
            assert!(valid(&current));
            current.byte_size = 56;
            assert!(!valid(&current));
            current.byte_size = size_of::<GraphV2>();
            current.context = core::ptr::null_mut();
            assert!(!valid(&current));
        }
    }
}
