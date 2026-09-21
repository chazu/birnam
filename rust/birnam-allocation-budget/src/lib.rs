//! Allocation charges since collection, independent of storage and GC policy.
use birnam_abi::allocation_budget::FactoryV1;
use core::ffi::{c_int, c_void};

struct Budget {
    bytes: usize,
}
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Budget { bytes: 0 })).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Budget>())) };
}
unsafe extern "C" fn read(context: *const c_void) -> usize {
    unsafe { (*context.cast::<Budget>()).bytes }
}
unsafe extern "C" fn replace(context: *mut c_void, bytes: usize) {
    unsafe { (*context.cast::<Budget>()).bytes = bytes };
}
unsafe extern "C" fn charge(context: *mut c_void, bytes: usize) -> c_int {
    let Some(total) = unsafe { read(context) }.checked_add(bytes) else {
        return 0;
    };
    unsafe { replace(context, total) };
    1
}
unsafe extern "C" fn preview(context: *const c_void, bytes: usize, out: *mut usize) -> c_int {
    let Some(total) = unsafe { read(context) }.checked_add(bytes) else {
        return 0;
    };
    unsafe { out.write(total) };
    1
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1,
    byte_size: size_of::<FactoryV1>(),
    create: Some(create),
    destroy: Some(destroy),
    read: Some(read),
    replace: Some(replace),
    charge: Some(charge),
    preview: Some(preview),
};
/// # Safety
/// The host pins this immutable descriptor and implementation for every owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_allocation_budget_factory_v1() -> *const FactoryV1 {
    &FACTORY
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int {
    1
}

#[cfg(test)]
mod tests;

/// Publish complete primitive operations through Id before Runtime exists.
unsafe extern "C" fn initialize_native(_: *const birnam_abi::id_protocol::Protocol, _: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    if count != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    Outcome::loaded(Operand::word(&FACTORY as *const FactoryV1 as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32, initialize: Some(initialize_native),
};
