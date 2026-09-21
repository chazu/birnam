//! Late String-to-Vector adapter, installed at Collection's original point.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::string_collection::ApiV1;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Collection supplies its original selection, even when registration reenters
// with another table before the first split installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("String collection methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
unsafe fn string_copy(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
    if object.is_null()
        || unsafe { (api.vtableOf)(object) } != unsafe { birnam_abi::string::behavior() }
    {
        return None;
    }
    let mut length = 0;
    let bytes = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
    if length == 0 {
        return Some(Vec::new());
    }
    if bytes.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec())
}

unsafe extern "C" fn string_split(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(separator) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(separator) } != unsafe { birnam_abi::string::behavior() } {
        let result = unsafe { birnam_abi::vector::empty() };
        return unsafe { birnam_abi::vector::push(result, self_object) };
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(separator) = (unsafe { string_copy(api, separator) }) else {
        return api.nilObj;
    };
    let mut result = unsafe { birnam_abi::vector::empty() };
    if separator.is_empty() {
        let item = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
        return unsafe { birnam_abi::vector::push(result, item) };
    }
    let mut start = 0usize;
    let mut cursor = 0usize;
    while cursor + separator.len() <= bytes.len() {
        if bytes[cursor..cursor + separator.len()] == separator[..] {
            let item = unsafe {
                (birnam_abi::string::make)(bytes[start..cursor].as_ptr(), cursor - start)
            };
            result = unsafe { birnam_abi::vector::push(result, item) };
            cursor += separator.len();
            start = cursor;
        } else {
            cursor += 1;
        }
    }
    let item = unsafe { (birnam_abi::string::make)(bytes[start..].as_ptr(), bytes.len() - start) };
    unsafe { birnam_abi::vector::push(result, item) }
}

unsafe extern "C" fn install(registration: *mut IdApi, execution: *const IdApi) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid String collection installation"
    );
    // Select execution before interning/registration callbacks can invoke split.
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(
            birnam_abi::string::behavior(),
            (api.intern)(c"split:".as_ptr()),
            string_split,
        );
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, string, vector, parent]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let (Some(api), Some(string), Some(vector)) = (api.as_word(), string.as_word(), vector.as_word()) else { return invalid_native(); };
    let api = api as *mut IdApi;
    let Some(parent) = parent.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || unsafe { birnam_abi::string::attach(string as _) } != 1 || unsafe { birnam_abi::vector::attach(vector as _) } != 1 { return invalid_native(); }
    if EXECUTION_API.get().is_some_and(|selected| *selected != api.addr()) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::word((&API as *const ApiV1).addr())), _ => return invalid_native(),
    }
    READY.store(true, Ordering::Release);
    unsafe { birnam_abi::set_vtable_parent(&*api, birnam_abi::string::behavior(), parent); install(api, api); }
    NATIVE_PHASE.store(2, std::sync::atomic::Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const ApiV1).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
