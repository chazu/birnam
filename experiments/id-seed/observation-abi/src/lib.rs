//! 81H observation and 81I scoped storage retirement carriers. No heap implementation.
//! All pointers are trusted, aligned and live for the enclosing callback only.
//! Observation callbacks are read-only. Retirement preparation is read-only;
//! committed plans may swap their own metadata. No callback may unwind.
//! Code/data are pinned. Zero references in declared payload slots mean absence.
use birnam_seed_abi::*;
use core::ffi::c_void;

pub const BUSY: Status = 0x200;
pub const INCOMPLETE: Status = 0x201;
pub const OWNER_PAYLOAD: u32 = 0;
pub const SEED_PAYLOAD: u32 = 1;
pub const OBSERVE: &[u8] = b"withStorageView:";
pub const PROVIDER: &[u8] = b"storageProvider";
pub const RECLAIM: &[u8] = b"withReclamationView:";
pub const RETIREMENT: &[u8] = b"prepareStorageRetirement";

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Allocation {
    pub object: Oop,
    pub bytes: usize,
    pub payload_owner: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: Oop,
    pub to: Oop,
}
#[repr(C)]
pub struct StorageView {
    pub allocations: *const Allocation,
    pub allocation_count: usize,
    pub roots: *const Oop,
    pub root_count: usize,
    pub edges: *const Edge,
    pub edge_count: usize,
}

/// Providers emit roots and complete payload descriptions, never liveness.
/// A payload description lists aligned byte offsets containing strong Oops.
/// Each nonempty owner payload needs exactly one description, even if dead.
#[repr(C)]
pub struct ProviderSink {
    pub context: *mut c_void,
    pub root: unsafe extern "C" fn(*mut c_void, Oop) -> Status,
    pub payload: unsafe extern "C" fn(*mut c_void, Oop, usize, *const usize, usize) -> Status,
}
#[repr(C)]
pub struct TraceRequest {
    pub roots: *const Oop,
    pub root_count: usize,
    pub providers: *const Method,
    pub provider_count: usize,
    pub report: Method,
}
#[repr(C)]
pub struct TraceReport {
    pub view: *const StorageView,
    pub reachable: *const Oop,
    pub reachable_count: usize,
    pub unreachable: *const Oop,
    pub unreachable_count: usize,
}

/// Prepared tracer entry takes an already guarded view; it does not start a send.
#[repr(C)]
pub struct TraceViewRequest {
    pub view: *const StorageView,
    pub request: *const TraceRequest,
}

/// Borrowed release capability, bound to its seed and enclosing phase epoch.
/// Successful release invalidates this view for further tracing/payload reads.
#[repr(C)]
pub struct ReclamationView {
    pub view: *const StorageView,
    pub epoch: u64,
    pub release: Method,
}
#[repr(C)]
pub struct ReleaseRequest {
    pub epoch: u64,
    pub objects: *const Oop,
    pub count: usize,
    // Must leave all upper state unchanged on failure. Success retires all
    // metadata for the batch; no fallible action may follow that retirement.
    pub retire: Method,
}

/// Preparation is read-only and fallible. On success it initializes `plan`.
/// Commit and abort consume the prepared plan, cannot fail, and must not unwind.
/// Commit updates metadata only: no finalizers, user sends or payload mutation.
/// Failed preparation cleans its own scratch and leaves the output uninitialized.
#[repr(C)]
pub struct RetirementRequest {
    pub objects: *const Oop,
    pub count: usize,
    pub plan: *mut RetirementPlan,
}
#[repr(C)]
pub struct RetirementPlan {
    pub data: *mut c_void,
    pub commit: unsafe extern "C" fn(*mut c_void),
    pub abort: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CollectionParticipant {
    pub provider: Method,
    pub prepare: Method,
}
#[repr(C)]
pub struct CollectRequest {
    pub roots: *const Oop,
    pub root_count: usize,
    pub participants: *const CollectionParticipant,
    pub participant_count: usize,
    pub trace: Method,
}
#[repr(C)]
#[derive(Default, Debug)]
pub struct CollectionResult {
    pub retired_objects: usize,
    pub retired_bytes: usize,
}

/// Encode a callback-scoped, immutable C-layout record in the existing carrier.
/// # Safety
/// The receiver must use the same T and must not retain the pointer.
pub unsafe fn record<T>(value: &T) -> Value {
    Value {
        kind: BYTES,
        reserved: 0,
        a: value as *const T as usize,
        b: size_of::<T>(),
    }
}
/// # Safety
/// A correctly tagged pointer must refer to a live T for the returned lifetime.
pub unsafe fn decode<'a, T>(args: *const Value, count: usize) -> Result<&'a T, Status> {
    if count != 1 || args.is_null() {
        return Err(INVALID);
    }
    let value = unsafe { *args };
    if value.kind != BYTES
        || value.reserved != 0
        || value.a == 0
        || value.b != size_of::<T>()
        || !value.a.is_multiple_of(align_of::<T>())
    {
        return Err(INVALID);
    }
    Ok(unsafe { &*(value.a as *const T) })
}
