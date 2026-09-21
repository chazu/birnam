//! Native root-anchor state, register capture and word scanning.
mod capture;
use birnam_abi::native_stack::{ApiV1, FactoryV4, FloorV1};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Anchor {
    api: ApiV1,
    floor: FloorV1,
    address: usize,
}
unsafe extern "C" fn set(api: *mut ApiV1, address: usize) {
    unsafe { (*api.cast::<Anchor>()).address = address };
}
unsafe extern "C" fn get(api: *const ApiV1) -> usize {
    unsafe { (*api.cast::<Anchor>()).address }
}
unsafe extern "C" fn public_set(context: *mut c_void, address: usize) {
    let owned = context.cast::<Anchor>();
    let floor = unsafe { (*owned).floor };
    // Admission may reenter; retain no reference to the anchor across it.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { set(owned.cast(), address) };
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Anchor {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), set: Some(public_set) },
        floor, address: 0,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Anchor>())) };
}
// The scanner runs below the live register-capture frame.
unsafe extern "C" fn scan(pointer: *const birnam_abi::trace_live::SinkV1, start: usize, bottom: usize) {
    let sink = unsafe { &*pointer };
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address.checked_add(size_of::<usize>()).is_some_and(|end| end <= bottom) {
        // Read and emit one complete word at a time, preserving callback effects.
        let candidate = unsafe { (address as *const usize).read() };
        let status = unsafe { (sink.emit)(sink.context, candidate) };
        if status != birnam_abi::storage::OK {
            eprintln!("birnam-id invariant failed: GC tracer rejected a candidate");
            std::process::abort();
        }
        address += size_of::<usize>();
    }
}
// Birnam currently targets arm64 macOS. This boundary outlives startup frames.
unsafe extern "C" fn thread_bottom() -> usize {
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stackaddr_np(thread: *mut c_void) -> *mut c_void;
    }
    unsafe { pthread_get_stackaddr_np(pthread_self()).addr() }
}
static FACTORY: FactoryV4 = FactoryV4 {
    version: 4, byte_size: size_of::<FactoryV4>(), create: Some(create), destroy: Some(destroy),
    set: Some(set), get: Some(get), scan: Some(scan), capture: Some(capture::capture), thread_bottom: Some(thread_bottom),
};
/// # Safety
/// Owner-thread operations require the declared descriptor and code lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_stack_factory_v4() -> *const FactoryV4 { &FACTORY }

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;

/// Publish complete primitive operations through Id before Runtime exists.
unsafe extern "C" fn initialize_native(_: *const birnam_abi::id_protocol::Protocol, _: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    if count != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    Outcome::loaded(Operand::word(&FACTORY as *const FactoryV4 as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32, initialize: Some(initialize_native),
};
