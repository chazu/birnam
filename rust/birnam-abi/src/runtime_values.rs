//! Completed value operations required before Runtime publishes execution.
use crate::{integer, float, word, string};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApisV1 {
    pub version: u32,
    pub byte_size: usize,
    pub integer: *mut integer::IntegerApiV1,
    pub float: *mut float::FloatApiV1,
    pub word: *mut word::WordApiV1,
    pub string: *mut string::StringApiV1,
}
const _: () = assert!(size_of::<ApisV1>() == 48);
/// # Safety
/// Descriptors are readable and their owners and code remain live through Runtime.
pub unsafe fn valid(pointer: *const ApisV1) -> bool {
    if pointer.is_null() { return false; }
    let value = unsafe { &*pointer };
    value.version == 1 && value.byte_size == size_of::<ApisV1>()
        && unsafe { integer::valid(value.integer) && float::valid(value.float)
            && word::valid(value.word) && string::valid(value.string) }
}
