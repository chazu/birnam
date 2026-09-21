//! Completed native Namespace, shared with later language-facing adapters.
use crate::{Oop, namespace::ApiV8};
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub api: *const ApiV8,
    pub storage: *mut c_void,
    pub receiver: Oop,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
}
const _: () = assert!(size_of::<OwnedV1>() == 56);
/// # Safety
/// Readable completed descriptor; code, receiver and storage remain live.
pub unsafe fn valid(pointer: *const OwnedV1) -> bool {
    if pointer.is_null() { return false; }
    let owner = unsafe { *pointer };
    owner.version == 1 && owner.byte_size == size_of::<OwnedV1>()
        && !owner.context.is_null() && !owner.storage.is_null() && !owner.receiver.is_null()
        && owner.destroy.is_some() && unsafe { crate::namespace::valid(owner.api) }
}
