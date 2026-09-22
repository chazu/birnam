//! Map canonical identities, HAMT storage, methods, traversal and printing.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::c_char;
use core::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};
const MAP_NAME: &[u8] = b"Map\0";
const MAP_VT_NAME: &[u8] = b"MapVt\0";
const MAP_NODE_NAME: &[u8] = b"MapNode\0";

const BITS: usize = 5;
const MASK: usize = (1 << BITS) - 1;

static MAP_LAYOUT: [u8; 2] = [birnam_abi::SLOT_OOP, birnam_abi::SLOT_RAW];
static HNODE_LAYOUT: [u8; 2] = [birnam_abi::SLOT_RAW, birnam_abi::SLOT_RAW];

const HNODE_BITMAP: u32 = 0;
const HNODE_COLLISION: u32 = 1;

#[derive(Clone, Copy)]
#[repr(C)]
struct HNodeHeader {
    tag: u32,
    n: u32,
    hash: u32,
    pad: u32,
}

#[repr(C)]
struct MapPayload {
    root: Oop,
    count: usize,
}

const _: () = {
    assert!(size_of::<MapPayload>() == 16);
    assert!(core::mem::offset_of!(MapPayload, count) == 8);
    assert!(size_of::<HNodeHeader>() == 16);
    assert!(core::mem::offset_of!(HNodeHeader, hash) == 8);
};
struct Family {
    api: usize,
    map_vt: usize,
    map_node_vt: usize,
}
static FAMILY: OnceLock<Family> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
static IDENTITY_ROOTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
// Vector owns and roots this prototype for the module lifetime.
static VECTOR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe fn send(receiver: Oop, selector: &core::ffi::CStr, args: &[Oop]) -> Oop {
    let api = unsafe { api_ref() };
    unsafe { (api.send)(receiver, (api.intern)(selector.as_ptr()), args.as_ptr(), args.len()) }
}
static READY: AtomicBool = AtomicBool::new(false);
fn abort(message: &str) -> ! {
    eprintln!("birnam Map: {message}");
    std::process::abort();
}
fn family() -> &'static Family {
    FAMILY
        .get()
        .unwrap_or_else(|| abort("Map family was not prepared"))
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    for root in &IDENTITY_ROOTS {
        let object = root.load(Ordering::Acquire) as Oop;
        if !object.is_null() {
            unsafe { mark(object) };
        }
    }
    if let Some(runtime) = RUNTIME.get() {
        for value in [runtime.value, runtime.value2, runtime.print_string] {
            unsafe { mark(value as Oop) };
        }
    }
}
unsafe extern "C" fn prepare(api_pointer: *mut IdApi, roots: *const birnam_abi::root_registry::ApiV1, parent: Oop) {
    if !READY.load(Ordering::Acquire) || api_pointer.is_null() {
        abort("invalid Map preparation");
    }
    let api = unsafe { &*api_pointer };
    ROOTS_REGISTERED.call_once(|| unsafe { birnam_abi::root_registry::register(roots, mark_roots) });
    FAMILY.get_or_init(|| {
        let map = unsafe { (api.proto)(parent) };
        IDENTITY_ROOTS[0].store(map as usize, Ordering::Release);
        let node = unsafe { (api.proto)(api.object_vt) };
        IDENTITY_ROOTS[2].store(node as usize, Ordering::Release);
        Family {
            api: api_pointer as usize,
            map_vt: map as usize,
            map_node_vt: node as usize,
        }
    });
}
unsafe extern "C" fn finish_prototype(api_pointer: *mut IdApi) {
    if IDENTITY_ROOTS[1].load(Ordering::Acquire) == 0 {
        let prototype = unsafe { empty_map(&*api_pointer, family()) };
        IDENTITY_ROOTS[1].store(prototype as usize, Ordering::Release);
    }
}
unsafe extern "C" fn set_layout(_api_pointer: *mut IdApi, layouts: *const birnam_abi::payload_layout::ApiV1) {
    unsafe {
        birnam_abi::payload_layout::set(layouts,
            family().map_node_vt as Oop,
            HNODE_LAYOUT.as_ptr(),
            HNODE_LAYOUT.len(),
            birnam_abi::SLOT_OOP,
        );
        birnam_abi::payload_layout::set(layouts,
            family().map_vt as Oop,
            MAP_LAYOUT.as_ptr(),
            MAP_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
    }
}
unsafe extern "C" fn publish(api_pointer: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1) {
    let api = unsafe { &*api_pointer };
    unsafe {
        birnam_abi::set_vtable_owner(
            api,
            family().map_vt as Oop,
            IDENTITY_ROOTS[1].load(Ordering::Acquire) as Oop,
        );
        birnam_abi::namespace_export::export(exports, MAP_VT_NAME.as_ptr().cast(), family().map_vt as Oop);
        birnam_abi::namespace_export::export(exports,
            MAP_NAME.as_ptr().cast(),
            IDENTITY_ROOTS[1].load(Ordering::Acquire) as Oop,
        );
        birnam_abi::namespace_export::export(exports, MAP_NODE_NAME.as_ptr().cast(), family().map_node_vt as Oop);
    }
}
unsafe fn map_payload(object: Oop) -> *mut MapPayload {
    if object.is_null() {
        abort("Map method received a null receiver");
    }
    object.cast::<MapPayload>()
}

unsafe fn hnode_header(object: Oop) -> *mut HNodeHeader {
    if object.is_null() {
        abort("Map node was null");
    }
    object.cast::<HNodeHeader>()
}

unsafe fn hnode_header_value(object: Oop) -> HNodeHeader {
    unsafe { ptr::read(hnode_header(object)) }
}

fn hnode_slot_count(header: HNodeHeader) -> usize {
    if header.tag == HNODE_BITMAP {
        2 * header.n.count_ones() as usize
    } else {
        2 * header.n as usize
    }
}

unsafe fn hnode_slot(object: Oop, index: usize) -> Oop {
    let slots = unsafe {
        object
            .cast::<u8>()
            .add(size_of::<HNodeHeader>())
            .cast::<Oop>()
    };
    unsafe { slots.add(index).read() }
}

unsafe fn set_hnode_slot(object: Oop, index: usize, value: Oop) {
    let slots = unsafe {
        object
            .cast::<u8>()
            .add(size_of::<HNodeHeader>())
            .cast::<Oop>()
    };
    unsafe { slots.add(index).write(value) };
}

unsafe fn new_hnode(
    api: &IdApi,
    runtime: &Family,
    tag: u32,
    n: u32,
    hash: u32,
    slot_count: usize,
) -> Oop {
    let size = size_of::<HNodeHeader>() + slot_count * size_of::<Oop>();
    let node = unsafe { (api.alloc)(runtime.map_node_vt as Oop, size) };
    if node.is_null() {
        abort("MapNode allocation failed");
    }
    unsafe {
        ptr::write(
            hnode_header(node),
            HNodeHeader {
                tag,
                n,
                hash,
                pad: 0,
            },
        );
        for index in 0..slot_count {
            set_hnode_slot(node, index, ptr::null_mut());
        }
    }
    node
}

unsafe fn copy_hnode(api: &IdApi, runtime: &Family, source: Oop) -> Oop {
    let header = unsafe { hnode_header_value(source) };
    let slot_count = hnode_slot_count(header);
    let destination =
        unsafe { new_hnode(api, runtime, header.tag, header.n, header.hash, slot_count) };
    for index in 0..slot_count {
        let value = unsafe { hnode_slot(source, index) };
        unsafe { set_hnode_slot(destination, index, value) };
    }
    destination
}

unsafe fn empty_map(api: &IdApi, runtime: &Family) -> Oop {
    let map = unsafe { (api.alloc)(runtime.map_vt as Oop, size_of::<MapPayload>()) };
    if map.is_null() {
        abort("Map allocation failed");
    }
    unsafe {
        ptr::write(
            map_payload(map),
            MapPayload {
                root: ptr::null_mut(),
                count: 0,
            },
        );
    }
    map
}

fn map_chunk(hash: u32, shift: usize) -> u32 {
    (hash >> shift.min(31)) & MASK as u32
}

fn map_bit(hash: u32, shift: usize) -> u32 {
    1u32 << map_chunk(hash, shift)
}

fn map_sparse_index(bitmap: u32, bit: u32) -> usize {
    (bitmap & bit.wrapping_sub(1)).count_ones() as usize
}

unsafe fn map_find(node: Oop, shift: usize, hash: u32, key: Oop, api: &IdApi) -> Oop {
    if node.is_null() {
        return ptr::null_mut();
    }
    let header = unsafe { hnode_header_value(node) };
    if header.tag == HNODE_BITMAP {
        let bit = map_bit(hash, shift);
        if header.n & bit == 0 {
            return ptr::null_mut();
        }
        let index = 2 * map_sparse_index(header.n, bit);
        let stored_key = unsafe { hnode_slot(node, index) };
        if stored_key.is_null() {
            let child = unsafe { hnode_slot(node, index + 1) };
            return unsafe { map_find(child, shift + BITS, hash, key, api) };
        }
        if unsafe { birnam_abi::collection_value::same_value(api, stored_key, key) } {
            return unsafe { hnode_slot(node, index + 1) };
        }
        return ptr::null_mut();
    }
    if hash != header.hash {
        return ptr::null_mut();
    }
    for entry in 0..header.n as usize {
        let index = 2 * entry;
        let stored_key = unsafe { hnode_slot(node, index) };
        if unsafe { birnam_abi::collection_value::same_value(api, stored_key, key) } {
            return unsafe { hnode_slot(node, index + 1) };
        }
    }
    ptr::null_mut()
}

#[allow(clippy::too_many_arguments)]
unsafe fn map_merge_two(
    api: &IdApi,
    runtime: &Family,
    shift: usize,
    first_key: Oop,
    first_hash: u32,
    first_value: Oop,
    second_key: Oop,
    second_hash: u32,
    second_value: Oop,
) -> Oop {
    if first_hash == second_hash {
        let node = unsafe { new_hnode(api, runtime, HNODE_COLLISION, 2, first_hash, 4) };
        unsafe {
            set_hnode_slot(node, 0, first_key);
            set_hnode_slot(node, 1, first_value);
            set_hnode_slot(node, 2, second_key);
            set_hnode_slot(node, 3, second_value);
        }
        return node;
    }
    let first_chunk = map_chunk(first_hash, shift);
    let second_chunk = map_chunk(second_hash, shift);
    if first_chunk == second_chunk {
        let child = unsafe {
            map_merge_two(
                api,
                runtime,
                shift + BITS,
                first_key,
                first_hash,
                first_value,
                second_key,
                second_hash,
                second_value,
            )
        };
        let node = unsafe { new_hnode(api, runtime, HNODE_BITMAP, 1u32 << first_chunk, 0, 2) };
        unsafe {
            set_hnode_slot(node, 0, ptr::null_mut());
            set_hnode_slot(node, 1, child);
        }
        return node;
    }
    let node = unsafe {
        new_hnode(
            api,
            runtime,
            HNODE_BITMAP,
            (1u32 << first_chunk) | (1u32 << second_chunk),
            0,
            4,
        )
    };
    if first_chunk < second_chunk {
        unsafe {
            set_hnode_slot(node, 0, first_key);
            set_hnode_slot(node, 1, first_value);
            set_hnode_slot(node, 2, second_key);
            set_hnode_slot(node, 3, second_value);
        }
    } else {
        unsafe {
            set_hnode_slot(node, 0, second_key);
            set_hnode_slot(node, 1, second_value);
            set_hnode_slot(node, 2, first_key);
            set_hnode_slot(node, 3, first_value);
        }
    }
    node
}

static MAP_ADDED: AtomicBool = AtomicBool::new(false);

unsafe fn map_assoc(
    api: &IdApi,
    runtime: &Family,
    node: Oop,
    shift: usize,
    hash: u32,
    key: Oop,
    value: Oop,
) -> Oop {
    if node.is_null() {
        MAP_ADDED.store(true, Ordering::Relaxed);
        let result = unsafe { new_hnode(api, runtime, HNODE_BITMAP, map_bit(hash, shift), 0, 2) };
        unsafe {
            set_hnode_slot(result, 0, key);
            set_hnode_slot(result, 1, value);
        }
        return result;
    }
    let header = unsafe { hnode_header_value(node) };
    if header.tag == HNODE_BITMAP {
        let bit = map_bit(hash, shift);
        let index = 2 * map_sparse_index(header.n, bit);
        if header.n & bit != 0 {
            let stored_key = unsafe { hnode_slot(node, index) };
            if stored_key.is_null() {
                let child = unsafe { hnode_slot(node, index + 1) };
                let replacement =
                    unsafe { map_assoc(api, runtime, child, shift + BITS, hash, key, value) };
                if replacement == child {
                    return node;
                }
                let result = unsafe { copy_hnode(api, runtime, node) };
                unsafe { set_hnode_slot(result, index + 1, replacement) };
                return result;
            }
            if unsafe { birnam_abi::collection_value::same_value(api, stored_key, key) } {
                let old_value = unsafe { hnode_slot(node, index + 1) };
                if old_value == value {
                    return node;
                }
                let result = unsafe { copy_hnode(api, runtime, node) };
                unsafe { set_hnode_slot(result, index + 1, value) };
                return result;
            }
            MAP_ADDED.store(true, Ordering::Relaxed);
            let child = unsafe {
                map_merge_two(
                    api,
                    runtime,
                    shift + BITS,
                    stored_key,
                    birnam_abi::collection_value::key_hash(api, stored_key),
                    hnode_slot(node, index + 1),
                    key,
                    hash,
                    value,
                )
            };
            let result = unsafe { copy_hnode(api, runtime, node) };
            unsafe {
                set_hnode_slot(result, index, ptr::null_mut());
                set_hnode_slot(result, index + 1, child);
            }
            return result;
        }
        MAP_ADDED.store(true, Ordering::Relaxed);
        let old_count = header.n.count_ones() as usize;
        let result = unsafe {
            new_hnode(
                api,
                runtime,
                HNODE_BITMAP,
                header.n | bit,
                0,
                2 * (old_count + 1),
            )
        };
        for old_index in 0..2 * old_count {
            let destination = if old_index < 2 * map_sparse_index(header.n, bit) {
                old_index
            } else {
                old_index + 2
            };
            unsafe { set_hnode_slot(result, destination, hnode_slot(node, old_index)) };
        }
        let insertion = 2 * map_sparse_index(header.n, bit);
        unsafe {
            set_hnode_slot(result, insertion, key);
            set_hnode_slot(result, insertion + 1, value);
        }
        return result;
    }
    if hash == header.hash {
        for entry in 0..header.n as usize {
            let index = 2 * entry;
            let stored_key = unsafe { hnode_slot(node, index) };
            if unsafe { birnam_abi::collection_value::same_value(api, stored_key, key) } {
                let old_value = unsafe { hnode_slot(node, index + 1) };
                if old_value == value {
                    return node;
                }
                let result = unsafe { copy_hnode(api, runtime, node) };
                unsafe { set_hnode_slot(result, index + 1, value) };
                return result;
            }
        }
        MAP_ADDED.store(true, Ordering::Relaxed);
        let old_count = header.n as usize;
        let result = unsafe {
            new_hnode(
                api,
                runtime,
                HNODE_COLLISION,
                header.n + 1,
                header.hash,
                2 * (old_count + 1),
            )
        };
        for index in 0..2 * old_count {
            unsafe { set_hnode_slot(result, index, hnode_slot(node, index)) };
        }
        unsafe {
            set_hnode_slot(result, 2 * old_count, key);
            set_hnode_slot(result, 2 * old_count + 1, value);
        }
        return result;
    }
    let wrapper = unsafe {
        new_hnode(
            api,
            runtime,
            HNODE_BITMAP,
            map_bit(header.hash, shift),
            0,
            2,
        )
    };
    unsafe {
        set_hnode_slot(wrapper, 0, ptr::null_mut());
        set_hnode_slot(wrapper, 1, node);
    }
    unsafe { map_assoc(api, runtime, wrapper, shift, hash, key, value) }
}

unsafe extern "C" fn empty() -> Oop {
    unsafe { empty_map(&*(family().api as *const IdApi), family()) }
}
unsafe extern "C" fn put(api_pointer: *const IdApi, self_object: Oop, key: Oop, value: Oop) -> Oop {
    let api = unsafe { &*api_pointer };
    let payload = unsafe { map_payload(self_object) };
    let root = unsafe { (*payload).root };
    MAP_ADDED.store(false, Ordering::Relaxed);
    let replacement = unsafe {
        map_assoc(
            api,
            family(),
            root,
            0,
            birnam_abi::collection_value::key_hash(api, key),
            key,
            value,
        )
    };
    if replacement == root {
        return self_object;
    }
    let old_count = unsafe { (*payload).count };
    let map = unsafe { (api.alloc)(family().map_vt as Oop, size_of::<MapPayload>()) };
    if map.is_null() {
        abort("Map allocation failed");
    }
    unsafe {
        ptr::write(
            map_payload(map),
            MapPayload {
                root: replacement,
                count: old_count + usize::from(MAP_ADDED.load(Ordering::Relaxed)),
            },
        );
    }
    map
}
unsafe extern "C" fn find(api_pointer: *const IdApi, self_object: Oop, key: Oop) -> Oop {
    let api = unsafe { &*api_pointer };
    unsafe {
        map_find(
            (*map_payload(self_object)).root,
            0,
            birnam_abi::collection_value::key_hash(api, key),
            key,
            api,
        )
    }
}
unsafe extern "C" fn count(object: Oop) -> usize {
    unsafe { (*map_payload(object)).count }
}
unsafe extern "C" fn root(object: Oop) -> Oop {
    unsafe { (*map_payload(object)).root }
}
unsafe extern "C" fn slot_count(node: Oop) -> usize {
    hnode_slot_count(unsafe { hnode_header_value(node) })
}
unsafe extern "C" fn slot(node: Oop, index: usize) -> Oop {
    unsafe { hnode_slot(node, index) }
}
unsafe extern "C" fn behavior() -> Oop {
    family().map_vt as Oop
}
struct Runtime {
    api: usize,
    value: usize,
    value2: usize,
    print_string: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("Map methods were not installed"))
}
unsafe fn api_ref() -> &'static IdApi {
    // Family retains the first Collection-selected table for the process lifetime.
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
unsafe fn map_collect_values(node: Oop, mut result: Oop, keys: bool) -> Oop {
    if node.is_null() {
        return result;
    }
    let slots = unsafe { self::slot_count(node) };
    for index in (0..slots).step_by(2) {
        let key = unsafe { self::slot(node, index) };
        if key.is_null() {
            let child = unsafe { self::slot(node, index + 1) };
            result = unsafe { map_collect_values(child, result, keys) };
        } else {
            let item = if keys {
                key
            } else {
                unsafe { self::slot(node, index + 1) }
            };
            result = unsafe { send(result, c"add:", &[item]) };
        }
    }
    result
}

unsafe fn map_walk_keys_and_values_do(node: Oop, block: Oop, api: &IdApi) {
    if node.is_null() {
        return;
    }
    let slots = unsafe { self::slot_count(node) };
    let selector = runtime().value2 as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { self::slot(node, index) };
        if key.is_null() {
            unsafe { map_walk_keys_and_values_do(self::slot(node, index + 1), block, api) };
        } else {
            let value = unsafe { self::slot(node, index + 1) };
            let arguments = [key, value];
            unsafe { (api.send)(block, selector, arguments.as_ptr(), 2) };
        }
    }
}

unsafe fn map_append_print(api: &IdApi, node: Oop, output: &mut Vec<u8>, first: &mut bool) -> bool {
    if node.is_null() {
        return true;
    }
    let slots = unsafe { self::slot_count(node) };
    let selector = runtime().print_string as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { self::slot(node, index) };
        if key.is_null() {
            if !unsafe { map_append_print(api, self::slot(node, index + 1), output, first) } {
                return false;
            }
            continue;
        }
        if !*first {
            output.extend_from_slice(b", ");
        }
        *first = false;
        let value = unsafe { self::slot(node, index + 1) };
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
    unsafe { self::empty() }
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
    unsafe { self::put(api, self_object, key, value) }
}

unsafe extern "C" fn map_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(key) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let value = unsafe { self::find(api, self_object, key) };
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
    let value = unsafe { self::find(api, self_object, key) };
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
    let value = unsafe { self::find(api, self_object, key) };
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
    unsafe { (birnam_abi::integer::make)(self::count(self_object) as i64) }
}

unsafe extern "C" fn map_keys(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { send(VECTOR.load(Ordering::Acquire) as Oop, c"new", &[]) };
    unsafe { map_collect_values(self::root(self_object), result, true) }
}

unsafe extern "C" fn map_values(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { send(VECTOR.load(Ordering::Acquire) as Oop, c"new", &[]) };
    unsafe { map_collect_values(self::root(self_object), result, false) }
}

unsafe extern "C" fn map_do(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    if count != 1 {
        return unsafe { api_ref() }.nilObj;
    }
    let values = unsafe { map_values(self_object, ptr::null(), 0) };
    let result = unsafe { send(values, c"do:", borrowed_oop_args(args, count).unwrap()) };
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
    unsafe { send(values, c"inject:into:", borrowed_oop_args(args, count).unwrap()) }
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
    unsafe { map_walk_keys_and_values_do(self::root(self_object), block, api) };
    self_object
}

unsafe extern "C" fn map_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let mut output = Vec::from(&b"{"[..]);
    let mut first = true;
    let ok = unsafe { map_append_print(api, self::root(self_object), &mut output, &mut first) };
    if !ok {
        return api.nilObj;
    }
    output.push(b'}');
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn install(api_pointer: *mut IdApi, value: Oop, value2: Oop, print_string: Oop) {
    RUNTIME.get_or_init(|| Runtime {
        api: family().api,
        value: value as usize,
        value2: value2 as usize,
        print_string: print_string as usize,
    });
    let api = unsafe { &*api_pointer };
    unsafe {
        (api.method)(self::behavior(), intern(api, b"new\0"), map_new);
        (api.method)(self::behavior(), intern(api, b"clone\0"), map_new);
        (api.method)(self::behavior(), intern(api, b"at:put:\0"), map_at_put);
        (api.method)(self::behavior(), intern(api, b"at:\0"), map_at);
        (api.method)(
            self::behavior(),
            intern(api, b"at:ifAbsent:\0"),
            map_at_if_absent,
        );
        (api.method)(
            self::behavior(),
            intern(api, b"includesKey:\0"),
            map_includes_key,
        );
        (api.method)(self::behavior(), intern(api, b"size\0"), map_size);
        (api.method)(self::behavior(), intern(api, b"keys\0"), map_keys);
        (api.method)(self::behavior(), intern(api, b"values\0"), map_values);
        (api.method)(self::behavior(), intern(api, b"do:\0"), map_do);
        (api.method)(self::behavior(), intern(api, b"inject:into:\0"), map_inject);
        (api.method)(
            self::behavior(),
            intern(api, b"keysAndValuesDo:\0"),
            map_keys_and_values_do,
        );
        (api.method)(
            self::behavior(),
            intern(api, b"printString\0"),
            map_print_string,
        );
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, presence, imports, exports, roots, layouts, integer, string, vector, policy, parent]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let (Some(api), Some(presence), Some(imports), Some(exports), Some(roots), Some(layouts)) = (api.as_word(), presence.as_word(), imports.as_word(), exports.as_word(), roots.as_word(), layouts.as_word()) else { return invalid_native(); };
    let api = api as *mut IdApi;
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let layouts = layouts as *const birnam_abi::payload_layout::ApiV1;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || !unsafe { birnam_abi::namespace_presence::valid(presence) && birnam_abi::namespace_import::valid(imports)
            && birnam_abi::namespace_export::valid(exports) && birnam_abi::root_registry::valid_owned(roots)
            && birnam_abi::payload_layout::valid(layouts) } { return invalid_native(); }
    let (Some(integer), Some(string), Some(vector), Some(policy)) = (integer.as_word(), string.as_word(), vector.as_object().filter(|p| !p.is_null()), policy.as_word()) else { return invalid_native(); };
    let Some(parent) = parent.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if unsafe { birnam_abi::integer::attach(integer as _) } != 1 || unsafe { birnam_abi::string::attach(string as _) } != 1 || unsafe { birnam_abi::collection_value::attach(policy as _) } != 1 { return invalid_native(); }
    if FAMILY.get().is_some_and(|selected| selected.api != api.addr()) { return invalid_native(); }
    if VECTOR.load(Ordering::Acquire) != 0 && VECTOR.load(Ordering::Acquire) != vector.addr() { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::unit()), _ => return invalid_native(),
    }
    VECTOR.store(vector.addr(), Ordering::Release);
    READY.store(true, Ordering::Release);
    unsafe { prepare(api, (*roots).api, parent); finish_prototype(api); set_layout(api, layouts); }
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let value = unsafe { retained.keep(intern(&*api, b"value\0")) };
    let value2 = unsafe { retained.keep(intern(&*api, b"value:value:\0")) };
    let print_string = unsafe { retained.keep(intern(&*api, b"printString\0")) };
    unsafe { install(api, value, value2, print_string); publish(api, exports); }
    NATIVE_PHASE.store(2, std::sync::atomic::Ordering::Release);
    Outcome::loaded(Operand::unit())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
