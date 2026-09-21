//! Native finalizer eligibility and inline callback attachments.
use birnam_abi::finalizers::{ApiV1, FactoryV1, FloorV1, SlotV1};
use birnam_abi::{Finalizer, Oop};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Capability {
    api: ApiV1,
    floor: FloorV1,
}

unsafe extern "C" fn slot(permanent: c_int, finalizer: Finalizer) -> SlotV1 {
    if permanent != 0 { SlotV1::EMPTY } else { SlotV1 { callback: Some(finalizer) } }
}

unsafe extern "C" fn public_set(context: *mut c_void, object: Oop, finalizer: Finalizer) {
    let floor = unsafe { (*context.cast::<Capability>()).floor };
    // No state borrow crosses admission, which may reenter this capability.
    unsafe { floor.admit.unwrap()(floor.context) };
    let mut permanent = 0;
    if unsafe { floor.read_permanent.unwrap()(floor.context, object, &mut permanent) } == 0 {
        return;
    }
    let attachment = unsafe { slot(permanent, finalizer) };
    if attachment.callback.is_some() {
        unsafe { floor.replace.unwrap()(floor.context, object, attachment) };
    }
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>()
        || floor.admit.is_none() || floor.read_permanent.is_none() || floor.replace.is_none()
    { return ptr::null_mut(); }
    let mut owned = Box::new(Capability {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), set: Some(public_set) },
        floor,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Capability>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy),
    slot: Some(slot),
};
/// # Safety
/// The immutable descriptor and code outlive all retaining consumers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_finalizers_factory_v1() -> *const FactoryV1 { &FACTORY }

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;

/// Publish the complete primitive description algorithms through Id loading.
unsafe extern "C" fn initialize_native(_: *const birnam_abi::id_protocol::Protocol, _: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    if count != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    Outcome::loaded(Operand::word(&FACTORY as *const FactoryV1 as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32, initialize: Some(initialize_native),
};
