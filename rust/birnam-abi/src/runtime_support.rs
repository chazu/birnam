//! Completed factory operations supplied to upper Runtime construction.
//! These contain no Runtime callbacks or future-filled dependency slots.
use crate::{method_observation, storage_observation, deferred_identity, deferred_index,
    dispatch_cache, guard_verdict};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoriesV1 {
    pub version: u32,
    pub byte_size: usize,
    pub observation: method_observation::FactoryV1,
    pub storage: storage_observation::FactoryV1,
    pub identity: deferred_identity::FactoryV1,
    pub index: deferred_index::FactoryV1,
    pub cache: dispatch_cache::FactoryV2,
    pub verdict: guard_verdict::FactoryV1,
}
const _: () = assert!(size_of::<FactoriesV1>() == 352);

/// # Safety
/// The descriptor is readable and each operation's code remains pinned. This
/// check invokes no operations and transfers no ownership.
pub unsafe fn valid(pointer: *const FactoriesV1) -> bool {
    if pointer.is_null() { return false; }
    if unsafe { (*pointer).version != 1 || (*pointer).byte_size != size_of::<FactoriesV1>() } { return false; }
    let value = unsafe { &*pointer };
    unsafe { method_observation::valid_factory(&value.observation)
        && storage_observation::valid(&value.storage)
        && deferred_identity::valid(&value.identity)
        && deferred_index::valid(&value.index)
        && dispatch_cache::valid(&value.cache)
        && guard_verdict::valid(&value.verdict) }
}
