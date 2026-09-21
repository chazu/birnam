//! Completed private-behavior ownership and native enumeration above Id.
use crate::{Oop, vtable_enumeration::ApiV2};
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub enumeration: *mut ApiV2,
    pub owner: Option<unsafe extern "C" fn(*const c_void, Oop) -> Oop>,
    pub set_owner: Option<unsafe extern "C" fn(*mut c_void, Oop, Oop)>,
    pub retire: Option<unsafe extern "C" fn(*mut c_void, *const usize, usize)>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
}
const _: () = assert!(size_of::<OwnedV1>() == 64);
/// # Safety
/// Readable completed descriptor with live records, enumeration and code.
pub unsafe fn valid(pointer: *const OwnedV1) -> bool {
    if pointer.is_null() { return false; }
    let owner = unsafe { *pointer };
    owner.version == 1 && owner.byte_size == size_of::<OwnedV1>() && !owner.context.is_null()
        && unsafe { crate::vtable_enumeration::valid(owner.enumeration) }
        && owner.owner.is_some() && owner.set_owner.is_some() && owner.retire.is_some() && owner.destroy.is_some()
}
