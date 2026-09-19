//! Production Integer representation, primitive operations and methods above Id.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};
pub mod storage;
static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());

unsafe extern "C" fn integer_roots(mark: birnam_abi::MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() {
        unsafe { (*instance).trace(mark) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_integer_provider_v1() -> *mut birnam_abi::integer::IntegerApiV1 {
    let instance = INSTANCE.load(Ordering::Acquire);
    if instance.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*instance).operations() }
    }
}

/// Optional native implementation declaration, independent of selector names.
/// See abi/native-numeric-semantics-v1.md. This pure query never enters Id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_numeric_semantics_v2(
    method: *const c_void,
    integer_vtable: Oop,
    functions: *const usize,
) -> u32 {
    let Some(runtime) = RUNTIME.get() else {
        return 0;
    };
    if functions.is_null() {
        return 0;
    }
    let api = unsafe { &*(runtime.api as *const IdApi) };
    // A module initialized against another floor cannot attest this floor's
    // canonical operands or allocation/error behavior.
    let ops = unsafe { birnam_abi::integer::operations() };
    let instance = INSTANCE.load(Ordering::Acquire);
    let identities = unsafe { (*instance).identities() };
    if identities[0] != integer_vtable
        || ops.behavior != identities[0]
        || ops.prototype != identities[1]
        || unsafe { core::slice::from_raw_parts(functions, 10) }
            != [
                ops.is_exact as usize,
                ops.read as usize,
                api.isExported as usize,
                api.import as usize,
                api.vtableOf as usize,
                ops.make as usize,
                ops.context as usize,
                ops.storage as usize,
                ops.prototype as usize,
                api.alloc as usize,
            ]
    {
        return 0;
    }
    declared_numeric_operator(method)
}

fn declared_numeric_operator(method: *const c_void) -> u32 {
    for (implementation, code) in [
        (integer_add as birnam_abi::Method, 1),
        (integer_sub as birnam_abi::Method, 2),
        (integer_mul as birnam_abi::Method, 3),
        (integer_div as birnam_abi::Method, 4),
        (integer_mod as birnam_abi::Method, 5),
    ] {
        if method == implementation as *const c_void {
            return code;
        }
    }
    0
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Integer: {message}");
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

unsafe fn send_args(api: &IdApi, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    let arguments = if args.is_empty() {
        ptr::null()
    } else {
        args.as_ptr()
    };
    // SAFETY: callers keep the synchronous stack-backed argument slice alive
    // until this send returns; IdApi does not retain the pointer.
    unsafe { (api.send)(receiver, selector, arguments, args.len()) }
}

unsafe fn integer(_api: &IdApi, value: Oop) -> Option<i64> {
    if value.is_null() || unsafe { (birnam_abi::integer::is_integer)(value) } == 0 {
        return None;
    }
    // SAFETY: isSmallInt established the floor's integer representation.
    Some(unsafe { (birnam_abi::integer::value)(value) })
}

unsafe fn result(api: &IdApi, value: i128) -> Oop {
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return unsafe { numeric_error(api, b"overflow\0", b"integer overflow") };
    }
    // SAFETY: the range check makes the conversion exact.
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

#[derive(Clone, Copy)]
enum Numeric {
    Integer(i64),
    Float(f64),
}

unsafe fn numeric(_api: &IdApi, value: Oop) -> Option<Numeric> {
    if value.is_null() {
        return None;
    }
    if unsafe { (birnam_abi::integer::is_integer)(value) } != 0 {
        return Some(Numeric::Integer(unsafe {
            (birnam_abi::integer::value)(value)
        }));
    }
    if unsafe { (birnam_abi::float::is_float)(value) } != 0 {
        return Some(Numeric::Float(unsafe { (birnam_abi::float::value)(value) }));
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
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, b"typeMismatch\0", b"Integer arithmetic needs a number")
        };
    };
    match (left, right) {
        (Numeric::Integer(left), Numeric::Integer(right)) => unsafe {
            result(api, integer_operation(left as i128, right as i128))
        },
        (Numeric::Integer(left), Numeric::Float(right)) => unsafe {
            (birnam_abi::float::make)(float_operation(left as f64, right))
        },
        (Numeric::Float(left), Numeric::Integer(right)) => unsafe {
            (birnam_abi::float::make)(float_operation(left, right as f64))
        },
        (Numeric::Float(left), Numeric::Float(right)) => unsafe {
            (birnam_abi::float::make)(float_operation(left, right))
        },
    }
}

unsafe extern "C" fn integer_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe {
        numeric_binary(
            self_object,
            args,
            count,
            |left, right| left + right,
            |left, right| left + right,
        )
    }
}

unsafe extern "C" fn integer_sub(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe {
        numeric_binary(
            self_object,
            args,
            count,
            |left, right| left - right,
            |left, right| left - right,
        )
    }
}

unsafe extern "C" fn integer_mul(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe {
        numeric_binary(
            self_object,
            args,
            count,
            |left, right| left * right,
            |left, right| left * right,
        )
    }
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
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, b"typeMismatch\0", b"Integer arithmetic needs a number")
        };
    };
    let zero_divide = match right {
        Numeric::Integer(value) => value == 0,
        Numeric::Float(value) => value == 0.0,
    };
    if zero_divide {
        return unsafe { numeric_error(api, b"zeroDivide\0", b"division by zero") };
    }
    match (left, right) {
        (Numeric::Integer(left), Numeric::Integer(right)) => unsafe {
            result(api, (left as i128) / (right as i128))
        },
        (Numeric::Integer(left), Numeric::Float(right)) => unsafe {
            (birnam_abi::float::make)(left as f64 / right)
        },
        (Numeric::Float(left), Numeric::Integer(right)) => unsafe {
            (birnam_abi::float::make)(left / right as f64)
        },
        (Numeric::Float(left), Numeric::Float(right)) => unsafe { (birnam_abi::float::make)(left / right) },
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
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { integer(api, right_object) }) else {
        return unsafe { numeric_error(api, b"typeMismatch\0", b"mod: needs an Integer") };
    };
    if right == 0 {
        return unsafe { numeric_error(api, b"zeroDivide\0", b"division by zero") };
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
    if unsafe { is_error(api, right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe { numeric_error(api, b"typeMismatch\0", b"comparison needs a number") };
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
    if left == right {
        api.trueObj
    } else {
        api.falseObj
    }
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
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

unsafe extern "C" fn integer_as_float(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    // SAFETY: the mounted Float module owns the boxed constructor.
    unsafe { (birnam_abi::float::make)(value as f64) }
}

unsafe extern "C" fn integer_to_do(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(values) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let Some(start) = (unsafe { integer(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(end) = (unsafe { integer(api, values[0]) }) else {
        return api.nilObj;
    };
    let block = values[1];
    let mut value = start;
    while value <= end {
        let argument = unsafe { (birnam_abi::integer::make)(value) };
        let result = unsafe { send_args(api, block, b"value:\0", &[argument]) };
        if unsafe { is_error(api, result) } {
            return result;
        }
        if value == i64::MAX {
            break;
        }
        value += 1;
    }
    self_object
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
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    unsafe { make_error(api, kind, message) }
}

/// The host retains the floor and this library for the process lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    if let Some(runtime) = RUNTIME.get() {
        assert_eq!(
            runtime.api, api_pointer as usize,
            "Integer mounted on another API"
        );
        return ptr::null_mut();
    }
    RUNTIME
        .set(Runtime {
            api: api_pointer as usize,
        })
        .ok()
        .expect("Integer initialization serialized");
    let instance = Box::into_raw(unsafe { storage::Instance::empty(api_pointer) });
    INSTANCE.store(instance, Ordering::Release);
    unsafe { (api.gcRegisterRootProvider)(integer_roots) };
    unsafe { (*instance).populate() };
    assert_eq!(
        unsafe { birnam_abi::integer::attach((*instance).operations()) },
        1
    );
    let integer_vt = unsafe { birnam_abi::integer::behavior() };
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
        (b"to:do:\0", integer_to_do),
    ];
    for (selector, method) in methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: integer_vt and each method are live for the process lifetime.
        unsafe { (api.method)(integer_vt, symbol, *method) };
    }
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_integer_rust_identity() -> *const c_char {
    c"rust-integer-v1".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_declarations_name_exact_implementations_and_reject_another_floor() {
        for (method, code) in [
            (integer_add as birnam_abi::Method, 1),
            (integer_sub, 2),
            (integer_mul, 3),
            (integer_div, 4),
            (integer_mod, 5),
        ] {
            assert_eq!(declared_numeric_operator(method as *const c_void), code);
            assert_eq!(
                unsafe {
                    birnam_native_numeric_semantics_v2(
                        method as *const c_void,
                        ptr::null_mut(),
                        ptr::null(),
                    )
                },
                0
            );
        }
        assert_eq!(declared_numeric_operator(integer_eq as *const c_void), 0);
        assert_eq!(declared_numeric_operator(ptr::null()), 0);
    }
}

birnam_abi::float_dependency!();

birnam_abi::string_dependency!();
