//! Collection parent, Map methods and collection adapters.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Once, OnceLock};

const RUST_COLLECTION_IDENTITY: &[u8] = b"rust-collection-r1-vector\0";
const COLLECTION_NAME: &[u8] = b"Collection\0";
const COLLECTION_VT_NAME: &[u8] = b"CollectionVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    collection_vt: usize,
    value: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.collection_vt,
            runtime.value,
            runtime.value1,
            runtime.value2,
            runtime.print_string,
        ] {
            // SAFETY: these are the collection module's process-owned cached
            // Oops and remain live while its methods can execute.
            unsafe { mark(value as Oop) };
        }
    }
}
fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Collection: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api_ref() -> &'static IdApi {
    // SAFETY: module_init records the process-owned table for the process
    // lifetime; the host retains it while every dynamic rung is live.
    unsafe { &*(runtime().api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    // SAFETY: selectors are static NUL-terminated byte strings.
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}

unsafe fn import(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: names are static NUL-terminated byte strings.
    unsafe { (api.import)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns this carrier for the duration of the method call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn map_collect_values(
    node: Oop,
    mut result: Oop,
    keys: bool,
) -> Oop {
    if node.is_null() {
        return result;
    }
    let slots = unsafe { birnam_abi::map::slot_count(node) };
    for index in (0..slots).step_by(2) {
        let key = unsafe { birnam_abi::map::slot(node, index) };
        if key.is_null() {
            let child = unsafe { birnam_abi::map::slot(node, index + 1) };
            result = unsafe { map_collect_values(child, result, keys) };
        } else {
            let item = if keys {
                key
            } else {
                unsafe { birnam_abi::map::slot(node, index + 1) }
            };
            result = unsafe { birnam_abi::vector::push(result, item) };
        }
    }
    result
}

unsafe fn map_walk_keys_and_values_do(node: Oop, block: Oop, api: &IdApi) {
    if node.is_null() {
        return;
    }
    let slots = unsafe { birnam_abi::map::slot_count(node) };
    let selector = runtime().value2 as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { birnam_abi::map::slot(node, index) };
        if key.is_null() {
            unsafe { map_walk_keys_and_values_do(birnam_abi::map::slot(node, index + 1), block, api) };
        } else {
            let value = unsafe { birnam_abi::map::slot(node, index + 1) };
            let arguments = [key, value];
            unsafe { (api.send)(block, selector, arguments.as_ptr(), 2) };
        }
    }
}

unsafe fn map_append_print(api: &IdApi, node: Oop, output: &mut Vec<u8>, first: &mut bool) -> bool {
    if node.is_null() {
        return true;
    }
    let slots = unsafe { birnam_abi::map::slot_count(node) };
    let selector = runtime().print_string as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { birnam_abi::map::slot(node, index) };
        if key.is_null() {
            if !unsafe { map_append_print(api, birnam_abi::map::slot(node, index + 1), output, first) } {
                return false;
            }
            continue;
        }
        if !*first {
            output.extend_from_slice(b", ");
        }
        *first = false;
        let value = unsafe { birnam_abi::map::slot(node, index + 1) };
        for (position, object) in [key, value].into_iter().enumerate() {
            let printed = unsafe { (api.send)(object, selector, ptr::null(), 0) };
            if printed.is_null() {
                return false;
            }
            let mut length = 0;
            let bytes = unsafe { (birnam_abi::string::bytes)(printed, &mut length) };
            if bytes.is_null() {
                return false;
            }
            output.extend_from_slice(unsafe { core::slice::from_raw_parts(bytes, length) });
            if position == 0 {
                output.extend_from_slice(b" -> ");
            }
        }
    }
    true
}

unsafe extern "C" fn map_new(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { birnam_abi::map::empty() }
}

unsafe extern "C" fn map_at_put(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let key = arguments[0];
    let value = arguments[1];
    if key.is_null() || value.is_null() {
        return api.nilObj;
    }
    unsafe { birnam_abi::map::put(api, self_object, key, value) }
}

unsafe extern "C" fn map_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(key) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let value = unsafe {
        birnam_abi::map::find(api, self_object, key)
    };
    if value.is_null() { api.nilObj } else { value }
}

unsafe extern "C" fn map_at_if_absent(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let key = arguments[0];
    let fallback = arguments[1];
    let value = unsafe {
        birnam_abi::map::find(api, self_object, key)
    };
    if !value.is_null() {
        return value;
    }
    unsafe { (api.send)(fallback, runtime().value as Oop, ptr::null(), 0) }
}

unsafe extern "C" fn map_includes_key(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(key) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let value = unsafe {
        birnam_abi::map::find(api, self_object, key)
    };
    if value.is_null() {
        api.falseObj
    } else {
        api.trueObj
    }
}

unsafe extern "C" fn map_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(birnam_abi::map::count(self_object) as i64) }
}

unsafe extern "C" fn map_keys(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { birnam_abi::vector::empty() };
    unsafe {
        map_collect_values(
            birnam_abi::map::root(self_object),
            result,
            true,
        )
    }
}

unsafe extern "C" fn map_values(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { birnam_abi::vector::empty() };
    unsafe {
        map_collect_values(
            birnam_abi::map::root(self_object),
            result,
            false,
        )
    }
}

unsafe extern "C" fn map_do(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    if count != 1 {
        return unsafe { api_ref() }.nilObj;
    }
    let values = unsafe { map_values(self_object, ptr::null(), 0) };
    let result = unsafe { birnam_abi::vector::for_each(values, args, count) };
    if result.is_null() {
        return result;
    }
    self_object
}

unsafe extern "C" fn map_inject(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    if count != 2 {
        return unsafe { api_ref() }.nilObj;
    }
    let values = unsafe { map_values(self_object, ptr::null(), 0) };
    unsafe { birnam_abi::vector::inject(values, args, count) }
}

unsafe extern "C" fn map_keys_and_values_do(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    unsafe { map_walk_keys_and_values_do(birnam_abi::map::root(self_object), block, api) };
    self_object
}

unsafe extern "C" fn map_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let mut output = Vec::from(&b"{"[..]);
    let mut first = true;
    let ok = unsafe {
        map_append_print(
            api,
            birnam_abi::map::root(self_object),
            &mut output,
            &mut first,
        )
    };
    if !ok {
        return api.nilObj;
    }
    output.push(b'}');
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
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

unsafe extern "C" fn string_split(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(separator) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(separator) } != unsafe { birnam_abi::string::behavior() } {
        let result = unsafe { birnam_abi::vector::empty() };
        return unsafe { birnam_abi::vector::push(result, self_object) };
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(separator) = (unsafe { string_copy(api, separator) }) else {
        return api.nilObj;
    };
    let mut result = unsafe { birnam_abi::vector::empty() };
    if separator.is_empty() {
        let item = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
        return unsafe { birnam_abi::vector::push(result, item) };
    }
    let mut start = 0usize;
    let mut cursor = 0usize;
    while cursor + separator.len() <= bytes.len() {
        if bytes[cursor..cursor + separator.len()] == separator[..] {
            let item = unsafe { (birnam_abi::string::make)(bytes[start..cursor].as_ptr(), cursor - start) };
            result = unsafe { birnam_abi::vector::push(result, item) };
            cursor += separator.len();
            start = cursor;
        } else {
            cursor += 1;
        }
    }
    let item = unsafe { (birnam_abi::string::make)(bytes[start..].as_ptr(), bytes.len() - start) };
    unsafe { birnam_abi::vector::push(result, item) }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(VECTOR_NAME.as_ptr().cast()) != 0 };
        let collection_vt = if reloading {
            unsafe { import(api, COLLECTION_VT_NAME) }
        } else {
            unsafe { (api.proto)(api.object_vt) }
        };
        let vector = unsafe { birnam_abi::vector::prepare(api_pointer, collection_vt, reloading) };
        let range_vt = unsafe { birnam_abi::range::prepare(api_pointer, collection_vt, reloading) };
        let map = unsafe { birnam_abi::map::prepare(api_pointer, collection_vt, reloading) };
        if [collection_vt, vector.node, vector.vector, range_vt, map.map, map.node]
            .iter().any(|value| value.is_null())
        {
            abort("Rust Collection exports were incomplete");
        }
        unsafe { birnam_abi::vector::finish_prototype(api_pointer) };
        unsafe { birnam_abi::range::finish_prototype(api_pointer) };
        unsafe { birnam_abi::map::finish_prototype(api_pointer) };
        Runtime {
            api: api_pointer as usize,
            collection_vt: collection_vt as usize,
            value: unsafe { intern(api, b"value\0") } as usize,
            value1: unsafe { intern(api, b"value:\0") } as usize,
            value2: unsafe { intern(api, b"value:value:\0") } as usize,
            print_string: unsafe { intern(api, b"printString\0") } as usize,
        }
    });
    ROOTS_REGISTERED.call_once(|| unsafe {
        (api.gcRegisterRootProvider)(mark_roots);
    });
    unsafe {
        birnam_abi::vector::set_layout(api_pointer);
        birnam_abi::range::set_layout(api_pointer);
        birnam_abi::map::set_layout(api_pointer);
        birnam_abi::set_vtable_parent(api, birnam_abi::string::behavior(), runtime.collection_vt as Oop);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"split:\0"), string_split);
        let vector_comma = birnam_abi::vector::install(
            api_pointer,
            runtime.value1 as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
        );
        birnam_abi::range::install(
            api_pointer,
            runtime.value1 as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
            vector_comma,
        );
        (api.method)(birnam_abi::map::behavior(), intern(api, b"new\0"), map_new);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"clone\0"), map_new);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"at:put:\0"), map_at_put);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"at:\0"), map_at);
        (api.method)(
            birnam_abi::map::behavior(),
            intern(api, b"at:ifAbsent:\0"),
            map_at_if_absent,
        );
        (api.method)(
            birnam_abi::map::behavior(),
            intern(api, b"includesKey:\0"),
            map_includes_key,
        );
        (api.method)(birnam_abi::map::behavior(), intern(api, b"size\0"), map_size);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"keys\0"), map_keys);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"values\0"), map_values);
        (api.method)(birnam_abi::map::behavior(), intern(api, b"do:\0"), map_do);
        (api.method)(
            birnam_abi::map::behavior(),
            intern(api, b"inject:into:\0"),
            map_inject,
        );
        (api.method)(
            birnam_abi::map::behavior(),
            intern(api, b"keysAndValuesDo:\0"),
            map_keys_and_values_do,
        );
        (api.method)(
            birnam_abi::map::behavior(),
            intern(api, b"printString\0"),
            map_print_string,
        );
    }
    if unsafe { (api.isExported)(VECTOR_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            birnam_abi::vector::set_owner(api_pointer);
            let _ = (api.exportObj)(
                COLLECTION_VT_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            let _ = (api.exportObj)(
                COLLECTION_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            birnam_abi::vector::publish(api_pointer);
            birnam_abi::range::publish(api_pointer);
            birnam_abi::map::publish(api_pointer);
        }
    }
}

/// Identifies this explicit Rust Collection vector fixture.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_collection_rust_identity() -> *const c_char {
    RUST_COLLECTION_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust Collection vector protocol against the process floor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void) -> *mut c_void {
    let _ = state;
    unsafe { bring_up(api) };
    ptr::null_mut()
}

birnam_abi::integer_dependency!();

birnam_abi::map_dependency!();

birnam_abi::string_dependency!();

birnam_abi::range_dependency!();

birnam_abi::vector_dependency!();
