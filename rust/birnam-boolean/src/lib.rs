//! Existing Boolean methods, loaded independently of Prelude (Design 81U).
//! Native base construction precedes later String/Block extensions.
#![allow(clippy::missing_safety_doc)]
mod base;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

static API: OnceLock<usize> = OnceLock::new();
fn abort(message: &str) -> ! {
    eprintln!("birnam Boolean: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let api = *API.get().unwrap_or_else(|| abort("module_init was not called"));
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

unsafe extern "C" fn boolean_and(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(argument) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj {
        argument
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn boolean_or(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(argument) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj {
        api.trueObj
    } else {
        argument
    }
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

unsafe extern "C" fn boolean_not(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    match unsafe { boolean_value(api, self_object) } {
        Some(true) => api.falseObj,
        Some(false) => api.trueObj,
        None => api.nilObj,
    }
}

unsafe extern "C" fn boolean_print_string(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let text = match unsafe { boolean_value(api, self_object) } {
        Some(true) => b"true".as_slice(),
        Some(false) => b"false".as_slice(),
        None => return api.nilObj,
    };
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

/// Install the existing Boolean protocol through ordinary object-model messages.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = API.get_or_init(|| api_pointer as usize);
    let boolean_vt = unsafe { (api.vtableOf)(api.trueObj) };
    let false_vt = unsafe { birnam_abi::ensure_private_vtable(api, api.falseObj) };
    if false_vt.is_null() || false_vt == boolean_vt {
        abort("could not establish False's private Boolean behavior");
    }
    let boolean_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"ifTrue:ifFalse:\0", boolean_true_if_true_if_false),
        (b"ifTrue:\0", boolean_true_if_true),
        (b"ifFalse:\0", boolean_true_if_false),
        (b"&\0", boolean_and),
        (b"|\0", boolean_or),
        (b"and:\0", boolean_and_block),
        (b"or:\0", boolean_or_block),
        (b"not\0", boolean_not),
        (b"printString\0", boolean_print_string),
    ];
    for (selector, method) in boolean_methods {
        if base::printing_ready() && *selector == b"printString\0" { continue; }
        if base::matches(api) && [b"&\0".as_slice(), b"|\0".as_slice(), b"not\0".as_slice()].contains(selector) { continue; }
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: both singleton booleans share this vtable for the process lifetime.
        unsafe { (api.method)(boolean_vt, symbol, *method) };
    }
    let false_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"ifTrue:ifFalse:\0", boolean_false_if_true_if_false),
        (b"ifTrue:\0", boolean_false_if_true),
        (b"ifFalse:\0", boolean_false_if_false),
    ];
    for (selector, method) in false_methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: False owns a process-lifetime private vtable delegating to Boolean.
        unsafe { (api.method)(false_vt, symbol, *method) };
    }
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::string_dependency!();

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
