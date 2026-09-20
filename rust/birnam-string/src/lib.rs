//! Existing String methods, independently loaded from Collection (81W).
//! Owns canonical representation and external byte buffers (81X).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Once, OnceLock};
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;
static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
static PHASE: AtomicU8 = AtomicU8::new(0);

struct Runtime {
    api: usize,
    print_string: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
fn abort(message: &str) -> ! {
    eprintln!("birnam String: {message}");
    std::process::abort();
}
fn runtime() -> &'static Runtime {
    RUNTIME.get().unwrap_or_else(|| abort("module_init was not called"))
}
unsafe fn api_ref() -> &'static IdApi {
    // The host pins this module and the selected floor table for process life.
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        unsafe { mark(runtime.print_string as Oop) };
    }
}

unsafe extern "C" fn storage_roots(mark: MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() { unsafe { (*instance).trace(mark) }; }
}
unsafe extern "C" fn cleanup(object: Oop) {
    let ops = unsafe { birnam_string_provider_v1().read() };
    assert_eq!(unsafe { ((*ops.storage).release)(ops.context, object) }, 1, "String finalizer lost byte ownership");
}
unsafe extern "C" fn cleanup_owner(finalizer: birnam_abi::Finalizer) -> *mut c_void {
    if finalizer as usize == cleanup as *const () as usize {
        INSTANCE.load(Ordering::Acquire).cast()
    } else { ptr::null_mut() }
}
unsafe extern "C" fn string_clone(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    let ops = unsafe { birnam_string_provider_v1().read() };
    unsafe { ((*ops.storage).clone)(ops.context, receiver, cleanup) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_string_provider_v1() -> *mut birnam_abi::string::StringApiV1 {
    if PHASE.load(Ordering::Acquire) < 2 { return ptr::null_mut(); }
    unsafe { (*INSTANCE.load(Ordering::Acquire)).operations() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_string_prepare_v4(api: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1, allocation: *const birnam_abi::native_allocation::NativeAllocationV1, roots: *const birnam_abi::root_registry::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) -> *mut birnam_abi::string::StringApiV1 {
    if api.is_null() { abort("preparation received a null IdApi"); }
    if PHASE.load(Ordering::Acquire) != 0 {
        let instance = INSTANCE.load(Ordering::Acquire);
        if instance.is_null() || unsafe { (*instance).api() } != api { abort("another IdApi is already mounted"); }
        if PHASE.load(Ordering::Acquire) == 1 { abort("recursive String preparation"); }
        return unsafe { birnam_string_provider_v1() };
    }
    if allocation.is_null() { abort("native allocation capability missing"); }
    PHASE.store(1, Ordering::Release);
    let instance = Box::into_raw(unsafe { storage::Instance::empty(api, exports, layouts, allocation.read(), cleanup, cleanup_owner) });
    INSTANCE.store(instance, Ordering::Release);
    unsafe { birnam_abi::root_registry::register(roots, storage_roots); (*instance).prepare(string_clone); }
    let ops = unsafe { (*instance).operations() };
    if unsafe { birnam_abi::string::attach(ops) } != 1 { abort("invalid String table"); }
    PHASE.store(2, Ordering::Release);
    ops
}

unsafe fn string_copy(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
    if object.is_null() || unsafe { (api.vtableOf)(object) } != unsafe { birnam_abi::string::behavior() } {
        return None;
    }
    let mut length = 0;
    let bytes = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
    if length == 0 {
        return Some(Vec::new());
    }
    if bytes.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec())
}

unsafe extern "C" fn string_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    self_object
}

unsafe extern "C" fn string_comma(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(mut other) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(other) } != unsafe { birnam_abi::string::behavior() } {
        other = unsafe { (api.send)(other, runtime().print_string as Oop, ptr::null(), 0) };
    }
    let Some(left) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { string_copy(api, other) }) else {
        return unsafe { (birnam_abi::string::make)(left.as_ptr(), left.len()) };
    };
    let mut output = Vec::with_capacity(left.len() + right.len());
    output.extend_from_slice(&left);
    output.extend_from_slice(&right);
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn string_equals(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(other) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let equal = unsafe { string_copy(api, self_object) }
        .zip(unsafe { string_copy(api, other) })
        .is_some_and(|(left, right)| left == right);
    if equal { api.trueObj } else { api.falseObj }
}

unsafe extern "C" fn string_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    unsafe { (birnam_abi::integer::make)(bytes.len() as i64) }
}

unsafe extern "C" fn string_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if index < 0 || index as usize >= bytes.len() {
        return api.nilObj;
    }
    let byte = [bytes[index as usize]];
    unsafe { (birnam_abi::string::make)(byte.as_ptr(), 1) }
}

unsafe extern "C" fn string_byte_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if index < 0 || index as usize >= bytes.len() {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(i64::from(bytes[index as usize])) }
}

unsafe extern "C" fn string_copy_from_to(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let from = arguments[0];
    let to = arguments[1];
    if unsafe { (birnam_abi::integer::is_integer)(from) } == 0 || unsafe { (birnam_abi::integer::is_integer)(to) } == 0 {
        return api.nilObj;
    }
    let from = unsafe { (birnam_abi::integer::value)(from) };
    let to = unsafe { (birnam_abi::integer::value)(to) };
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if from < 0 || to < from || to as usize > bytes.len() {
        return api.nilObj;
    }
    unsafe {
        (birnam_abi::string::make)(
            bytes[from as usize..to as usize].as_ptr(),
            (to - from) as usize,
        )
    }
}

unsafe extern "C" fn string_trimmed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c);
    let start = bytes
        .iter()
        .position(|byte| !is_space(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_space(*byte))
        .map_or(start, |index| index + 1);
    unsafe { (birnam_abi::string::make)(bytes[start..end].as_ptr(), end - start) }
}

unsafe extern "C" fn string_index_of(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(needle) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(haystack) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(needle) = (unsafe { string_copy(api, needle) }) else {
        return api.nilObj;
    };
    if needle.is_empty() {
        return unsafe { (birnam_abi::integer::make)(0) };
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle.as_slice())
        .map_or(api.nilObj, |index| unsafe {
            (birnam_abi::integer::make)(index as i64)
        })
}

unsafe extern "C" fn string_bracket_depth(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let mut depth = 0i64;
    let mut in_string = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    unsafe { (birnam_abi::integer::make)(depth) }
}

unsafe extern "C" fn string_replace_with(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let Some(haystack) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(needle) = (unsafe { string_copy(api, arguments[0]) }) else {
        return api.nilObj;
    };
    let Some(replacement) = (unsafe { string_copy(api, arguments[1]) }) else {
        return api.nilObj;
    };
    if needle.is_empty() {
        return unsafe { (birnam_abi::string::make)(haystack.as_ptr(), haystack.len()) };
    }
    let mut output = Vec::new();
    let mut index = 0usize;
    while index < haystack.len() {
        if index + needle.len() <= haystack.len()
            && haystack[index..index + needle.len()] == needle[..]
        {
            output.extend_from_slice(&replacement);
            index += needle.len();
        } else {
            output.push(haystack[index]);
            index += 1;
        }
    }
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn string_as_symbol(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let mut terminated = bytes;
    terminated.push(0);
    unsafe { (api.intern)(terminated.as_ptr().cast::<c_char>()) }
}

unsafe extern "C" fn string_as_integer(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let text = trim_ascii(&bytes);
    match core::str::from_utf8(text)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
    {
        Some(value) => unsafe { (birnam_abi::integer::make)(value) },
        None => api.nilObj,
    }
}

unsafe extern "C" fn string_as_number(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let text = trim_ascii(&bytes);
    let Ok(text) = core::str::from_utf8(text) else {
        return api.nilObj;
    };
    if !text.contains(['.', 'e', 'E'])
        && let Ok(value) = text.parse::<i64>()
    {
        return unsafe { (birnam_abi::integer::make)(value) };
    }
    match text.parse::<f64>() {
        Ok(value) => unsafe { (birnam_abi::float::make)(value) },
        Err(_) => api.nilObj,
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
    let start = bytes
        .iter()
        .position(|byte| !is_space(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_space(*byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

/// Install the existing String methods through the selected floor API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) -> *mut c_void {
    let _ = unsafe { birnam_string_prepare_v4(api_pointer, exports, ptr::null(), roots, layouts) };
    let previous = PHASE.swap(3, Ordering::AcqRel);
    if previous != 2 && previous != 4 { abort("recursive String activation"); }
    let _ = unsafe { birnam_abi::integer::operations() };
    let _ = unsafe { birnam_abi::float::operations() };
    let api = unsafe { &*api_pointer };
    if previous == 2 { unsafe { (*INSTANCE.load(Ordering::Acquire)).publish() }; }
    ROOTS_REGISTERED.call_once(|| unsafe { birnam_abi::root_registry::register(roots, mark_roots) });
    let _ = RUNTIME.get_or_init(|| Runtime {
        api: api_pointer as usize,
        print_string: unsafe { intern(api, b"printString\0") } as usize,
    });
    unsafe {
        (api.method)(birnam_abi::string::behavior(), intern(api, b",\0"), string_comma);
        (api.method)(
            birnam_abi::string::behavior(),
            intern(api, b"printString\0"),
            string_print_string,
        );
        (api.method)(birnam_abi::string::behavior(), intern(api, b"=\0"), string_equals);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"size\0"), string_size);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"at:\0"), string_at);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"byteAt:\0"), string_byte_at);
        (api.method)(
            birnam_abi::string::behavior(),
            intern(api, b"copyFrom:to:\0"),
            string_copy_from_to,
        );
        (api.method)(birnam_abi::string::behavior(), intern(api, b"trimmed\0"), string_trimmed);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"indexOf:\0"), string_index_of);
        (api.method)(
            birnam_abi::string::behavior(),
            intern(api, b"bracketDepth\0"),
            string_bracket_depth,
        );
        (api.method)(
            birnam_abi::string::behavior(),
            intern(api, b"replace:with:\0"),
            string_replace_with,
        );
        (api.method)(birnam_abi::string::behavior(), intern(api, b"asSymbol\0"), string_as_symbol);
        (api.method)(
            birnam_abi::string::behavior(),
            intern(api, b"asInteger\0"),
            string_as_integer,
        );
        (api.method)(birnam_abi::string::behavior(), intern(api, b"asNumber\0"), string_as_number);
    }
    PHASE.store(4, Ordering::Release);
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::integer_dependency!();
birnam_abi::float_dependency!();
