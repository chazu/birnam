//! Existing Boolean methods, loaded independently of Prelude (Design 81U).
//! Native base construction precedes later String/Block extensions.
#![allow(clippy::missing_safety_doc)]
mod base;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_char;
use core::ptr;
use std::sync::OnceLock;

static API: OnceLock<usize> = OnceLock::new();
fn abort(message: &str) -> ! {
    eprintln!("birnam Boolean: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let api = *API.get().unwrap_or_else(|| abort("module is not initialized"));
    // The host pins this module and the selected floor table for process life.
    unsafe { &*(api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
}

unsafe fn boolean_value(api: &IdApi, value: Oop) -> Option<bool> {
    if value == api.trueObj {
        Some(true)
    } else if value == api.falseObj {
        Some(false)
    } else {
        None
    }
}

unsafe extern "C" fn boolean_true_if_true_if_false(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(when_true) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let _ = self_object;
    unsafe { send0(api, when_true[0], b"value\0") }
}

unsafe extern "C" fn boolean_false_if_true_if_false(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(when_true) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let _ = self_object;
    unsafe { send0(api, when_true[1], b"value\0") }
}

unsafe extern "C" fn boolean_true_if_true(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(branch) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let _ = self_object;
    unsafe { send0(api, branch, b"value\0") }
}

unsafe extern "C" fn boolean_false_if_true(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    api.nilObj
}

unsafe extern "C" fn boolean_true_if_false(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    api.nilObj
}

unsafe extern "C" fn boolean_false_if_false(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(branch) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let _ = self_object;
    unsafe { send0(api, branch, b"value\0") }
}

unsafe extern "C" fn boolean_and_block(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj {
        unsafe { send0(api, block, b"value\0") }
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn boolean_or_block(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj {
        api.trueObj
    } else {
        unsafe { send0(api, block, b"value\0") }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

static CONTROL_COMPLETE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
unsafe fn initialize_control(id: *const birnam_abi::id_protocol::Protocol, api: birnam_abi::id_protocol::Operand) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Outcome, Operand};
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some(api) = api.as_word() else { return invalid(); };
    let api = api as *mut IdApi;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver || !base::matches(&*api) } { return invalid(); }
    if let Some(selected) = API.get() {
        return if CONTROL_COMPLETE.load(std::sync::atomic::Ordering::Acquire) && *selected == api.addr() { Outcome::loaded(Operand::unit()) } else { invalid() };
    }
    API.set(api.addr()).ok().expect("serialized Boolean construction");
    let api = unsafe { &*api };
    let true_vt = unsafe { (api.vtableOf)(api.trueObj) };
    let false_vt = unsafe { birnam_abi::ensure_private_vtable(api, api.falseObj) };
    let methods: &[(&[u8], birnam_abi::Method, birnam_abi::Method)] = &[
        (b"ifTrue:ifFalse:\0", boolean_true_if_true_if_false, boolean_false_if_true_if_false),
        (b"ifTrue:\0", boolean_true_if_true, boolean_false_if_true),
        (b"ifFalse:\0", boolean_true_if_false, boolean_false_if_false),
    ];
    for (name, when_true, when_false) in methods {
        let selector = unsafe { intern(api, name) };
        unsafe { (api.method)(true_vt, selector, *when_true); (api.method)(false_vt, selector, *when_false); }
    }
    for (name, method) in [(c"and:", boolean_and_block as birnam_abi::Method), (c"or:", boolean_or_block as birnam_abi::Method)] {
        let selector = unsafe { (api.intern)(name.as_ptr()) };
        unsafe { (api.method)(true_vt, selector, method) };
    }
    CONTROL_COMPLETE.store(true, std::sync::atomic::Ordering::Release);
    Outcome::loaded(Operand::unit())
}
