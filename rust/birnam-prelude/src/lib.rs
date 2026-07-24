//! Rust primitive-prelude rung.
//!
//! This N7.1 slice owns the primitive protocol through the frozen IdApi
//! boundary. It is deliberately a separate cdylib: the Zig prelude remains the
//! oracle until the complete numeric, Boolean, Block, and System parity gates
//! pass.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::io::{self, Write};
use std::sync::OnceLock;

const RUST_PRELUDE_IDENTITY: &[u8] = b"rust-prelude-n7.1-core\0";

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

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
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

#[derive(Clone, Copy)]
enum Numeric {
    Integer(i64),
    Float(f64),
}

unsafe fn numeric(api: &IdApi, value: Oop) -> Option<Numeric> {
    if value.is_null() {
        return None;
    }
    if unsafe { (api.isSmallInt)(value) } != 0 {
        return Some(Numeric::Integer(unsafe { (api.smallIntValue)(value) }));
    }
    if unsafe { (api.isFloat)(value) } != 0 {
        return Some(Numeric::Float(unsafe { (api.floatValue)(value) }));
    }
    None
}

unsafe fn numeric_binary(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    integer_operation: impl FnOnce(i128, i128) -> i128,
    float_operation: impl FnOnce(f64, f64) -> f64,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { numeric(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
    };
    match (left, right) {
        (Numeric::Integer(left), Numeric::Integer(right)) => {
            unsafe { result(api, integer_operation(left as i128, right as i128)) }
        }
        (Numeric::Integer(left), Numeric::Float(right)) => unsafe {
            (api.mkFloat)(float_operation(left as f64, right))
        },
        (Numeric::Float(left), Numeric::Integer(right)) => unsafe {
            (api.mkFloat)(float_operation(left, right as f64))
        },
        (Numeric::Float(left), Numeric::Float(right)) => unsafe {
            (api.mkFloat)(float_operation(left, right))
        },
    }
}

unsafe extern "C" fn integer_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { numeric_binary(self_object, args, count, |left, right| left + right, |left, right| left + right) }
}

unsafe extern "C" fn integer_sub(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { numeric_binary(self_object, args, count, |left, right| left - right, |left, right| left - right) }
}

unsafe extern "C" fn integer_mul(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { numeric_binary(self_object, args, count, |left, right| left * right, |left, right| left * right) }
}

unsafe extern "C" fn integer_div(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { numeric(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
    };
    match (left, right) {
        (Numeric::Integer(left), Numeric::Integer(right)) if right != 0 => unsafe {
            result(api, (left as i128) / (right as i128))
        },
        (Numeric::Integer(left), Numeric::Float(right)) if right != 0.0 => unsafe {
            (api.mkFloat)(left as f64 / right)
        },
        (Numeric::Float(left), Numeric::Integer(right)) if right != 0 => unsafe {
            (api.mkFloat)(left / right as f64)
        },
        (Numeric::Float(left), Numeric::Float(right)) if right != 0.0 => unsafe {
            (api.mkFloat)(left / right)
        },
        _ => api.nilObj,
    }
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
    predicate: impl FnOnce(f64, f64) -> bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { numeric(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
    };
    let matches = match (left, right) {
        (Numeric::Integer(left), Numeric::Integer(right)) => predicate(left as f64, right as f64),
        (Numeric::Integer(left), Numeric::Float(right)) => predicate(left as f64, right),
        (Numeric::Float(left), Numeric::Integer(right)) => predicate(left, right as f64),
        (Numeric::Float(left), Numeric::Float(right)) => predicate(left, right),
    };
    if matches { api.trueObj } else { api.falseObj }
}

unsafe extern "C" fn integer_eq(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
        return api.falseObj;
    };
    if left == right { api.trueObj } else { api.falseObj }
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
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
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
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    if right == 0.0 {
        return api.nilObj;
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
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return api.nilObj;
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    if predicate(left, right) { api.trueObj } else { api.falseObj }
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
    if left == right { api.trueObj } else { api.falseObj }
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

unsafe extern "C" fn float_negated(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
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
        return api.nilObj;
    }
    let value = value.trunc();
    // A SmallInt cannot represent 2^63 even though that boundary rounds from
    // i64::MAX when converted to f64. Reject it before Rust's saturating cast.
    if value < i64::MIN as f64 || value >= 9_223_372_036_854_775_808.0 {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe extern "C" fn float_as_integer(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
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
        return api.nilObj;
    }
    unsafe { (api.mkFloat)(value.sqrt()) }
}

unsafe extern "C" fn float_print_string(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
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

unsafe fn boolean_value(api: &IdApi, value: Oop) -> Option<bool> {
    if value == api.trueObj {
        Some(true)
    } else if value == api.falseObj {
        Some(false)
    } else {
        None
    }
}

unsafe extern "C" fn boolean_if_true_if_false(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(when_true) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let branch = if self_object == api.trueObj {
        when_true[0]
    } else {
        when_true[1]
    };
    unsafe { send0(api, branch, b"value\0") }
}

unsafe extern "C" fn boolean_if_true(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(branch) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj {
        unsafe { send0(api, branch, b"value\0") }
    } else {
        api.nilObj
    }
}

unsafe extern "C" fn boolean_if_false(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(branch) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.falseObj {
        unsafe { send0(api, branch, b"value\0") }
    } else {
        api.nilObj
    }
}

unsafe extern "C" fn boolean_and(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(argument) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj { argument } else { api.falseObj }
}

unsafe extern "C" fn boolean_or(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { boolean_value(api, self_object) }.is_none() {
        return api.nilObj;
    }
    let Some(argument) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if self_object == api.trueObj { api.trueObj } else { argument }
}

unsafe extern "C" fn boolean_and_block(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
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

unsafe extern "C" fn boolean_or_block(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
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
    unsafe { (api.mkString)(text.as_ptr(), text.len()) }
}

unsafe fn block_loop(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    continue_on_true: bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(body) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    loop {
        let condition = unsafe { send0(api, self_object, b"value\0") };
        let is_true = condition == api.trueObj;
        let is_false = condition == api.falseObj;
        if !is_true && !is_false {
            return api.nilObj;
        }
        if is_true != continue_on_true {
            return api.nilObj;
        }
        // The Error rung is not yet installed in this opt-in slice, so a body
        // result cannot be classified here. The loop still discards ordinary
        // body values and remains stack-flat across all iterations.
        let _ = unsafe { send0(api, body, b"value\0") };
    }
}

unsafe extern "C" fn block_while_true(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { block_loop(self_object, args, count, true) }
}

unsafe extern "C" fn block_while_false(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { block_loop(self_object, args, count, false) }
}

unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<&[u8]> {
    if value.is_null() || unsafe { (api.vtableOf)(value) } != api.string_vt {
        return None;
    }
    let mut length = 0usize;
    let bytes = unsafe { (api.stringBytes)(value, &mut length) };
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

fn write_system_output(bytes: &[u8], newline: bool) {
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(bytes);
    if newline {
        let _ = stdout.write_all(b"\n");
    }
}

unsafe fn system_output(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    newline: bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(bytes) = (unsafe { string_bytes(api, value) }) else {
        return api.nilObj;
    };
    write_system_output(bytes, newline);
    self_object
}

unsafe extern "C" fn system_print(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, false) }
}

unsafe extern "C" fn system_println(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, true) }
}

unsafe fn install_system_protocol(api: &IdApi) {
    let (system_vt, system_object) = if unsafe { (api.isExported)(c"System".as_ptr()) } != 0 {
        // SAFETY: an exported System family is process-owned and remains live
        // across reloads; extend its vtable instead of minting a duplicate.
        unsafe { ((api.import)(c"SystemVt".as_ptr()), (api.import)(c"System".as_ptr())) }
    } else {
        let system_vt = unsafe { (api.proto)(api.object_vt) };
        let system_object = unsafe { (api.alloc)(system_vt, 0) };
        unsafe { (api.setOwner)(system_vt, system_object) };
        unsafe { (api.exportObj)(c"SystemVt".as_ptr(), system_vt) };
        unsafe { (api.exportObj)(c"System".as_ptr(), system_object) };
        (system_vt, system_object)
    };
    let print = unsafe { intern(api, b"print:\0") };
    let println = unsafe { intern(api, b"println:\0") };
    // SAFETY: the System vtable and callback addresses are process-lifetime
    // values. The singleton is retained by the exported family.
    unsafe {
        (api.method)(system_vt, print, system_print);
        (api.method)(system_vt, println, system_println);
    }
    let _ = system_object;
}

unsafe fn install_block_protocol(api: &IdApi) {
    let (block_vt, block_proto) = if unsafe { (api.isExported)(c"Block".as_ptr()) } != 0 {
        let block_vt = unsafe { (api.import)(c"BlockVt".as_ptr()) };
        let block_proto = unsafe { (api.import)(c"Block".as_ptr()) };
        (block_vt, block_proto)
    } else {
        let block_vt = unsafe { (api.proto)(api.object_vt) };
        // SAFETY: a Block payload consists only of captured Oops, and this
        // first native loop rung does not inspect the payload itself.
        unsafe { (api.setLayout)(block_vt, ptr::null(), 0, birnam_abi::SLOT_OOP) };
        let block_proto = unsafe { (api.alloc)(block_vt, 0) };
        unsafe { (api.setOwner)(block_vt, block_proto) };
        unsafe { (api.exportObj)(c"Block".as_ptr(), block_proto) };
        unsafe { (api.exportObj)(c"BlockVt".as_ptr(), block_vt) };
        (block_vt, block_proto)
    };
    let _ = block_proto;
    let while_true = unsafe { intern(api, b"whileTrue:\0") };
    let while_false = unsafe { intern(api, b"whileFalse:\0") };
    // SAFETY: the Block vtable is process-owned and remains live with the image.
    unsafe {
        (api.method)(block_vt, while_true, block_while_true);
        (api.method)(block_vt, while_false, block_while_false);
    }
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
    let boolean_vt = unsafe { (api.vtableOf)(api.trueObj) };
    let boolean_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"ifTrue:ifFalse:\0", boolean_if_true_if_false),
        (b"ifTrue:\0", boolean_if_true),
        (b"ifFalse:\0", boolean_if_false),
        (b"&\0", boolean_and),
        (b"|\0", boolean_or),
        (b"and:\0", boolean_and_block),
        (b"or:\0", boolean_or_block),
        (b"not\0", boolean_not),
        (b"printString\0", boolean_print_string),
    ];
    for (selector, method) in boolean_methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: both singleton booleans share this vtable for the process lifetime.
        unsafe { (api.method)(boolean_vt, symbol, *method) };
    }
    unsafe { install_block_protocol(api) };
    unsafe { install_system_protocol(api) };
}

/// Identifies this opt-in N7.1 Rust prelude numeric/Boolean slice.
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
