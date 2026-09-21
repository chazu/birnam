//! Native Bootstrap ownership of Id module initialization results.
use crate::{
    id_protocol::{Operand, Outcome},
    root_registry,
};
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub load: unsafe extern "C" fn(*mut c_void, *const u8, usize, *const Operand, usize) -> Outcome,
    /// Transfer an existing, working early root set to a complete root registry.
    /// Returns a native status. Call before collection, outside module entry.
    pub adopt_roots: unsafe extern "C" fn(*mut c_void, *const root_registry::OwnedV1) -> u32,
    /// Release one retained Object/Protocol result. Returns one once, else zero.
    pub release_result: unsafe extern "C" fn(*mut c_void, Operand) -> usize,
    pub destroy: unsafe extern "C" fn(*mut c_void),
}
const _: () = assert!(size_of::<OwnedV1>() == 56);
/// # Safety
/// Readable initialized native owner with live code and callback contexts.
pub unsafe fn valid(pointer: *const OwnedV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let owner = unsafe { *pointer };
    owner.version == 1 && owner.byte_size == size_of::<OwnedV1>() && !owner.context.is_null()
}
