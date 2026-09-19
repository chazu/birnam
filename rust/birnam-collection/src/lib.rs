//! Collection parent, Vector methods, Map storage and collection adapters.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{CStr, c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};

const RUST_COLLECTION_IDENTITY: &[u8] = b"rust-collection-r1-vector\0";
const COLLECTION_NAME: &[u8] = b"Collection\0";
const COLLECTION_VT_NAME: &[u8] = b"CollectionVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
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

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    collection_vt: usize,
    map_vt: usize,
    map_proto: usize,
    map_node_vt: usize,
    value: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static SORT_ROOTS_REGISTERED: Once = Once::new();
thread_local! {
    static SORT_ROOTS: RefCell<Vec<Oop>> = const { RefCell::new(Vec::new()) };
}

struct SortRootScope {
    base: usize,
}

impl SortRootScope {
    fn new(initial: &[Oop]) -> Self {
        let base = SORT_ROOTS.with(|roots| {
            let mut roots = roots.borrow_mut();
            let base = roots.len();
            roots.extend_from_slice(initial);
            base
        });
        Self { base }
    }

    fn push(&self, value: Oop) {
        SORT_ROOTS.with(|roots| roots.borrow_mut().push(value));
    }
}

impl Drop for SortRootScope {
    fn drop(&mut self) {
        SORT_ROOTS.with(|roots| roots.borrow_mut().truncate(self.base));
    }
}

unsafe extern "C" fn mark_sort_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.collection_vt,
            runtime.map_vt,
            runtime.map_proto,
            runtime.map_node_vt,
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
    SORT_ROOTS.with(|roots| {
        let roots = roots.borrow();
        for &root in roots.iter() {
            // SAFETY: the collector supplies a synchronous mark callback and
            // this thread-local stack contains Oops copied from active sends.
            unsafe { mark(root) };
        }
    });
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

unsafe fn same_value(api: &IdApi, left: Oop, right: Oop) -> bool {
    if left == right {
        return true;
    }
    if left.is_null() || right.is_null() {
        return false;
    }
    if unsafe { (birnam_abi::integer::is_integer)(left) } != 0 && unsafe { (birnam_abi::integer::is_integer)(right) } != 0 {
        return unsafe { (birnam_abi::integer::value)(left) == (birnam_abi::integer::value)(right) };
    }
    if unsafe { (birnam_abi::float::is_float)(left) } != 0 && unsafe { (birnam_abi::float::is_float)(right) } != 0 {
        return unsafe { (birnam_abi::float::value)(left) == (birnam_abi::float::value)(right) };
    }
    if unsafe { (api.vtableOf)(left) } != unsafe { birnam_abi::string::behavior() }
        || unsafe { (api.vtableOf)(right) } != unsafe { birnam_abi::string::behavior() }
    {
        return false;
    }
    let mut left_len = 0;
    let mut right_len = 0;
    let left_bytes = unsafe { (birnam_abi::string::bytes)(left, &mut left_len) };
    let right_bytes = unsafe { (birnam_abi::string::bytes)(right, &mut right_len) };
    if left_len != right_len {
        return false;
    }
    // SAFETY: stringBytes supplies borrowed byte extents for this synchronous
    // comparison; both slices end before any foreign call.
    unsafe {
        core::slice::from_raw_parts(left_bytes, left_len)
            == core::slice::from_raw_parts(right_bytes, right_len)
    }
}

unsafe extern "C" fn vector_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(birnam_abi::vector::count(self_object) as i64) }
}

unsafe extern "C" fn vector_at_method(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    if index < 0 {
        return api.nilObj;
    }
    let value = unsafe { birnam_abi::vector::at(self_object, index as usize) };
    if value.is_null() { api.nilObj } else { value }
}

unsafe extern "C" fn vector_add(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    unsafe { birnam_abi::vector::push(self_object, value) }
}

unsafe extern "C" fn vector_do(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let selector = runtime().value1 as Oop;
    let total = unsafe { birnam_abi::vector::count(self_object) };
    for index in 0..total {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        let arguments = [value];
        // SAFETY: the argument carrier is stack-local and the send is
        // synchronous; no Rust borrow is held across re-entry.
        unsafe { (api.send)(block, selector, arguments.as_ptr(), arguments.len()) };
    }
    self_object
}

unsafe extern "C" fn vector_inject(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let block = arguments[1];
    let selector = runtime().value2 as Oop;
    let total = unsafe { birnam_abi::vector::count(self_object) };
    let mut accumulator = arguments[0];
    for index in 0..total {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        let call_arguments = [accumulator, value];
        accumulator = unsafe { (api.send)(block, selector, call_arguments.as_ptr(), 2) };
        if accumulator.is_null() {
            return api.nilObj;
        }
    }
    accumulator
}

unsafe extern "C" fn vector_index_of(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(needle) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let total = unsafe { birnam_abi::vector::count(self_object) };
    for index in 0..total {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        if unsafe { same_value(api, value, needle) } {
            return unsafe { (birnam_abi::integer::make)(index as i64) };
        }
    }
    api.nilObj
}

unsafe extern "C" fn vector_copy_from_to(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (birnam_abi::integer::is_integer)(arguments[0]) } == 0
        || unsafe { (birnam_abi::integer::is_integer)(arguments[1]) } == 0
    {
        return api.nilObj;
    }
    let from = unsafe { (birnam_abi::integer::value)(arguments[0]) };
    let to = unsafe { (birnam_abi::integer::value)(arguments[1]) };
    let total = unsafe { birnam_abi::vector::count(self_object) };
    if from < 0 || to < from || to as usize > total {
        return api.nilObj;
    }
    let mut result = unsafe { birnam_abi::vector::empty() };
    for index in from as usize..to as usize {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        result = unsafe { birnam_abi::vector::push(result, value) };
    }
    result
}

unsafe extern "C" fn vector_reversed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let total = unsafe { birnam_abi::vector::count(self_object) };
    let mut result = unsafe { birnam_abi::vector::empty() };
    for index in (0..total).rev() {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        result = unsafe { birnam_abi::vector::push(result, value) };
    }
    result
}

unsafe extern "C" fn vector_sort_by(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(block) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let total = unsafe { birnam_abi::vector::count(self_object) };
    if total < 2 {
        return self_object;
    }
    let roots = SortRootScope::new(&[self_object, block]);

    // Copy the elements and compute each key before sorting. Rust-owned Vec
    // buffers are invisible to the floor collector, so the scoped provider
    // keeps every copied element and callback result live across later sends.
    let mut elements = Vec::with_capacity(total);
    let mut keys = Vec::with_capacity(total);
    let value_selector = runtime().value1 as Oop;
    for index in 0..total {
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        let call_arguments = [value];
        let key = unsafe { (api.send)(block, value_selector, call_arguments.as_ptr(), 1) };
        if key.is_null() {
            return api.nilObj;
        }
        elements.push(value);
        keys.push(key);
        roots.push(value);
        roots.push(key);
    }

    // Stable insertion sort, matching the reference key comparator and
    // preserving input order for equal keys.
    let less_selector = unsafe { intern(api, b"<\0") };
    let mut order: Vec<usize> = (0..total).collect();
    for index in 1..total {
        let candidate = order[index];
        let mut position = index;
        while position > 0 {
            let previous = order[position - 1];
            let comparator_arguments = [keys[previous]];
            let less = unsafe {
                (api.send)(
                    keys[candidate],
                    less_selector,
                    comparator_arguments.as_ptr(),
                    1,
                )
            };
            if less != api.trueObj {
                break;
            }
            order[position] = previous;
            position -= 1;
        }
        order[position] = candidate;
    }

    let mut result = unsafe { birnam_abi::vector::empty() };
    for index in order {
        result = unsafe { birnam_abi::vector::push(result, elements[index]) };
    }
    result
}

unsafe extern "C" fn vector_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let selector = runtime().print_string as Oop;
    let total = unsafe { birnam_abi::vector::count(self_object) };
    let mut output = Vec::from(&b"#["[..]);
    for index in 0..total {
        if index != 0 {
            output.push(b' ');
        }
        let value = unsafe { birnam_abi::vector::at(self_object, index) };
        let printed = unsafe { (api.send)(value, selector, ptr::null(), 0) };
        if printed.is_null() {
            return api.nilObj;
        }
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(printed, &mut length) };
        if bytes.is_null() {
            return api.nilObj;
        }
        // SAFETY: stringBytes returns a stable borrowed extent for this send.
        output.extend_from_slice(unsafe { core::slice::from_raw_parts(bytes, length) });
    }
    output.push(b']');
    // SAFETY: the floor copies this transient byte buffer into a String.
    unsafe { (birnam_abi::string::make)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn vector_comma(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    let mut result = self_object;
    let total = unsafe { birnam_abi::vector::count(other) };
    for index in 0..total {
        let value = unsafe { birnam_abi::vector::at(other, index) };
        result = unsafe { birnam_abi::vector::push(result, value) };
    }
    result
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
    runtime: &Runtime,
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

unsafe fn copy_hnode(api: &IdApi, runtime: &Runtime, source: Oop) -> Oop {
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

unsafe fn empty_map(api: &IdApi, runtime: &Runtime) -> Oop {
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

fn map_mix(mut value: u64) -> u32 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32
}

fn map_fnv1a(bytes: &[u8]) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
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

unsafe fn map_key_hash(api: &IdApi, key: Oop) -> u32 {
    if unsafe { (birnam_abi::integer::is_integer)(key) } != 0 {
        return map_mix(unsafe { (birnam_abi::integer::value)(key) } as u64);
    }
    let value_type = unsafe { (api.vtableOf)(key) };
    if value_type == unsafe { birnam_abi::string::behavior() } {
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(key, &mut length) };
        if length == 0 {
            return map_fnv1a(&[]);
        }
        if bytes.is_null() {
            return 0;
        }
        return map_fnv1a(unsafe { core::slice::from_raw_parts(bytes, length) });
    }
    if value_type == api.symbol_vt {
        let name = unsafe { (api.symbolName)(key) };
        if name.is_null() {
            return 0;
        }
        return map_fnv1a(unsafe { CStr::from_ptr(name) }.to_bytes());
    }
    if value_type == unsafe { birnam_abi::float::behavior() } {
        let value = unsafe { (birnam_abi::float::value)(key) };
        let bits = if value == 0.0 { 0 } else { value.to_bits() };
        return map_mix(bits);
    }
    map_mix(key as usize as u64)
}

unsafe fn map_key_equal(api: &IdApi, left: Oop, right: Oop) -> bool {
    unsafe { same_value(api, left, right) }
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
        if unsafe { map_key_equal(api, stored_key, key) } {
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
        if unsafe { map_key_equal(api, stored_key, key) } {
            return unsafe { hnode_slot(node, index + 1) };
        }
    }
    ptr::null_mut()
}

#[allow(clippy::too_many_arguments)]
unsafe fn map_merge_two(
    api: &IdApi,
    runtime: &Runtime,
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
    runtime: &Runtime,
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
            if unsafe { map_key_equal(api, stored_key, key) } {
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
                    map_key_hash(api, stored_key),
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
            if unsafe { map_key_equal(api, stored_key, key) } {
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

unsafe fn map_collect_values(
    node: Oop,
    mut result: Oop,
    keys: bool,
) -> Oop {
    if node.is_null() {
        return result;
    }
    let header = unsafe { hnode_header_value(node) };
    let slots = hnode_slot_count(header);
    for index in (0..slots).step_by(2) {
        let key = unsafe { hnode_slot(node, index) };
        if key.is_null() {
            let child = unsafe { hnode_slot(node, index + 1) };
            result = unsafe { map_collect_values(child, result, keys) };
        } else {
            let item = if keys {
                key
            } else {
                unsafe { hnode_slot(node, index + 1) }
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
    let header = unsafe { hnode_header_value(node) };
    let slots = hnode_slot_count(header);
    let selector = runtime().value2 as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { hnode_slot(node, index) };
        if key.is_null() {
            unsafe { map_walk_keys_and_values_do(hnode_slot(node, index + 1), block, api) };
        } else {
            let value = unsafe { hnode_slot(node, index + 1) };
            let arguments = [key, value];
            unsafe { (api.send)(block, selector, arguments.as_ptr(), 2) };
        }
    }
}

unsafe fn map_append_print(api: &IdApi, node: Oop, output: &mut Vec<u8>, first: &mut bool) -> bool {
    if node.is_null() {
        return true;
    }
    let header = unsafe { hnode_header_value(node) };
    let slots = hnode_slot_count(header);
    let selector = runtime().print_string as Oop;
    for index in (0..slots).step_by(2) {
        let key = unsafe { hnode_slot(node, index) };
        if key.is_null() {
            if !unsafe { map_append_print(api, hnode_slot(node, index + 1), output, first) } {
                return false;
            }
            continue;
        }
        if !*first {
            output.extend_from_slice(b", ");
        }
        *first = false;
        let value = unsafe { hnode_slot(node, index + 1) };
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

unsafe extern "C" fn vector_new(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { birnam_abi::vector::empty() }
}

unsafe extern "C" fn map_new(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { empty_map(api, runtime()) }
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
    let payload = unsafe { map_payload(self_object) };
    let root = unsafe { (*payload).root };
    MAP_ADDED.store(false, Ordering::Relaxed);
    let replacement =
        unsafe { map_assoc(api, runtime(), root, 0, map_key_hash(api, key), key, value) };
    if replacement == root {
        return self_object;
    }
    let old_count = unsafe { (*payload).count };
    let map = unsafe { (api.alloc)(runtime().map_vt as Oop, size_of::<MapPayload>()) };
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

unsafe extern "C" fn map_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(key) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let value = unsafe {
        map_find(
            (*map_payload(self_object)).root,
            0,
            map_key_hash(api, key),
            key,
            api,
        )
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
        map_find(
            (*map_payload(self_object)).root,
            0,
            map_key_hash(api, key),
            key,
            api,
        )
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
        map_find(
            (*map_payload(self_object)).root,
            0,
            map_key_hash(api, key),
            key,
            api,
        )
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
    unsafe { (birnam_abi::integer::make)((*map_payload(self_object)).count as i64) }
}

unsafe extern "C" fn map_keys(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { birnam_abi::vector::empty() };
    unsafe {
        map_collect_values(
            (*map_payload(self_object)).root,
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
            (*map_payload(self_object)).root,
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
    let result = unsafe { vector_do(values, args, count) };
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
    unsafe { vector_inject(values, args, count) }
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
    unsafe { map_walk_keys_and_values_do((*map_payload(self_object)).root, block, api) };
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
            (*map_payload(self_object)).root,
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
        let (
            collection_vt,
            node_vt,
            vector_vt,
            range_vt,
            map_vt,
            map_proto,
            map_node_vt,
        ) = if reloading {
            let collection_vt = unsafe { import(api, COLLECTION_VT_NAME) };
            let vector = unsafe { birnam_abi::vector::prepare(api_pointer, collection_vt, true) };
            let node_vt = vector.node;
            let vector_vt = vector.vector;
            let range_vt = unsafe { birnam_abi::range::prepare(api_pointer, collection_vt, true) };
            (
                collection_vt, node_vt, vector_vt, range_vt,
                unsafe { import(api, MAP_VT_NAME) },
                unsafe { import(api, MAP_NAME) },
                unsafe { import(api, MAP_NODE_NAME) },
            )
        } else {
            let collection_vt = unsafe { (api.proto)(api.object_vt) };
            let vector = unsafe { birnam_abi::vector::prepare(api_pointer, collection_vt, false) };
            let node_vt = vector.node;
            let vector_vt = vector.vector;
            let range_vt = unsafe { birnam_abi::range::prepare(api_pointer, collection_vt, false) };
            let map_vt = unsafe { (api.proto)(collection_vt) };
            let map_node_vt = unsafe { (api.proto)(api.object_vt) };
            (
                collection_vt,
                node_vt,
                vector_vt,
                range_vt,
                map_vt,
                ptr::null_mut(),
                map_node_vt,
            )
        };
        if [
            collection_vt,
            node_vt,
            vector_vt,
            range_vt,
            map_vt,
            map_node_vt,
        ]
        .iter()
        .any(|value| value.is_null())
        {
            abort("Rust Collection exports were incomplete");
        }
        unsafe { birnam_abi::vector::finish_prototype(api_pointer) };
        unsafe { birnam_abi::range::finish_prototype(api_pointer) };
        let map_proto = if map_proto.is_null() {
            unsafe {
                empty_map(
                    api,
                    &Runtime {
                        api: api_pointer as usize,
                        collection_vt: collection_vt as usize,
                        map_vt: map_vt as usize,
                        map_proto: 0,
                        map_node_vt: map_node_vt as usize,
                        value: 0,
                        value1: 0,
                        value2: 0,
                        print_string: 0,
                    },
                )
            }
        } else {
            map_proto
        };
        Runtime {
            api: api_pointer as usize,
            collection_vt: collection_vt as usize,
            map_vt: map_vt as usize,
            map_proto: map_proto as usize,
            map_node_vt: map_node_vt as usize,
            value: unsafe { intern(api, b"value\0") } as usize,
            value1: unsafe { intern(api, b"value:\0") } as usize,
            value2: unsafe { intern(api, b"value:value:\0") } as usize,
            print_string: unsafe { intern(api, b"printString\0") } as usize,
        }
    });
    SORT_ROOTS_REGISTERED.call_once(|| unsafe {
        (api.gcRegisterRootProvider)(mark_sort_roots);
    });
    unsafe {
        birnam_abi::vector::set_layout(api_pointer);
        birnam_abi::range::set_layout(api_pointer);
        (api.setLayout)(
            runtime.map_node_vt as Oop,
            HNODE_LAYOUT.as_ptr(),
            HNODE_LAYOUT.len(),
            birnam_abi::SLOT_OOP,
        );
        (api.setLayout)(
            runtime.map_vt as Oop,
            MAP_LAYOUT.as_ptr(),
            MAP_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
        birnam_abi::set_vtable_parent(api, birnam_abi::string::behavior(), runtime.collection_vt as Oop);
        (api.method)(birnam_abi::string::behavior(), intern(api, b"split:\0"), string_split);
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"size\0"),
            vector_size,
        );
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"at:\0"),
            vector_at_method,
        );
        (api.method)(birnam_abi::vector::behavior(), intern(api, b"add:\0"), vector_add);
        (api.method)(birnam_abi::vector::behavior(), intern(api, b"do:\0"), vector_do);
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"inject:into:\0"),
            vector_inject,
        );
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"printString\0"),
            vector_print_string,
        );
        (api.method)(birnam_abi::vector::behavior(), intern(api, b",\0"), vector_comma);
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"indexOf:\0"),
            vector_index_of,
        );
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"copyFrom:to:\0"),
            vector_copy_from_to,
        );
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"reversed\0"),
            vector_reversed,
        );
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"sortBy:\0"),
            vector_sort_by,
        );
        (api.method)(birnam_abi::vector::behavior(), intern(api, b"new\0"), vector_new);
        (api.method)(
            birnam_abi::vector::behavior(),
            intern(api, b"clone\0"),
            vector_new,
        );
        birnam_abi::range::install(
            api_pointer,
            runtime.value1 as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
            vector_comma,
        );
        (api.method)(runtime.map_vt as Oop, intern(api, b"new\0"), map_new);
        (api.method)(runtime.map_vt as Oop, intern(api, b"clone\0"), map_new);
        (api.method)(runtime.map_vt as Oop, intern(api, b"at:put:\0"), map_at_put);
        (api.method)(runtime.map_vt as Oop, intern(api, b"at:\0"), map_at);
        (api.method)(
            runtime.map_vt as Oop,
            intern(api, b"at:ifAbsent:\0"),
            map_at_if_absent,
        );
        (api.method)(
            runtime.map_vt as Oop,
            intern(api, b"includesKey:\0"),
            map_includes_key,
        );
        (api.method)(runtime.map_vt as Oop, intern(api, b"size\0"), map_size);
        (api.method)(runtime.map_vt as Oop, intern(api, b"keys\0"), map_keys);
        (api.method)(runtime.map_vt as Oop, intern(api, b"values\0"), map_values);
        (api.method)(runtime.map_vt as Oop, intern(api, b"do:\0"), map_do);
        (api.method)(
            runtime.map_vt as Oop,
            intern(api, b"inject:into:\0"),
            map_inject,
        );
        (api.method)(
            runtime.map_vt as Oop,
            intern(api, b"keysAndValuesDo:\0"),
            map_keys_and_values_do,
        );
        (api.method)(
            runtime.map_vt as Oop,
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
            birnam_abi::set_vtable_owner(api, runtime.map_vt as Oop, runtime.map_proto as Oop);
            let _ = (api.exportObj)(MAP_VT_NAME.as_ptr().cast(), runtime.map_vt as Oop);
            let _ = (api.exportObj)(MAP_NAME.as_ptr().cast(), runtime.map_proto as Oop);
            let _ = (api.exportObj)(MAP_NODE_NAME.as_ptr().cast(), runtime.map_node_vt as Oop);
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

birnam_abi::float_dependency!();

birnam_abi::string_dependency!();

birnam_abi::range_dependency!();

birnam_abi::vector_dependency!();
