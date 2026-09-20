//! Interned publication policy; Id retains Symbol and weak-mirror representation.
use birnam_abi::Oop;
use birnam_abi::namespace::{BindV1, InternV1, MirrorV1};
use core::ffi::{c_int, c_void};

pub(crate) unsafe extern "C" fn publish(
    namespace: *mut c_void,
    bind: BindV1,
    floor: *mut c_void,
    intern: InternV1,
    mirror: MirrorV1,
    bytes: *const u8,
    length: usize,
    value: Oop,
    result: *mut Oop,
) -> c_int {
    let symbol = unsafe { intern(floor, bytes, length) };
    if symbol.is_null() {
        return 0;
    }
    let exported = unsafe { bind(namespace, symbol, value) };
    unsafe { mirror(floor, symbol, value) };
    unsafe { *result = exported };
    1
}
