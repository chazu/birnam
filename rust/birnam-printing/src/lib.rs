//! Object and Symbol printing defaults, available before collections.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::printing::ApiV1;
use birnam_abi::{IdApi, Oop};
use core::ffi::c_char;
use core::ptr;
use std::ffi::CStr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Installation retains its original selection, even when registration reenters
// with another table before the first printing installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("Printing methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
}

unsafe extern "C" fn object_print_string(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::string::make)(b"anObject".as_ptr(), b"anObject".len()) }
}

unsafe extern "C" fn object_as_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { send0(api, self_object, b"printString\0") }
}

unsafe extern "C" fn symbol_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let name = unsafe { (api.symbolName)(self_object) };
    if name.is_null() {
        return api.nilObj;
    }
    // SAFETY: self_object remains rooted for this active send; copy its
    // borrowed name before mkString can allocate or collect.
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    let mut rendered = Vec::with_capacity(name.len().saturating_add(1));
    rendered.push(b'#');
    rendered.extend_from_slice(name);
    unsafe { (birnam_abi::string::make)(rendered.as_ptr(), rendered.len()) }
}

unsafe extern "C" fn install(registration: *mut IdApi, execution: *const IdApi) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid printing installation"
    );
    // Select execution before interning/registration can invoke methods or reenter.
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    let print_string = unsafe { intern(api, b"printString\0") };
    let as_string = unsafe { intern(api, b"asString\0") };
    let symbol_print_string = print_string;
    unsafe {
        (api.method)(api.object_vt, print_string, object_print_string);
        (api.method)(api.object_vt, as_string, object_as_string);
        (api.method)(
            api.symbol_vt,
            symbol_print_string,
            self::symbol_print_string,
        );
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_printing_provider_v1() -> *const ApiV1 {
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

static NATIVE_BUSY: AtomicBool = AtomicBool::new(false);
static NATIVE_COMPLETE: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn initialize_native(id: *const birnam_abi::id_protocol::Protocol, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([api, string]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(api), Some(string)) = (api.as_word(), string.as_word()) else { return invalid(); };
    let api = api as *mut IdApi;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || EXECUTION_API.get().is_some_and(|selected| *selected != api.addr())
        || unsafe { birnam_abi::string::attach(string as _) } != 1 { return invalid(); }
    if NATIVE_COMPLETE.load(Ordering::Acquire) { return Outcome::loaded(Operand::word((&API as *const ApiV1).addr())); }
    if NATIVE_BUSY.swap(true, Ordering::AcqRel) { return invalid(); }
    READY.store(true, Ordering::Release);
    unsafe { install(api, api) };
    NATIVE_COMPLETE.store(true, Ordering::Release);
    NATIVE_BUSY.store(false, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const ApiV1).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32,
    initialize: Some(initialize_native),
};
