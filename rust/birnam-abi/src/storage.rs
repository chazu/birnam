//! Callback-scoped production storage facts and owner-supplied descriptions.
//! Trusted native callers pin code/data and never retain borrowed arrays.
use crate::{Oop, RootProvider, TraceHook};
use core::ffi::{c_int, c_void};

pub type Status = c_int;
pub const OK: Status = 0;
pub const INVALID: Status = 1;
pub const BUSY: Status = 2;
pub const INCOMPLETE: Status = 3;
pub const VERSION: u32 = 1;
pub const PERMANENT: u32 = 1;
pub const CORE_PAYLOAD: u32 = 2;
pub const HAS_FINALIZER: u32 = 4;
pub const HAS_GC_LAYOUT: u32 = 8;
pub const NATIVE_ROOTS_UNOBSERVED: u32 = 1;
pub const LEGACY_ROOTS_UNOBSERVED: u32 = 2;
pub const LEGACY_HOOKS_UNOBSERVED: u32 = 4;
pub const DEFERRED_IDENTITIES_UNAVAILABLE: u32 = 8;
pub const PAYLOAD_DESCRIPTIONS_MISSING: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AllocationV1 {
    pub object: Oop,
    pub identity: u64,
    pub requested_size: usize,
    pub padded_size: usize,
    pub allocation_bytes: usize,
    pub behavior: Oop,
    pub flags: u32,
    pub trace_hook: Option<TraceHook>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeV1 {
    pub from: Oop,
    pub to: Oop,
}

#[repr(C)]
pub struct ViewV1 {
    pub version: u32,
    pub byte_size: usize,
    pub runtime_identity: u64,
    pub observation_identity: u64,
    pub allocations: *const AllocationV1,
    pub allocation_count: usize,
    pub roots: *const Oop,
    pub root_count: usize,
    pub edges: *const EdgeV1,
    pub edge_count: usize,
    pub legacy_root_providers: *const RootProvider,
    pub legacy_root_provider_count: usize,
    pub deferred_count: usize,
    pub deferred_bytes: usize,
    pub coverage_gaps: u32,
    pub gc_precise: u32,
}

pub type VisitorV1 = unsafe extern "C" fn(*mut c_void, *const ViewV1) -> Status;
#[repr(C)]
pub struct RequestV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub visitor: Option<VisitorV1>,
}
/// Host safepoint entry exported by Id. It sends the current withStorageView:.
pub type ObserveV1 = unsafe extern "C" fn(*const RequestV1) -> Status;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DescriptionV1 {
    pub runtime_identity: u64,
    pub allocation_identity: u64,
    pub object: Oop,
    pub extent: usize,
    pub strong_offsets: *const usize,
    pub strong_offset_count: usize,
}

#[repr(C)]
pub struct ProviderSinkV1 {
    pub context: *mut c_void,
    pub root: unsafe extern "C" fn(*mut c_void, Oop) -> Status,
    pub describe: unsafe extern "C" fn(*mut c_void, *const DescriptionV1) -> Status,
}
pub type ProvideV1 =
    unsafe extern "C" fn(*mut c_void, *const ViewV1, *const ProviderSinkV1) -> Status;
#[repr(C)]
pub struct ProviderV1 {
    pub context: *mut c_void,
    pub provide: Option<ProvideV1>,
}

#[repr(C)]
pub struct DescribedViewV1 {
    pub storage: *const ViewV1,
    pub roots: *const Oop,
    pub root_count: usize,
    pub descriptions: *const DescriptionV1,
    pub description_count: usize,
    pub payload_edges: *const EdgeV1,
    pub payload_edge_count: usize,
    pub missing_description_count: usize,
    pub coverage_gaps: u32,
}
pub type DescribedVisitorV1 = unsafe extern "C" fn(*mut c_void, *const DescribedViewV1) -> Status;
#[repr(C)]
pub struct DescribeRequestV1 {
    pub version: u32,
    pub byte_size: usize,
    pub providers: *const ProviderV1,
    pub provider_count: usize,
    /// Return INCOMPLETE without invoking visitor if any of these gaps remain.
    pub reject_gaps: u32,
    pub context: *mut c_void,
    pub visitor: Option<DescribedVisitorV1>,
}
/// Invoke only inside a live observation callback, passing that exact view.
pub type DescribeV1 = unsafe extern "C" fn(*const ViewV1, *const DescribeRequestV1) -> Status;
