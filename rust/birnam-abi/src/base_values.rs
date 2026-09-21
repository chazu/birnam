//! Completed Nil/Boolean identities created through Id before upper Runtime.
use crate::Oop;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ValuesV1 {
    pub version: u32,
    pub byte_size: usize,
    pub nil_behavior: Oop,
    pub boolean_behavior: Oop,
    pub nil: Oop,
    pub true_object: Oop,
    pub false_object: Oop,
}
const _: () = assert!(size_of::<ValuesV1>() == 56);
/// # Safety
/// Readable completed descriptor whose objects and native module remain live.
pub unsafe fn valid(pointer: *const ValuesV1) -> bool {
    if pointer.is_null() { return false; }
    let values = unsafe { *pointer };
    values.version == 1 && values.byte_size == size_of::<ValuesV1>()
        && [values.nil_behavior, values.boolean_behavior, values.nil, values.true_object, values.false_object].iter().all(|object| !object.is_null())
}
