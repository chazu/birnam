//! Complete collection records and controls, owned above Id before activation.
use crate::{Oop, allocation_budget, collection_phase, gc_settings, native_stack};
use core::ffi::c_void;
pub const MARKED: u32 = 1;
pub const PERMANENT: u32 = 2;
pub const MISSING: u32 = u32::MAX;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RecordV1 {
    pub object: Oop,
    pub flags: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ViewV1 {
    pub entries: *const RecordV1,
    pub count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RecordsV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub insert: unsafe extern "C" fn(*mut c_void, Oop, u32) -> i32,
    pub get: unsafe extern "C" fn(*const c_void, usize) -> u32,
    pub retain: unsafe extern "C" fn(*mut c_void, usize) -> i32,
    pub mark: unsafe extern "C" fn(*mut c_void, usize) -> i32,
    pub clear: unsafe extern "C" fn(*mut c_void),
    /// Copy before any later operation on this owner; no callbacks run here.
    pub snapshot: unsafe extern "C" fn(*mut c_void) -> ViewV1,
    pub remove_dead: unsafe extern "C" fn(*mut c_void, usize) -> i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AccessV1 {
    pub context: *mut c_void,
    pub settings: unsafe extern "C" fn(*mut c_void),
    pub stack: unsafe extern "C" fn(*mut c_void),
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub records: *const RecordsV1,
    pub stack: *mut native_stack::ApiV1,
    pub stack_factory: *const native_stack::FactoryV2,
    pub settings: *mut gc_settings::ApiV1,
    pub settings_factory: *const gc_settings::FactoryV1,
    pub budget_context: *mut c_void,
    pub budget_factory: *const allocation_budget::FactoryV1,
    pub phase_context: *mut c_void,
    pub phase_factory: *const collection_phase::FactoryV1,
    pub set_access: unsafe extern "C" fn(*mut c_void, AccessV1),
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub collect: crate::collect::CollectV1,
}
const _: () = {
    assert!(size_of::<RecordV1>() == 16 && size_of::<ViewV1>() == 16);
    assert!(size_of::<RecordsV1>() == 80 && size_of::<OwnedV1>() == 120);
};
/// # Safety
/// Readable descriptor and predecessor tables; all contexts and code stay live.
pub unsafe fn valid(pointer: *const OwnedV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let owner = unsafe { *pointer };
    if owner.version != 1
        || owner.byte_size != size_of::<OwnedV1>()
        || owner.context.is_null()
        || owner.records.is_null()
        || owner.budget_context.is_null()
        || owner.phase_context.is_null()
    {
        return false;
    }
    let records = unsafe { *owner.records };
    records.version == 1
        && records.byte_size == size_of::<RecordsV1>()
        && !records.context.is_null()
        && unsafe {
            native_stack::valid(owner.stack)
                && native_stack::valid_factory(owner.stack_factory)
                && gc_settings::valid(owner.settings)
                && gc_settings::valid_factory(owner.settings_factory)
                && allocation_budget::valid(owner.budget_factory)
                && collection_phase::valid(owner.phase_factory)
        }
}
