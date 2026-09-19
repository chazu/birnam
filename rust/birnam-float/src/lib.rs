//! Existing Float methods, loaded independently of Prelude.
//! Representation remains in Id for this first checkpoint (Design 81Q).
#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

static API: OnceLock<usize> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Float: {message}");
    std::process::abort();
}

unsafe fn api_ref() -> &'static IdApi {
    let api = *API
        .get()
        .unwrap_or_else(|| abort("module_init was not called"));
    // SAFETY: the host pins the selected floor table and this module's code.
    unsafe { &*(api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: callers supply static NUL-terminated selectors.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

#[derive(Clone, Copy)]
enum Numeric {
    Integer(i64),
    Float(f64),
}

unsafe fn numeric(api: &IdApi, value: Oop) -> Option<Numeric> {
    if value.is_null() {
        return None;
    }
    if unsafe { (birnam_abi::integer::is_integer)(value) } != 0 {
        return Some(Numeric::Integer(unsafe {
            (birnam_abi::integer::value)(value)
        }));
    }
    if unsafe { (api.isFloat)(value) } != 0 {
        return Some(Numeric::Float(unsafe { (api.floatValue)(value) }));
    }
    None
}

unsafe fn float_self(api: &IdApi, value: Oop) -> Option<f64> {
    if value.is_null() || unsafe { (api.isFloat)(value) } == 0 {
        return None;
    }
    Some(unsafe { (api.floatValue)(value) })
}

unsafe fn float_binary(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    operation: impl FnOnce(f64, f64) -> f64,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, b"typeMismatch\0", b"Float arithmetic needs a number")
        };
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    unsafe { (api.mkFloat)(operation(left, right)) }
}

unsafe extern "C" fn float_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_binary(self_object, args, count, |left, right| left + right) }
}

unsafe extern "C" fn float_sub(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_binary(self_object, args, count, |left, right| left - right) }
}

unsafe extern "C" fn float_mul(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_binary(self_object, args, count, |left, right| left * right) }
}

unsafe extern "C" fn float_div(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, b"typeMismatch\0", b"Float arithmetic needs a number")
        };
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    if right == 0.0 {
        return unsafe { numeric_error(api, b"zeroDivide\0", b"division by zero") };
    }
    unsafe { (api.mkFloat)(left / right) }
}

unsafe fn float_compare(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    predicate: impl FnOnce(f64, f64) -> bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe { numeric_error(api, b"typeMismatch\0", b"comparison needs a number") };
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    if predicate(left, right) {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn float_eq(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(Numeric::Float(right)) = (unsafe { numeric(api, right_object) }) else {
        return api.falseObj;
    };
    if left == right {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn float_lt(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_compare(self_object, args, count, |left, right| left < right) }
}

unsafe extern "C" fn float_le(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_compare(self_object, args, count, |left, right| left <= right) }
}

unsafe extern "C" fn float_gt(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_compare(self_object, args, count, |left, right| left > right) }
}

unsafe extern "C" fn float_ge(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { float_compare(self_object, args, count, |left, right| left >= right) }
}

unsafe extern "C" fn float_negated(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { (api.mkFloat)(-value) }
}

unsafe fn float_to_integer(api: &IdApi, value: f64) -> Oop {
    if !value.is_finite() {
        return unsafe { numeric_error(api, b"overflow\0", b"float out of Integer range") };
    }
    let value = value.trunc();
    // A SmallInt cannot represent 2^63 even though that boundary rounds from
    // i64::MAX when converted to f64. Reject it before Rust's saturating cast.
    if value < i64::MIN as f64 || value >= 9_223_372_036_854_775_808.0 {
        return unsafe { numeric_error(api, b"overflow\0", b"float out of Integer range") };
    }
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

unsafe extern "C" fn float_as_integer(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { float_to_integer(api, value) }
}

unsafe extern "C" fn float_abs(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { (api.mkFloat)(value.abs()) }
}

unsafe extern "C" fn float_floor(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { float_to_integer(api, value.floor()) }
}

unsafe extern "C" fn float_ceiling(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { float_to_integer(api, value.ceil()) }
}

unsafe extern "C" fn float_rounded(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { float_to_integer(api, value.round()) }
}

unsafe extern "C" fn float_sqrt(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    if value < 0.0 {
        return unsafe { numeric_error(api, b"domain\0", b"square root of a negative number") };
    }
    unsafe { (api.mkFloat)(value.sqrt()) }
}

unsafe extern "C" fn float_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { float_self(api, self_object) }) else {
        return api.nilObj;
    };
    let mut text = value.to_string();
    if !text.contains('.') && !text.contains('e') && !text.contains('E') {
        text.push_str(".0");
    }
    // SAFETY: `text` remains alive for the duration of mkString's copy.
    unsafe { (api.mkString)(text.as_ptr(), text.len()) }
}

unsafe fn is_error(api: &IdApi, value: Oop) -> bool {
    if value.is_null() || unsafe { (api.isExported)(c"ErrorVt".as_ptr()) } == 0 {
        return false;
    }
    let error_vt = unsafe { (api.import)(c"ErrorVt".as_ptr()) };
    !error_vt.is_null() && unsafe { (api.vtableOf)(value) } == error_vt
}

unsafe fn make_error(api: &IdApi, kind: Oop, message: Oop) -> Oop {
    // Error owns its representation and Object protocol in the standalone
    // Error rung. Float faults are intentionally lazy: before that rung is
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

/// Install the existing Float protocol on the canonical floor behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = API.get_or_init(|| api_pointer as usize);
    let float_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"+\0", float_add),
        (b"-\0", float_sub),
        (b"*\0", float_mul),
        (b"/\0", float_div),
        (b"=\0", float_eq),
        (b"<\0", float_lt),
        (b"<=\0", float_le),
        (b">\0", float_gt),
        (b">=\0", float_ge),
        (b"negated\0", float_negated),
        (b"asInteger\0", float_as_integer),
        (b"abs\0", float_abs),
        (b"floor\0", float_floor),
        (b"ceiling\0", float_ceiling),
        (b"rounded\0", float_rounded),
        (b"sqrt\0", float_sqrt),
        (b"printString\0", float_print_string),
    ];
    for (selector, method) in float_methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: float_vt and each method are live for the process lifetime.
        unsafe { (api.method)(api.float_vt, symbol, *method) };
    }
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::integer_dependency!();
