//! Existing Word methods, loaded independently of Prelude (Design 81S).
//! Canonical objects and representation remain in Id for this checkpoint.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;
static API: OnceLock<usize> = OnceLock::new();
fn abort(message: &str) -> ! {
    eprintln!("birnam Word: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let api = *API
        .get()
        .unwrap_or_else(|| abort("module_init was not called"));
    // The host retains the selected Id table and this module for process life.
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

unsafe fn make_error(api: &IdApi, kind: Oop, message: Oop) -> Oop {
    // Error owns its representation and Object protocol in the standalone
    // Error rung. Prelude faults are intentionally lazy: before that rung is
    // loaded, the floor has no catchable Error value and answers nil, matching
    // the native prelude's `Error` import behavior.
    if unsafe { (api.isExported)(c"Error".as_ptr()) } == 0 {
        return api.nilObj;
    }
    let factory = unsafe { (api.import)(c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let args = [kind, message];
    // SAFETY: the standalone Error factory owns this selector and the stack
    // argument array remains live for the duration of the synchronous send.
    unsafe { (api.send)(factory, selector, args.as_ptr(), args.len()) }
}

unsafe fn numeric_error(api: &IdApi, kind: &[u8], message: &[u8]) -> Oop {
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (api.mkString)(message.as_ptr(), message.len()) };
    unsafe { make_error(api, kind, message) }
}

unsafe fn word_value(api: &IdApi, value: Oop) -> Option<u64> {
    if value.is_null() || unsafe { (api.isWord)(value) } == 0 {
        return None;
    }
    Some(unsafe { (api.wordValue)(value) })
}

unsafe extern "C" fn word_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    let text = format!("0x{value:x}");
    unsafe { (api.mkString)(text.as_ptr(), text.len()) }
}

unsafe extern "C" fn word_eq(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { word_value(api, right_object) }) else {
        return api.falseObj;
    };
    if left == right {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn word_as_integer(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    if value > i64::MAX as u64 {
        return unsafe { numeric_error(api, b"overflow\0", b"word out of Integer range") };
    }
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

/// Install the existing Word protocol on the canonical floor behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let _ = API.get_or_init(|| api_pointer as usize);
    let api = unsafe { &*api_pointer };
    let word_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"printString\0", word_print_string),
        (b"=\0", word_eq),
        (b"asInteger\0", word_as_integer),
    ];
    for (selector, method) in word_methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: word_vt and each callback are process-lifetime values.
        unsafe { (api.method)(api.word_vt, symbol, *method) };
    }
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::integer_dependency!();
