//! Rust primitive-prelude rung.
//!
//! This first N7.1 slice owns the integer protocol through the frozen IdApi
//! boundary. It is deliberately a separate cdylib: the Zig prelude remains the
//! oracle until the complete numeric, Boolean, Block, and System parity gates
//! pass.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

const RUST_PRELUDE_IDENTITY: &[u8] = b"rust-prelude-n7.1-integer\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Prelude: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api_ref() -> &'static IdApi {
    let runtime = runtime();
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains the selected floor for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn integer(api: &IdApi, value: Oop) -> Option<i64> {
    if value.is_null() || unsafe { (api.isSmallInt)(value) } == 0 {
        return None;
    }
    // SAFETY: isSmallInt established the floor's integer representation.
    Some(unsafe { (api.smallIntValue)(value) })
}

unsafe fn result(api: &IdApi, value: i128) -> Oop {
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return api.nilObj;
    }
    // SAFETY: the range check makes the conversion exact.
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe fn binary_int(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    operation: impl FnOnce(i128, i128) -> i128,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { integer(api, right_object) }) else {
        return api.nilObj;
    };
    unsafe { result(api, operation(left as i128, right as i128)) }
}

unsafe extern "C" fn integer_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { binary_int(self_object, args, count, |left, right| left + right) }
}

unsafe extern "C" fn integer_sub(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { binary_int(self_object, args, count, |left, right| left - right) }
}

unsafe extern "C" fn integer_mul(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { binary_int(self_object, args, count, |left, right| left * right) }
}

unsafe extern "C" fn integer_div(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { integer(api, right_object) }) else {
        return api.nilObj;
    };
    if right == 0 {
        return api.nilObj;
    }
    unsafe { result(api, (left as i128) / (right as i128)) }
}

unsafe extern "C" fn integer_mod(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { integer(api, right_object) }) else {
        return api.nilObj;
    };
    if right == 0 {
        return api.nilObj;
    }
    unsafe { result(api, (left as i128) % (right as i128)) }
}

unsafe fn compare(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    predicate: impl FnOnce(i64, i64) -> bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { integer(api, right_object) }) else {
        return api.nilObj;
    };
    if predicate(left, right) {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn integer_eq(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { compare(self_object, args, count, |left, right| left == right) }
}

unsafe extern "C" fn integer_lt(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { compare(self_object, args, count, |left, right| left < right) }
}

unsafe extern "C" fn integer_le(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { compare(self_object, args, count, |left, right| left <= right) }
}

unsafe extern "C" fn integer_gt(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { compare(self_object, args, count, |left, right| left > right) }
}

unsafe extern "C" fn integer_ge(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { compare(self_object, args, count, |left, right| left >= right) }
}

unsafe extern "C" fn integer_print_string(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let text = value.to_string();
    // SAFETY: `text` remains alive for the duration of mkString's copy.
    unsafe { (api.mkString)(text.as_ptr(), text.len()) }
}

unsafe extern "C" fn integer_as_float(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    // SAFETY: the floor owns the boxed Float constructor.
    unsafe { (api.mkFloat)(value as f64) }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = RUNTIME.get_or_init(|| Runtime {
        api: api_pointer as usize,
    });
    let integer_vt = api.smallint_vt;
    let methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"+\0", integer_add),
        (b"-\0", integer_sub),
        (b"*\0", integer_mul),
        (b"/\0", integer_div),
        (b"mod:\0", integer_mod),
        (b"=\0", integer_eq),
        (b"<\0", integer_lt),
        (b"<=\0", integer_le),
        (b">\0", integer_gt),
        (b">=\0", integer_ge),
        (b"printString\0", integer_print_string),
        (b"asFloat\0", integer_as_float),
    ];
    for (selector, method) in methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: integer_vt and each method are live for the process lifetime.
        unsafe { (api.method)(integer_vt, symbol, *method) };
    }
}

/// Identifies this opt-in N7.1 Rust prelude slice.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_prelude_rust_identity() -> *const c_char {
    RUST_PRELUDE_IDENTITY.as_ptr().cast()
}

/// Initialize the Rust prelude against the selected floor's IdApi.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    // SAFETY: bring_up enforces the null boundary and only retains the
    // process-owned IdApi address.
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_PRELUDE_IDENTITY, birnam_prelude_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable process-lifetime storage.
        let identity = unsafe { CStr::from_ptr(birnam_prelude_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_PRELUDE_IDENTITY);
    }
}
