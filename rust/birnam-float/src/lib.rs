//! Existing Float methods, loaded independently of Prelude.
//! Owns the canonical representation and explicit native Float dependency (81R).
#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use birnam_abi::error_value::{is_error, numeric_error};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;

static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
// 0 unprepared, 1 preparing, 2 prepared, 3 activating, 4 active.
static PHASE: AtomicU8 = AtomicU8::new(0);
fn abort(message: &str) -> ! {
    eprintln!("birnam Float: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let instance = INSTANCE.load(Ordering::Acquire);
    if instance.is_null() {
        abort("Float was not prepared");
    }
    unsafe { &*(*instance).api() }
}
unsafe extern "C" fn float_roots(mark: birnam_abi::MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() {
        unsafe { (*instance).trace(mark) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_float_provider_v1() -> *mut birnam_abi::float::FloatApiV1 {
    if PHASE.load(Ordering::Acquire) < 2 {
        return ptr::null_mut();
    }
    unsafe { (*INSTANCE.load(Ordering::Acquire)).operations() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_float_prepare_v4(
    api: *mut IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    exports: *const birnam_abi::namespace_export::ApiV1,
) -> *mut birnam_abi::float::FloatApiV1 {
    if api.is_null() {
        abort("preparation received a null IdApi");
    }
    if PHASE.load(Ordering::Acquire) != 0 {
        let instance = INSTANCE.load(Ordering::Acquire);
        if instance.is_null() || unsafe { (*instance).api() } != api {
            abort("another IdApi is already mounted");
        }
        if PHASE.load(Ordering::Acquire) == 1 {
            abort("recursive Float preparation");
        }
        return unsafe { birnam_float_provider_v1() };
    }
    PHASE.store(1, Ordering::Release);
    let instance = Box::into_raw(unsafe { storage::Instance::empty(api, presence, imports, exports) });
    INSTANCE.store(instance, Ordering::Release);
    unsafe {
        ((*api).gcRegisterRootProvider)(float_roots);
        (*instance).prepare();
    }
    let operations = unsafe { (*instance).operations() };
    if unsafe { birnam_abi::float::attach(operations) } != 1 {
        abort("invalid Float table");
    }
    PHASE.store(2, Ordering::Release);
    operations
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

unsafe fn float_self(_api: &IdApi, value: Oop) -> Option<f64> {
    if value.is_null() || unsafe { (birnam_abi::float::is_float)(value) } == 0 {
        return None;
    }
    Some(unsafe { (birnam_abi::float::value)(value) })
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
    if unsafe { is_error(api, execution_presence(), execution_imports(), right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, execution_presence(), execution_imports(), b"typeMismatch\0", b"Float arithmetic needs a number")
        };
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    unsafe { (birnam_abi::float::make)(operation(left, right)) }
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
    if unsafe { is_error(api, execution_presence(), execution_imports(), right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe {
            numeric_error(api, execution_presence(), execution_imports(), b"typeMismatch\0", b"Float arithmetic needs a number")
        };
    };
    let right = match right {
        Numeric::Integer(value) => value as f64,
        Numeric::Float(value) => value,
    };
    if right == 0.0 {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"zeroDivide\0", b"division by zero") };
    }
    unsafe { (birnam_abi::float::make)(left / right) }
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
    if unsafe { is_error(api, execution_presence(), execution_imports(), right_object) } {
        return right_object;
    }
    let Some(right) = (unsafe { numeric(api, right_object) }) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"typeMismatch\0", b"comparison needs a number") };
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
    unsafe { (birnam_abi::float::make)(-value) }
}

unsafe fn float_to_integer(api: &IdApi, value: f64) -> Oop {
    if !value.is_finite() {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"overflow\0", b"float out of Integer range") };
    }
    let value = value.trunc();
    // A SmallInt cannot represent 2^63 even though that boundary rounds from
    // i64::MAX when converted to f64. Reject it before Rust's saturating cast.
    if value < i64::MIN as f64 || value >= 9_223_372_036_854_775_808.0 {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"overflow\0", b"float out of Integer range") };
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
    unsafe { (birnam_abi::float::make)(value.abs()) }
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
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"domain\0", b"square root of a negative number") };
    }
    unsafe { (birnam_abi::float::make)(value.sqrt()) }
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
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

/// Install the existing Float protocol on this module's canonical behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV1) -> *mut c_void {
    let operations = unsafe { birnam_float_prepare_v4(api_pointer, presence, imports, exports) };
    let previous = PHASE.swap(3, Ordering::AcqRel);
    if previous != 2 && previous != 4 {
        abort("recursive Float activation");
    }
    // Verify the peer before the first replaceable publication callback.
    let _ = unsafe { birnam_abi::integer::operations() };
    let api = unsafe { &*api_pointer };
    if previous == 2 {
        unsafe { (*INSTANCE.load(Ordering::Acquire)).publish() };
    }
    // Preserve method-only initialization: repeated calls reinstall methods
    // without recreating canonical objects or overwriting namespace bindings.
    let behavior = unsafe { (*operations).behavior };
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
        // SAFETY: the behavior and each method are live for the process lifetime.
        unsafe { (api.method)(behavior, symbol, *method) };
    }
    PHASE.store(4, Ordering::Release);
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::integer_dependency!();

birnam_abi::string_dependency!();

birnam_abi::error_value_dependency!();

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { unsafe { (*INSTANCE.load(Ordering::Acquire)).presence() } }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { unsafe { (*INSTANCE.load(Ordering::Acquire)).imports() } }
