//! Range representation, construction and all existing Range methods.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::range::ApiV4;
use birnam_abi::{IdApi, MarkFn, Method, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};

// Owned RAW representation; Image continues to use its generic RAW codec.
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
    range_vt: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
    vector_comma: Method,
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
const RANGE_NAME: &[u8] = b"Range\0";
const RANGE_VT_NAME: &[u8] = b"RangeVt\0";
static RANGE_LAYOUT: [u8; 2] = [birnam_abi::SLOT_RAW; 2];
struct Family {
    api: usize,
    range_vt: usize,
}
static FAMILY: OnceLock<Family> = OnceLock::new();
static IDENTITY_ROOTS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
fn family() -> &'static Family {
    FAMILY
        .get()
        .unwrap_or_else(|| abort("Range family was not prepared"))
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    for root in &IDENTITY_ROOTS {
        let object = root.load(Ordering::Acquire) as Oop;
        if !object.is_null() {
            unsafe { mark(object) };
        }
    }
    if let Some(runtime) = RUNTIME.get() {
        for object in [runtime.value1, runtime.value2, runtime.print_string] {
            unsafe { mark(object as Oop) };
        }
    }
}
unsafe extern "C" fn prepare(api_pointer: *mut IdApi, imports: *const birnam_abi::namespace_import::ApiV1, parent: Oop, adopting: i32) -> Oop {
    if !READY.load(Ordering::Acquire) || api_pointer.is_null() {
        abort("invalid Range preparation");
    }
    let api = unsafe { &*api_pointer };
    ROOTS_REGISTERED.call_once(|| unsafe { (api.gcRegisterRootProvider)(mark_roots) });
    let family = FAMILY.get_or_init(|| {
        let behavior = if adopting != 0 {
            unsafe { birnam_abi::namespace_import::import(imports, RANGE_VT_NAME.as_ptr().cast()) }
        } else {
            unsafe { (api.proto)(parent) }
        };
        IDENTITY_ROOTS[0].store(behavior as usize, Ordering::Release);
        let prototype = if adopting != 0 {
            unsafe { birnam_abi::namespace_import::import(imports, RANGE_NAME.as_ptr().cast()) }
        } else {
            ptr::null_mut()
        };
        IDENTITY_ROOTS[1].store(prototype as usize, Ordering::Release);
        Family {
            api: api_pointer as usize,
            range_vt: behavior as usize,
        }
    });
    family.range_vt as Oop
}
unsafe extern "C" fn finish_prototype(api_pointer: *mut IdApi) {
    if IDENTITY_ROOTS[1].load(Ordering::Acquire) == 0 {
        let prototype = unsafe { empty_range(&*api_pointer, family().range_vt as Oop, 0, 0) };
        IDENTITY_ROOTS[1].store(prototype as usize, Ordering::Release);
    }
}
unsafe extern "C" fn set_layout(api_pointer: *mut IdApi) {
    let api = unsafe { &*api_pointer };
    unsafe {
        (api.setLayout)(
            family().range_vt as Oop,
            RANGE_LAYOUT.as_ptr(),
            RANGE_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        )
    };
}
unsafe extern "C" fn publish(api_pointer: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1) {
    let api = unsafe { &*api_pointer };
    let behavior = family().range_vt as Oop;
    let prototype = IDENTITY_ROOTS[1].load(Ordering::Acquire) as Oop;
    unsafe {
        birnam_abi::set_vtable_owner(api, behavior, prototype);
        birnam_abi::namespace_export::export(exports, RANGE_VT_NAME.as_ptr().cast(), behavior);
        birnam_abi::namespace_export::export(exports, RANGE_NAME.as_ptr().cast(), prototype);
    }
}
unsafe fn empty_range(api: &IdApi, behavior: Oop, start: i64, end: i64) -> Oop {
    let range = unsafe { (api.alloc)(behavior, size_of::<RangePayload>()) };
    if range.is_null() {
        abort("Range allocation failed");
    }
    unsafe {
        *range.cast::<RangePayload>() = RangePayload { start, end };
    }
    range
}

unsafe extern "C" fn integer_to(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { (birnam_abi::integer::is_integer)(self_object) } == 0 {
        return api.nilObj;
    }
    let Some(end) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (birnam_abi::integer::is_integer)(end) } == 0 {
        return api.nilObj;
    }
    unsafe {
        empty_range(
            api,
            runtime().range_vt as Oop,
            (birnam_abi::integer::value)(self_object),
            (birnam_abi::integer::value)(end),
        )
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

unsafe extern "C" fn range_reversed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut result = unsafe { birnam_abi::vector::empty() };
    let mut value = i128::from(end);
    let start = i128::from(start);
    while value >= start {
        result = unsafe { birnam_abi::vector::push(result, (birnam_abi::integer::make)(value as i64)) };
        value -= 1;
    }
    result
}

unsafe extern "C" fn range_comma(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(other) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(other) } != unsafe { birnam_abi::vector::behavior() } {
        return api.nilObj;
    }
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut result = unsafe { birnam_abi::vector::empty() };
    let mut value = i128::from(start);
    let end = i128::from(end);
    while value <= end {
        result = unsafe { birnam_abi::vector::push(result, (birnam_abi::integer::make)(value as i64)) };
        value += 1;
    }
    unsafe { (runtime().vector_comma)(result, args, count) }
}

// Called at Collection's original Range registration point, before publication.
unsafe extern "C" fn install(
    api_pointer: *mut IdApi,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
    vector_comma: Method,
) {
    if !READY.load(Ordering::Acquire)
        || api_pointer.is_null()
        || value1.is_null()
        || value2.is_null()
        || print_string.is_null()
    {
        abort("invalid Range construction dependency");
    }
    let api = unsafe { &*api_pointer };
    RUNTIME.get_or_init(|| Runtime {
        api: family().api,
        range_vt: family().range_vt,
        value1: value1 as usize,
        value2: value2 as usize,
        print_string: print_string as usize,
        vector_comma,
    });
    let behavior = family().range_vt as Oop;
    unsafe {
        (api.method)(behavior, intern(api, b"size\0"), range_size);
        (api.method)(behavior, intern(api, b"includes:\0"), range_includes);
        (api.method)(behavior, intern(api, b"do:\0"), range_do);
        (api.method)(behavior, intern(api, b"inject:into:\0"), range_inject);
        (api.method)(behavior, intern(api, b"at:\0"), range_at);
        (api.method)(behavior, intern(api, b"reversed\0"), range_reversed);
        (api.method)(behavior, intern(api, b"printString\0"), range_print_string);
        (api.method)(behavior, intern(api, b",\0"), range_comma);
        (api.method)(
            birnam_abi::integer::behavior(),
            intern(api, b"to:\0"),
            integer_to,
        );
    }
}
static API: ApiV4 = ApiV4 {
    version: 4,
    byte_size: size_of::<ApiV4>(),
    prepare: Some(prepare),
    finish_prototype: Some(finish_prototype),
    set_layout: Some(set_layout),
    install: Some(install),
    publish: Some(publish),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV1) -> *mut c_void {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_range_provider_v4() -> *const ApiV4 {
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

birnam_abi::vector_dependency!();
