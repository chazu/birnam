//! Map canonical identities, representation and native HAMT storage.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::map::{ApiV1, BehaviorsV1};
use birnam_abi::{IdApi, MarkFn, Oop};
use core::ffi::c_void;
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
}
unsafe extern "C" fn prepare(api_pointer: *mut IdApi, parent: Oop, adopting: i32) -> BehaviorsV1 {
    if !READY.load(Ordering::Acquire) || api_pointer.is_null() {
        abort("invalid Map preparation");
    }
    let api = unsafe { &*api_pointer };
    ROOTS_REGISTERED.call_once(|| unsafe { (api.gcRegisterRootProvider)(mark_roots) });
    let state = FAMILY.get_or_init(|| {
        let map = if adopting != 0 {
            unsafe { (api.import)(MAP_VT_NAME.as_ptr().cast()) }
        } else {
            unsafe { (api.proto)(parent) }
        };
        IDENTITY_ROOTS[0].store(map as usize, Ordering::Release);
        let prototype = if adopting != 0 {
            unsafe { (api.import)(MAP_NAME.as_ptr().cast()) }
        } else {
            ptr::null_mut()
        };
        IDENTITY_ROOTS[1].store(prototype as usize, Ordering::Release);
        let node = if adopting != 0 {
            unsafe { (api.import)(MAP_NODE_NAME.as_ptr().cast()) }
        } else {
            unsafe { (api.proto)(api.object_vt) }
        };
        IDENTITY_ROOTS[2].store(node as usize, Ordering::Release);
        Family {
            api: api_pointer as usize,
            map_vt: map as usize,
            map_node_vt: node as usize,
        }
    });
    BehaviorsV1 {
        map: state.map_vt as Oop,
        node: state.map_node_vt as Oop,
    }
}
unsafe extern "C" fn finish_prototype(api_pointer: *mut IdApi) {
    if IDENTITY_ROOTS[1].load(Ordering::Acquire) == 0 {
        let prototype = unsafe { empty_map(&*api_pointer, family()) };
        IDENTITY_ROOTS[1].store(prototype as usize, Ordering::Release);
    }
}
unsafe extern "C" fn set_layout(api_pointer: *mut IdApi) {
    let api = unsafe { &*api_pointer };
    unsafe {
        (api.setLayout)(
            family().map_node_vt as Oop,
            HNODE_LAYOUT.as_ptr(),
            HNODE_LAYOUT.len(),
            birnam_abi::SLOT_OOP,
        );
        (api.setLayout)(
            family().map_vt as Oop,
            MAP_LAYOUT.as_ptr(),
            MAP_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
    }
}
unsafe extern "C" fn publish(api_pointer: *mut IdApi) {
    let api = unsafe { &*api_pointer };
    unsafe {
        birnam_abi::set_vtable_owner(
            api,
            family().map_vt as Oop,
            IDENTITY_ROOTS[1].load(Ordering::Acquire) as Oop,
        );
        (api.exportObj)(MAP_VT_NAME.as_ptr().cast(), family().map_vt as Oop);
        (api.exportObj)(
            MAP_NAME.as_ptr().cast(),
            IDENTITY_ROOTS[1].load(Ordering::Acquire) as Oop,
        );
        (api.exportObj)(MAP_NODE_NAME.as_ptr().cast(), family().map_node_vt as Oop);
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
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    prepare: Some(prepare),
    finish_prototype: Some(finish_prototype),
    set_layout: Some(set_layout),
    publish: Some(publish),
    empty: Some(empty),
    put: Some(put),
    find: Some(find),
    count: Some(count),
    root: Some(root),
    slot_count: Some(slot_count),
    slot: Some(slot),
    behavior: Some(behavior),
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
pub extern "C" fn birnam_map_provider_v1() -> *const ApiV1 {
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
birnam_abi::collection_value_dependency!();
