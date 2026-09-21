//! Shared exact-String and printable-value borrowed byte policy (81AK).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::text_view::{ApiV1, BytesV1};
use birnam_abi::{IdApi, Oop};
use core::ffi::c_char;
use core::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
static READY: AtomicBool = AtomicBool::new(false);

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
}

unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<&[u8]> {
    if value.is_null()
        || unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() }
    {
        return None;
    }
    let mut length = 0usize;
    let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    if length == 0 {
        return Some(&[]);
    }
    if bytes.is_null() {
        return None;
    }
    // SAFETY: stringBytes returns the floor-owned String payload for the
    // duration of this callback; the write completes before the borrow ends.
    Some(unsafe { core::slice::from_raw_parts(bytes, length) })
}

unsafe fn printable_string(api: &IdApi, value: Oop) -> Option<&[u8]> {
    if let Some(bytes) = unsafe { string_bytes(api, value) } {
        return Some(bytes);
    }
    if value.is_null() {
        return None;
    }
    let rendered = unsafe { send0(api, value, b"printString\0") };
    unsafe { string_bytes(api, rendered) }
}

unsafe extern "C" fn exact(api: *const IdApi, value: Oop) -> BytesV1 {
    assert!(!api.is_null(), "text view received a null IdApi");
    BytesV1::from_slice(unsafe { string_bytes(&*api, value) })
}
unsafe extern "C" fn printable(api: *const IdApi, value: Oop) -> BytesV1 {
    assert!(!api.is_null(), "text view received a null IdApi");
    BytesV1::from_slice(unsafe { printable_string(&*api, value) })
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    exact: Some(exact),
    printable: Some(printable),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_text_view_provider_v1() -> *const ApiV1 {
    if READY.load(Ordering::Acquire) {
        &API
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([string]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(string) = string.as_word() else { return invalid(); };
    if unsafe { birnam_abi::string::attach(string as _) } != 1 { return invalid(); }
    READY.store(true, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const ApiV1).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
