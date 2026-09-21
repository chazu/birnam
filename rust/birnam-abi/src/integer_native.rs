//! Optional pure declarations for the native Integer implementation.
use crate::{Oop, id_protocol::{NativeEntry, Protocol}, integer::IntegerApiV1, root_registry};
use core::ffi::c_int;

/// Immutable dependencies actually used by this Integer constructor and its
/// successful arithmetic path. This is a description, never a dispatch table.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConstructionV1 {
    pub id: Protocol,
    pub allocation_size: NativeEntry,
    pub behavior_of: NativeEntry,
    pub allocate: Oop,
    pub namespace: Oop,
    pub binding: Oop,
    pub error_behavior_name: Oop,
    pub roots: root_registry::OwnedV1,
}
pub type ConstructionQueryV1 = unsafe extern "C" fn(*const IntegerApiV1, *mut ConstructionV1) -> c_int;
/// Return 1..5 for the successful exact-Integer arithmetic contract, or zero.
/// The caller separately proves construction, lookup and Namespace admission.
pub type NumericQueryV5 = unsafe extern "C" fn(NativeEntry, *const IntegerApiV1) -> u32;
const _: () = {
    assert!(size_of::<ConstructionV1>() == 184);
    assert!(core::mem::offset_of!(ConstructionV1, roots) == 112);
};
