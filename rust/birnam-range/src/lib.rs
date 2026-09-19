//! Range's existing non-Vector methods and ordered protocol installation.
//! Collection retains construction/storage and the two Vector-backed methods.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::range_methods::ApiV1;
use birnam_abi::{IdApi, MarkFn, Method, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};

// Existing RAW representation, still constructed by Collection (81Z).
#[repr(C)]
struct RangePayload {
    start: i64,
    end: i64,
}
const _: () = {
    assert!(size_of::<RangePayload>() == 16);
    assert!(align_of::<RangePayload>() == 8);
    assert!(core::mem::offset_of!(RangePayload, end) == 8);
};
struct Runtime {
    api: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
static READY: AtomicBool = AtomicBool::new(false);
fn abort(message: &str) -> ! {
    eprintln!("birnam Range: {message}");
    std::process::abort();
}
fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("Range protocol was not installed"))
}
unsafe fn api_ref() -> &'static IdApi {
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for object in [runtime.value1, runtime.value2, runtime.print_string] {
            unsafe { mark(object as Oop) };
        }
    }
}

unsafe fn range_payload(object: Oop) -> *mut RangePayload {
    if object.is_null() {
        abort("Range method received a null receiver");
    }
    object.cast::<RangePayload>()
}

unsafe fn range_bounds(object: Oop) -> (i64, i64) {
    let payload = unsafe { range_payload(object) };
    unsafe { ((*payload).start, (*payload).end) }
}

unsafe extern "C" fn range_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let (start, end) = unsafe { range_bounds(self_object) };
    if end < start {
        return unsafe { (birnam_abi::integer::make)(0) };
    }
    let size = i128::from(end) - i128::from(start) + 1;
    if size > i128::from(i64::MAX) {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(size as i64) }
}

unsafe extern "C" fn range_includes(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (birnam_abi::integer::is_integer)(value) } == 0 {
        return api.falseObj;
    }
    let value = unsafe { (birnam_abi::integer::value)(value) };
    let (start, end) = unsafe { range_bounds(self_object) };
    if start <= end && value >= start && value <= end {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn range_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(index) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (birnam_abi::integer::is_integer)(index) } == 0 {
        return api.nilObj;
    }
    let index = unsafe { (birnam_abi::integer::value)(index) };
    let (start, end) = unsafe { range_bounds(self_object) };
    let value = i128::from(start) + i128::from(index);
    if index < 0 || start > end || value > i128::from(end) || value > i128::from(i64::MAX) {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

unsafe extern "C" fn range_do(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let selector = runtime().value1 as Oop;
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut value = i128::from(start);
    let end = i128::from(end);
    while value <= end {
        let item = unsafe { (birnam_abi::integer::make)(value as i64) };
        let arguments = [item];
        unsafe { (api.send)(block, selector, arguments.as_ptr(), 1) };
        value += 1;
    }
    self_object
}

unsafe extern "C" fn range_inject(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let selector = runtime().value2 as Oop;
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut value = i128::from(start);
    let end = i128::from(end);
    let mut accumulator = arguments[0];
    while value <= end {
        let item = unsafe { (birnam_abi::integer::make)(value as i64) };
        let call_arguments = [accumulator, item];
        accumulator = unsafe { (api.send)(arguments[1], selector, call_arguments.as_ptr(), 2) };
        if accumulator.is_null() {
            return api.nilObj;
        }
        value += 1;
    }
    accumulator
}

unsafe extern "C" fn range_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let selector = runtime().print_string as Oop;
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut output = Vec::from(&b"#["[..]);
    let mut value = i128::from(start);
    let end = i128::from(end);
    while value <= end {
        if value != i128::from(start) {
            output.push(b' ');
        }
        let item = unsafe { (birnam_abi::integer::make)(value as i64) };
        let printed = unsafe { (api.send)(item, selector, ptr::null(), 0) };
        if printed.is_null() {
            return api.nilObj;
        }
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(printed, &mut length) };
        if bytes.is_null() {
            return api.nilObj;
        }
        output.extend_from_slice(unsafe { core::slice::from_raw_parts(bytes, length) });
        value += 1;
    }
    output.push(b']');
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
}

// Called at Collection's original Range registration point, before publication.
unsafe extern "C" fn install(
    api_pointer: *mut IdApi,
    behavior: Oop,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
    range_reversed: Method,
    range_comma: Method,
) {
    if !READY.load(Ordering::Acquire)
        || api_pointer.is_null()
        || behavior.is_null()
        || value1.is_null()
        || value2.is_null()
        || print_string.is_null()
    {
        abort("invalid Range construction dependency");
    }
    let api = unsafe { &*api_pointer };
    RUNTIME.get_or_init(|| Runtime {
        api: api_pointer as usize,
        value1: value1 as usize,
        value2: value2 as usize,
        print_string: print_string as usize,
    });
    ROOTS_REGISTERED.call_once(|| unsafe { (api.gcRegisterRootProvider)(mark_roots) });
    unsafe {
        (api.method)(behavior, intern(api, b"size\0"), range_size);
        (api.method)(behavior, intern(api, b"includes:\0"), range_includes);
        (api.method)(behavior, intern(api, b"do:\0"), range_do);
        (api.method)(behavior, intern(api, b"inject:into:\0"), range_inject);
        (api.method)(behavior, intern(api, b"at:\0"), range_at);
        (api.method)(behavior, intern(api, b"reversed\0"), range_reversed);
        (api.method)(behavior, intern(api, b"printString\0"), range_print_string);
        (api.method)(behavior, intern(api, b",\0"), range_comma);
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void) -> *mut c_void {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_range_methods_provider_v1() -> *const ApiV1 {
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
birnam_abi::integer_dependency!();
birnam_abi::string_dependency!();
