//! Existing StringBuilder representation and protocol, extracted from Collection.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::alloc::{Layout, alloc, dealloc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};

const STRING_BUILDER_NAME: &[u8] = b"StringBuilder\0";
const STRING_BUILDER_VT_NAME: &[u8] = b"StringBuilderVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
static STRING_BUILDER_LAYOUT: [u8; 3] = [birnam_abi::SLOT_RAW; 3];
#[repr(C)]
struct StringBuilderPayload {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}
const _: () = {
    assert!(size_of::<StringBuilderPayload>() == 24);
    assert!(align_of::<StringBuilderPayload>() == 8);
};
#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    string_builder_vt: usize,
    string_builder_proto: usize,
    print_string: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
// Owner-thread construction publishes each root before the next Id callback.
// These slots also retain the family after ordinary export rebinding.
static ROOTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    for root in &ROOTS {
        let object = root.load(Ordering::Acquire) as Oop;
        if !object.is_null() {
            unsafe { mark(object) };
        }
    }
}
fn abort(message: &str) -> ! {
    eprintln!("birnam StringBuilder: {message}");
    std::process::abort();
}
fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}
unsafe fn api_ref() -> &'static IdApi {
    // The host pins this module and the selected Id table for process life.
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}
unsafe fn import(imports: *const birnam_abi::namespace_import::ApiV1, name: &[u8]) -> Oop {
    unsafe { birnam_abi::namespace_import::import(imports, name.as_ptr().cast::<c_char>()) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn string_builder_payload(object: Oop) -> *mut StringBuilderPayload {
    if object.is_null() {
        abort("StringBuilder method received a null receiver");
    }
    object.cast::<StringBuilderPayload>()
}

unsafe fn string_builder_alloc(capacity: usize) -> *mut u8 {
    if capacity == 0 {
        return ptr::null_mut();
    }
    let layout =
        Layout::array::<u8>(capacity).unwrap_or_else(|_| abort("StringBuilder capacity overflow"));
    let buffer = unsafe { alloc(layout) };
    if buffer.is_null() {
        abort("StringBuilder buffer allocation failed");
    }
    buffer
}

unsafe fn string_builder_free(buffer: *mut u8, capacity: usize) {
    if buffer.is_null() || capacity == 0 {
        return;
    }
    let layout =
        Layout::array::<u8>(capacity).unwrap_or_else(|_| abort("StringBuilder capacity overflow"));
    unsafe { dealloc(buffer, layout) };
}

unsafe extern "C" fn string_builder_finalize(object: Oop) {
    let payload = unsafe { string_builder_payload(object) };
    let buffer = unsafe { (*payload).ptr };
    let capacity = unsafe { (*payload).cap };
    unsafe { string_builder_free(buffer, capacity) };
    unsafe {
        (*payload).ptr = ptr::null_mut();
        (*payload).len = 0;
        (*payload).cap = 0;
    }
}

unsafe fn string_builder_ensure(payload: *mut StringBuilderPayload, extra: usize) {
    let length = unsafe { (*payload).len };
    let capacity = unsafe { (*payload).cap };
    let needed = length
        .checked_add(extra)
        .unwrap_or_else(|| abort("StringBuilder length overflow"));
    if needed <= capacity {
        return;
    }
    let mut next_capacity = if capacity == 0 { 64 } else { capacity };
    while next_capacity < needed {
        next_capacity = next_capacity
            .checked_mul(2)
            .unwrap_or_else(|| abort("StringBuilder capacity overflow"));
    }
    let next_buffer = unsafe { string_builder_alloc(next_capacity) };
    let old_buffer = unsafe { (*payload).ptr };
    if length != 0 {
        unsafe { ptr::copy_nonoverlapping(old_buffer, next_buffer, length) };
    }
    unsafe { string_builder_free(old_buffer, capacity) };
    unsafe {
        (*payload).ptr = next_buffer;
        (*payload).cap = next_capacity;
    }
}

unsafe fn empty_string_builder(api: &IdApi, runtime: &Runtime) -> Oop {
    let builder = unsafe {
        (api.alloc)(
            runtime.string_builder_vt as Oop,
            size_of::<StringBuilderPayload>(),
        )
    };
    if builder.is_null() {
        abort("StringBuilder allocation failed");
    }
    unsafe {
        ptr::write(
            string_builder_payload(builder),
            StringBuilderPayload {
                ptr: ptr::null_mut(),
                len: 0,
                cap: 0,
            },
        );
        (api.setFinalizer)(builder, string_builder_finalize);
    }
    builder
}

unsafe extern "C" fn string_builder_new(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { empty_string_builder(api, runtime()) }
}

unsafe extern "C" fn string_builder_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(mut value) = (unsafe { argument(args, count) }) else {
        return self_object;
    };
    if unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
        value = unsafe { (api.send)(value, runtime().print_string as Oop, ptr::null(), 0) };
    }
    if value.is_null()
        || unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() }
    {
        return self_object;
    }
    let mut length = 0;
    let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    if length == 0 {
        return self_object;
    }
    if bytes.is_null() {
        return self_object;
    }
    let payload = unsafe { string_builder_payload(self_object) };
    unsafe { string_builder_ensure(payload, length) };
    let destination = unsafe { (*payload).ptr.add((*payload).len) };
    unsafe { ptr::copy_nonoverlapping(bytes, destination, length) };
    unsafe { (*payload).len += length };
    self_object
}

unsafe extern "C" fn string_builder_add_byte(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return self_object;
    };
    if unsafe { (birnam_abi::integer::is_integer)(value) } == 0 {
        return self_object;
    }
    let payload = unsafe { string_builder_payload(self_object) };
    unsafe { string_builder_ensure(payload, 1) };
    let byte = unsafe { (birnam_abi::integer::value)(value) } as u8;
    unsafe {
        (*payload).ptr.add((*payload).len).write(byte);
        (*payload).len += 1;
    }
    self_object
}

unsafe extern "C" fn string_builder_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)((*string_builder_payload(self_object)).len as i64) }
}

unsafe extern "C" fn string_builder_as_string(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let payload = unsafe { string_builder_payload(self_object) };
    let length = unsafe { (*payload).len };
    if length == 0 {
        return unsafe { (birnam_abi::string::make)(b"".as_ptr(), 0) };
    }
    let bytes = unsafe { (*payload).ptr };
    if bytes.is_null() {
        return api.nilObj;
    }
    unsafe { (birnam_abi::string::make)(bytes, length) }
}

/// Initialize this native family using its existing adoption/publication rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV1) -> *mut c_void {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    ROOTS_REGISTERED.call_once(|| unsafe { (api.gcRegisterRootProvider)(mark_roots) });
    let runtime = *RUNTIME.get_or_init(|| {
        let adopting = unsafe { birnam_abi::namespace_presence::contains(presence, STRING_BUILDER_NAME.as_ptr().cast()) } != 0;
        let behavior = if adopting {
            unsafe { import(imports, STRING_BUILDER_VT_NAME) }
        } else {
            unsafe { (api.proto)(api.object_vt) }
        };
        if behavior.is_null() {
            abort("StringBuilder exports were incomplete");
        }
        ROOTS[0].store(behavior as usize, Ordering::Release);
        let prototype = if adopting {
            unsafe { import(imports, STRING_BUILDER_NAME) }
        } else {
            ptr::null_mut()
        };
        let mut runtime = Runtime {
            api: api_pointer as usize,
            string_builder_vt: behavior as usize,
            string_builder_proto: prototype as usize,
            print_string: 0,
        };
        let prototype = if prototype.is_null() {
            unsafe { empty_string_builder(api, &runtime) }
        } else {
            prototype
        };
        ROOTS[1].store(prototype as usize, Ordering::Release);
        runtime.string_builder_proto = prototype as usize;
        let selector = unsafe { intern(api, b"printString\0") };
        ROOTS[2].store(selector as usize, Ordering::Release);
        runtime.print_string = selector as usize;
        runtime
    });
    unsafe {
        (api.setLayout)(
            runtime.string_builder_vt as Oop,
            STRING_BUILDER_LAYOUT.as_ptr(),
            STRING_BUILDER_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"new\0"),
            string_builder_new,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"clone\0"),
            string_builder_new,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"add:\0"),
            string_builder_add,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"addByte:\0"),
            string_builder_add_byte,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"size\0"),
            string_builder_size,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"asString\0"),
            string_builder_as_string,
        );
        (api.method)(
            runtime.string_builder_vt as Oop,
            intern(api, b"printString\0"),
            string_builder_as_string,
        );
    }
    // Preserve Collection's existing cross-family publication guard exactly.
    if unsafe { birnam_abi::namespace_presence::contains(presence, VECTOR_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            birnam_abi::set_vtable_owner(
                api,
                runtime.string_builder_vt as Oop,
                runtime.string_builder_proto as Oop,
            );
            birnam_abi::namespace_export::export(exports,
                STRING_BUILDER_VT_NAME.as_ptr().cast(),
                runtime.string_builder_vt as Oop,
            );
            birnam_abi::namespace_export::export(exports,
                STRING_BUILDER_NAME.as_ptr().cast(),
                runtime.string_builder_proto as Oop,
            );
        }
    }
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
birnam_abi::integer_dependency!();
birnam_abi::string_dependency!();
