//! Persistent collection storage for the Rust Collection rung.
//!
//! This first R1 slice deliberately keeps the storage kernel independent from
//! the foreign `IdApi` callbacks.  `PersistentVector` has the same durable
//! shape as the native vector: a 32-way path-copied trie, a short tail, and an
//! immutable append operation.  The dynamic protocol wrapper is added only
//! after this kernel's sharing and boundary behavior is characterized.

#![allow(clippy::missing_safety_doc)]

use std::array;
use std::sync::Arc;

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{CStr, c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::alloc::{Layout, alloc, dealloc};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

const BITS: usize = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;

type Leaf<T> = [Option<T>; WIDTH];
type Branch<T> = [Option<Arc<Node<T>>>; WIDTH];

#[derive(Clone)]
enum Node<T> {
    Leaf(Leaf<T>),
    Branch(Box<Branch<T>>),
}

fn empty_leaf<T>() -> Leaf<T> {
    array::from_fn(|_| None)
}

fn empty_branch<T>() -> Branch<T> {
    array::from_fn(|_| None)
}

fn new_path<T>(level: usize, node: Arc<Node<T>>) -> Arc<Node<T>> {
    if level == 0 {
        return node;
    }
    let mut branch = empty_branch();
    branch[0] = Some(new_path(level - BITS, node));
    Arc::new(Node::Branch(Box::new(branch)))
}

fn copy_branch<T: Clone>(node: &Arc<Node<T>>) -> Branch<T> {
    match node.as_ref() {
        Node::Branch(branch) => branch.as_ref().clone(),
        Node::Leaf(_) => panic!("collection trie expected a branch node"),
    }
}

fn push_tail<T: Clone>(
    old_count: usize,
    level: usize,
    parent: &Arc<Node<T>>,
    tail: Arc<Node<T>>,
) -> Arc<Node<T>> {
    let subindex = ((old_count - 1) >> level) & MASK;
    let mut branch = copy_branch(parent);
    let child = if level == BITS {
        tail
    } else {
        match &branch[subindex] {
            Some(existing) => push_tail(old_count, level - BITS, existing, tail),
            None => new_path(level - BITS, tail),
        }
    };
    branch[subindex] = Some(child);
    Arc::new(Node::Branch(Box::new(branch)))
}

fn array_for<T>(vector: &PersistentVector<T>, index: usize) -> Option<&Leaf<T>> {
    if index >= vector.tail_offset() {
        return Some(vector.tail.as_ref());
    }
    let mut node = vector.root.as_ref();
    let mut level = vector.shift;
    while level > 0 {
        let child = match node {
            Node::Branch(branch) => branch[(index >> level) & MASK].as_deref()?,
            Node::Leaf(_) => return None,
        };
        node = child;
        level -= BITS;
    }
    match node {
        Node::Leaf(leaf) => Some(leaf),
        Node::Branch(_) => None,
    }
}

/// An immutable, persistent vector with a 32-way radix trie and a short tail.
///
/// Values are cloned only along the path copied by `push`; all untouched trie
/// nodes are shared by `Arc`.  This makes old versions safe to retain while a
/// newer version is built, matching Birnam's immutable collection contract.
#[derive(Clone)]
pub struct PersistentVector<T> {
    count: usize,
    shift: usize,
    root: Arc<Node<T>>,
    tail: Arc<Leaf<T>>,
    tail_len: usize,
}

impl<T> Default for PersistentVector<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PersistentVector<T> {
    /// Construct an empty vector.
    pub fn new() -> Self {
        Self {
            count: 0,
            shift: BITS,
            root: Arc::new(Node::Branch(Box::new(empty_branch()))),
            tail: Arc::new(empty_leaf()),
            tail_len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn tail_offset(&self) -> usize {
        self.count - self.tail_len
    }

    /// Return the value at a zero-based index, or `None` when it is outside
    /// the vector's bounds.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.count {
            return None;
        }
        let leaf = array_for(self, index)?;
        leaf[index & MASK].as_ref()
    }
}

impl<T: Clone> PersistentVector<T> {
    /// Append one value, returning a new vector and preserving this version.
    pub fn push(&self, value: T) -> Self {
        let mut next_tail = (*self.tail).clone();
        if self.tail_len < WIDTH {
            next_tail[self.tail_len] = Some(value);
            return Self {
                count: self.count + 1,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::new(next_tail),
                tail_len: self.tail_len + 1,
            };
        }

        let tail_node = Arc::new(Node::Leaf((*self.tail).clone()));
        let (root, shift) = if (self.count >> BITS) > (1usize << self.shift) {
            let mut new_root = empty_branch();
            new_root[0] = Some(Arc::clone(&self.root));
            new_root[1] = Some(new_path(self.shift, tail_node));
            (
                Arc::new(Node::Branch(Box::new(new_root))),
                self.shift + BITS,
            )
        } else {
            (
                push_tail(self.count, self.shift, &self.root, tail_node),
                self.shift,
            )
        };

        next_tail = empty_leaf();
        next_tail[0] = Some(value);
        Self {
            count: self.count + 1,
            shift,
            root,
            tail: Arc::new(next_tail),
            tail_len: 1,
        }
    }

    /// Materialize a copy in index order for protocol adapters and tests.
    pub fn to_vec(&self) -> Vec<T> {
        (0..self.count)
            .filter_map(|index| self.get(index).cloned())
            .collect()
    }
}

// -- IdApi Vector fixture ---------------------------------------------------

const RUST_COLLECTION_IDENTITY: &[u8] = b"rust-collection-r1-vector\0";
const COLLECTION_NAME: &[u8] = b"Collection\0";
const COLLECTION_VT_NAME: &[u8] = b"CollectionVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
const VECTOR_VT_NAME: &[u8] = b"VectorVt\0";
const VECTOR_NODE_NAME: &[u8] = b"VectorNode\0";
const RANGE_NAME: &[u8] = b"Range\0";
const RANGE_VT_NAME: &[u8] = b"RangeVt\0";
const MAP_NAME: &[u8] = b"Map\0";
const MAP_VT_NAME: &[u8] = b"MapVt\0";
const MAP_NODE_NAME: &[u8] = b"MapNode\0";
const STRING_BUILDER_NAME: &[u8] = b"StringBuilder\0";
const STRING_BUILDER_VT_NAME: &[u8] = b"StringBuilderVt\0";

#[repr(C)]
struct VecPayload {
    count: usize,
    shift: usize,
    root: Oop,
    tail: Oop,
    tail_len: usize,
}

#[repr(C)]
struct RangePayload {
    start: i64,
    end: i64,
}

static RANGE_LAYOUT: [u8; 2] = [birnam_abi::SLOT_RAW, birnam_abi::SLOT_RAW];
static MAP_LAYOUT: [u8; 2] = [birnam_abi::SLOT_OOP, birnam_abi::SLOT_RAW];
static HNODE_LAYOUT: [u8; 2] = [birnam_abi::SLOT_RAW, birnam_abi::SLOT_RAW];
static STRING_BUILDER_LAYOUT: [u8; 3] = [
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_RAW,
];

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

#[repr(C)]
struct StringBuilderPayload {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    collection_vt: usize,
    vector_vt: usize,
    vector_proto: usize,
    node_vt: usize,
    range_vt: usize,
    range_proto: usize,
    map_vt: usize,
    map_proto: usize,
    map_node_vt: usize,
    string_builder_vt: usize,
    string_builder_proto: usize,
    value: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static VECTOR_LAYOUT: [u8; 5] = [
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_OOP,
    birnam_abi::SLOT_OOP,
    birnam_abi::SLOT_RAW,
];

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

unsafe fn vec_payload(object: Oop) -> *mut VecPayload {
    if object.is_null() {
        abort("Vector method received a null receiver");
    }
    // SAFETY: Vector instances are allocated with the exact VecPayload layout.
    object.cast::<VecPayload>()
}

unsafe fn node_slots(object: Oop) -> *mut [Oop; WIDTH] {
    if object.is_null() {
        abort("Vector node was null");
    }
    // SAFETY: VectorNode payloads are allocated as WIDTH consecutive OOP slots.
    object.cast::<[Oop; WIDTH]>()
}

unsafe fn new_node(api: &IdApi, node_vt: Oop) -> Oop {
    // SAFETY: the vtable has an all-OOP layout and the requested payload is
    // exactly one vector trie node.
    let node = unsafe { (api.alloc)(node_vt, size_of::<[Oop; WIDTH]>()) };
    if node.is_null() {
        abort("VectorNode allocation failed");
    }
    node
}

unsafe fn copy_node(api: &IdApi, node_vt: Oop, source: Oop) -> Oop {
    let destination = unsafe { new_node(api, node_vt) };
    // SAFETY: both payloads have the exact WIDTH-slot node extent.
    unsafe { (*node_slots(destination)).copy_from_slice(&*node_slots(source)) };
    destination
}

unsafe fn oop_new_path(api: &IdApi, node_vt: Oop, level: usize, node: Oop) -> Oop {
    if level == 0 {
        return node;
    }
    let result = unsafe { new_node(api, node_vt) };
    unsafe { (*node_slots(result))[0] = oop_new_path(api, node_vt, level - BITS, node) };
    result
}

unsafe fn oop_push_tail(
    api: &IdApi,
    node_vt: Oop,
    old_count: usize,
    level: usize,
    parent: Oop,
    tail: Oop,
) -> Oop {
    let subindex = ((old_count - 1) >> level) & MASK;
    let result = unsafe { copy_node(api, node_vt, parent) };
    let child = if level == BITS {
        tail
    } else {
        let existing = unsafe { (*node_slots(parent))[subindex] };
        if existing.is_null() {
            unsafe { oop_new_path(api, node_vt, level - BITS, tail) }
        } else {
            unsafe { oop_push_tail(api, node_vt, old_count, level - BITS, existing, tail) }
        }
    };
    unsafe { (*node_slots(result))[subindex] = child };
    result
}

unsafe fn empty_vector(api: &IdApi, runtime: &Runtime) -> Oop {
    let root = unsafe { new_node(api, runtime.node_vt as Oop) };
    let tail = unsafe { new_node(api, runtime.node_vt as Oop) };
    // SAFETY: the vtable's layout matches VecPayload exactly.
    let vector = unsafe { (api.alloc)(runtime.vector_vt as Oop, size_of::<VecPayload>()) };
    if vector.is_null() {
        abort("Vector allocation failed");
    }
    unsafe {
        *vec_payload(vector) = VecPayload {
            count: 0,
            shift: BITS,
            root,
            tail,
            tail_len: 0,
        };
    }
    vector
}

unsafe fn empty_range(api: &IdApi, runtime: &Runtime, start: i64, end: i64) -> Oop {
    let range = unsafe { (api.alloc)(runtime.range_vt as Oop, size_of::<RangePayload>()) };
    if range.is_null() {
        abort("Range allocation failed");
    }
    unsafe {
        *range.cast::<RangePayload>() = RangePayload { start, end };
    }
    range
}

unsafe fn oop_array_for(vector: *const VecPayload, index: usize) -> Oop {
    if index >= unsafe { (*vector).count } {
        return ptr::null_mut();
    }
    if index >= unsafe { (*vector).count - (*vector).tail_len } {
        return unsafe { (*vector).tail };
    }
    let mut node = unsafe { (*vector).root };
    let mut level = unsafe { (*vector).shift };
    while level > 0 {
        let child = unsafe { (*node_slots(node))[(index >> level) & MASK] };
        if child.is_null() {
            return ptr::null_mut();
        }
        node = child;
        level -= BITS;
    }
    node
}

unsafe fn vector_at(vector: *const VecPayload, index: usize) -> Oop {
    let leaf = unsafe { oop_array_for(vector, index) };
    if leaf.is_null() {
        return ptr::null_mut();
    }
    unsafe { (*node_slots(leaf))[index & MASK] }
}

unsafe fn vector_push(api: &IdApi, runtime: &Runtime, vector: Oop, value: Oop) -> Oop {
    let source = unsafe { vec_payload(vector) };
    let source_count = unsafe { (*source).count };
    let source_shift = unsafe { (*source).shift };
    let source_root = unsafe { (*source).root };
    let source_tail = unsafe { (*source).tail };
    let source_tail_len = unsafe { (*source).tail_len };
    let mut tail = unsafe { copy_node(api, runtime.node_vt as Oop, source_tail) };
    if source_tail_len < WIDTH {
        unsafe { (*node_slots(tail))[source_tail_len] = value };
        let result = unsafe { (api.alloc)(runtime.vector_vt as Oop, size_of::<VecPayload>()) };
        if result.is_null() {
            abort("Vector allocation failed");
        }
        unsafe {
            *vec_payload(result) = VecPayload {
                count: source_count + 1,
                shift: source_shift,
                root: source_root,
                tail,
                tail_len: source_tail_len + 1,
            };
        }
        return result;
    }

    let tail_node = source_tail;
    let root;
    let mut shift = source_shift;
    if (source_count >> BITS) > (1usize << source_shift) {
        root = unsafe { new_node(api, runtime.node_vt as Oop) };
        unsafe {
            (*node_slots(root))[0] = source_root;
            (*node_slots(root))[1] =
                oop_new_path(api, runtime.node_vt as Oop, source_shift, tail_node);
        }
        shift += BITS;
    } else {
        root = unsafe {
            oop_push_tail(
                api,
                runtime.node_vt as Oop,
                source_count,
                source_shift,
                source_root,
                tail_node,
            )
        };
    }

    tail = unsafe { new_node(api, runtime.node_vt as Oop) };
    unsafe { (*node_slots(tail))[0] = value };
    let result = unsafe { (api.alloc)(runtime.vector_vt as Oop, size_of::<VecPayload>()) };
    if result.is_null() {
        abort("Vector allocation failed");
    }
    unsafe {
        *vec_payload(result) = VecPayload {
            count: source_count + 1,
            shift,
            root,
            tail,
            tail_len: 1,
        };
    }
    result
}

unsafe fn same_value(api: &IdApi, left: Oop, right: Oop) -> bool {
    if left == right {
        return true;
    }
    if left.is_null() || right.is_null() {
        return false;
    }
    if unsafe { (api.isSmallInt)(left) } != 0 && unsafe { (api.isSmallInt)(right) } != 0 {
        return unsafe { (api.smallIntValue)(left) == (api.smallIntValue)(right) };
    }
    if unsafe { (api.isFloat)(left) } != 0 && unsafe { (api.isFloat)(right) } != 0 {
        return unsafe { (api.floatValue)(left) == (api.floatValue)(right) };
    }
    if unsafe { (api.vtableOf)(left) } != api.string_vt
        || unsafe { (api.vtableOf)(right) } != api.string_vt
    {
        return false;
    }
    let mut left_len = 0;
    let mut right_len = 0;
    let left_bytes = unsafe { (api.stringBytes)(left, &mut left_len) };
    let right_bytes = unsafe { (api.stringBytes)(right, &mut right_len) };
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
    unsafe { (api.mkSmallInt)((*vec_payload(self_object)).count as i64) }
}

unsafe extern "C" fn vector_at_method(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(index) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(index) } == 0 {
        return api.nilObj;
    }
    let index = unsafe { (api.smallIntValue)(index) };
    if index < 0 {
        return api.nilObj;
    }
    let value = unsafe { vector_at(vec_payload(self_object), index as usize) };
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
    unsafe { vector_push(api, runtime(), self_object, value) }
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
    let total = unsafe { (*vec_payload(self_object)).count };
    for index in 0..total {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
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
    let total = unsafe { (*vec_payload(self_object)).count };
    let mut accumulator = arguments[0];
    for index in 0..total {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
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
    let total = unsafe { (*vec_payload(self_object)).count };
    for index in 0..total {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
        if unsafe { same_value(api, value, needle) } {
            return unsafe { (api.mkSmallInt)(index as i64) };
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
    if unsafe { (api.isSmallInt)(arguments[0]) } == 0
        || unsafe { (api.isSmallInt)(arguments[1]) } == 0
    {
        return api.nilObj;
    }
    let from = unsafe { (api.smallIntValue)(arguments[0]) };
    let to = unsafe { (api.smallIntValue)(arguments[1]) };
    let total = unsafe { (*vec_payload(self_object)).count };
    if from < 0 || to < from || to as usize > total {
        return api.nilObj;
    }
    let mut result = unsafe { empty_vector(api, runtime()) };
    for index in from as usize..to as usize {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
        result = unsafe { vector_push(api, runtime(), result, value) };
    }
    result
}

unsafe extern "C" fn vector_reversed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let total = unsafe { (*vec_payload(self_object)).count };
    let mut result = unsafe { empty_vector(api, runtime()) };
    for index in (0..total).rev() {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
        result = unsafe { vector_push(api, runtime(), result, value) };
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
    let total = unsafe { (*vec_payload(self_object)).count };
    if total < 2 {
        return self_object;
    }

    // Copy the elements and compute each key before sorting. No pointer into
    // the live vector or Rust-owned buffer is held across a foreign send.
    let mut elements = Vec::with_capacity(total);
    let mut keys = Vec::with_capacity(total);
    let value_selector = runtime().value1 as Oop;
    for index in 0..total {
        let value = unsafe { vector_at(vec_payload(self_object), index) };
        let call_arguments = [value];
        let key = unsafe { (api.send)(block, value_selector, call_arguments.as_ptr(), 1) };
        if key.is_null() {
            return api.nilObj;
        }
        elements.push(value);
        keys.push(key);
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

    let mut result = unsafe { empty_vector(api, runtime()) };
    for index in order {
        result = unsafe { vector_push(api, runtime(), result, elements[index]) };
    }
    result
}

unsafe extern "C" fn vector_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let selector = runtime().print_string as Oop;
    let total = unsafe { (*vec_payload(self_object)).count };
    let mut output = Vec::from(&b"#["[..]);
    for index in 0..total {
        if index != 0 {
            output.push(b' ');
        }
        let value = unsafe { vector_at(vec_payload(self_object), index) };
        let printed = unsafe { (api.send)(value, selector, ptr::null(), 0) };
        if printed.is_null() {
            return api.nilObj;
        }
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(printed, &mut length) };
        if bytes.is_null() {
            return api.nilObj;
        }
        // SAFETY: stringBytes returns a stable borrowed extent for this send.
        output.extend_from_slice(unsafe { core::slice::from_raw_parts(bytes, length) });
    }
    output.push(b']');
    // SAFETY: the floor copies this transient byte buffer into a String.
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn vector_comma(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(other) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(other) } != runtime().vector_vt as Oop {
        return api.nilObj;
    }
    let mut result = self_object;
    let total = unsafe { (*vec_payload(other)).count };
    for index in 0..total {
        let value = unsafe { vector_at(vec_payload(other), index) };
        result = unsafe { vector_push(api, runtime(), result, value) };
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
    if unsafe { (api.isSmallInt)(key) } != 0 {
        return map_mix(unsafe { (api.smallIntValue)(key) } as u64);
    }
    let value_type = unsafe { (api.vtableOf)(key) };
    if value_type == api.string_vt {
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(key, &mut length) };
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
    if value_type == api.float_vt {
        let value = unsafe { (api.floatValue)(key) };
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
    api: &IdApi,
    runtime: &Runtime,
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
            result = unsafe { map_collect_values(api, runtime, child, result, keys) };
        } else {
            let item = if keys {
                key
            } else {
                unsafe { hnode_slot(node, index + 1) }
            };
            result = unsafe { vector_push(api, runtime, result, item) };
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
            let bytes = unsafe { (api.stringBytes)(printed, &mut length) };
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
    unsafe { empty_vector(api, runtime()) }
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
    unsafe { (api.mkSmallInt)((*map_payload(self_object)).count as i64) }
}

unsafe extern "C" fn map_keys(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let result = unsafe { empty_vector(api, runtime()) };
    unsafe {
        map_collect_values(
            api,
            runtime(),
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
    let result = unsafe { empty_vector(api, runtime()) };
    unsafe {
        map_collect_values(
            api,
            runtime(),
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
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
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
    if unsafe { (api.vtableOf)(value) } != api.string_vt {
        value = unsafe { (api.send)(value, runtime().print_string as Oop, ptr::null(), 0) };
    }
    if value.is_null() || unsafe { (api.vtableOf)(value) } != api.string_vt {
        return self_object;
    }
    let mut length = 0;
    let bytes = unsafe { (api.stringBytes)(value, &mut length) };
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
    if unsafe { (api.isSmallInt)(value) } == 0 {
        return self_object;
    }
    let payload = unsafe { string_builder_payload(self_object) };
    unsafe { string_builder_ensure(payload, 1) };
    let byte = unsafe { (api.smallIntValue)(value) } as u8;
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
    unsafe { (api.mkSmallInt)((*string_builder_payload(self_object)).len as i64) }
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
        return unsafe { (api.mkString)(b"".as_ptr(), 0) };
    }
    let bytes = unsafe { (*payload).ptr };
    if bytes.is_null() {
        return api.nilObj;
    }
    unsafe { (api.mkString)(bytes, length) }
}

unsafe fn string_copy(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
    if object.is_null() || unsafe { (api.vtableOf)(object) } != api.string_vt {
        return None;
    }
    let mut length = 0;
    let bytes = unsafe { (api.stringBytes)(object, &mut length) };
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
    if unsafe { (api.vtableOf)(other) } != api.string_vt {
        other = unsafe { (api.send)(other, runtime().print_string as Oop, ptr::null(), 0) };
    }
    let Some(left) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { string_copy(api, other) }) else {
        return unsafe { (api.mkString)(left.as_ptr(), left.len()) };
    };
    let mut output = Vec::with_capacity(left.len() + right.len());
    output.extend_from_slice(&left);
    output.extend_from_slice(&right);
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
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
    unsafe { (api.mkSmallInt)(bytes.len() as i64) }
}

unsafe extern "C" fn string_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(index) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(index) } == 0 {
        return api.nilObj;
    }
    let index = unsafe { (api.smallIntValue)(index) };
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if index < 0 || index as usize >= bytes.len() {
        return api.nilObj;
    }
    let byte = [bytes[index as usize]];
    unsafe { (api.mkString)(byte.as_ptr(), 1) }
}

unsafe extern "C" fn string_byte_at(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(index) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(index) } == 0 {
        return api.nilObj;
    }
    let index = unsafe { (api.smallIntValue)(index) };
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if index < 0 || index as usize >= bytes.len() {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)(i64::from(bytes[index as usize])) }
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
    if unsafe { (api.isSmallInt)(from) } == 0 || unsafe { (api.isSmallInt)(to) } == 0 {
        return api.nilObj;
    }
    let from = unsafe { (api.smallIntValue)(from) };
    let to = unsafe { (api.smallIntValue)(to) };
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    if from < 0 || to < from || to as usize > bytes.len() {
        return api.nilObj;
    }
    unsafe {
        (api.mkString)(
            bytes[from as usize..to as usize].as_ptr(),
            (to - from) as usize,
        )
    }
}

unsafe extern "C" fn string_split(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(separator) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(separator) } != api.string_vt {
        let result = unsafe { empty_vector(api, runtime()) };
        return unsafe { vector_push(api, runtime(), result, self_object) };
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(separator) = (unsafe { string_copy(api, separator) }) else {
        return api.nilObj;
    };
    let mut result = unsafe { empty_vector(api, runtime()) };
    if separator.is_empty() {
        let item = unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) };
        return unsafe { vector_push(api, runtime(), result, item) };
    }
    let mut start = 0usize;
    let mut cursor = 0usize;
    while cursor + separator.len() <= bytes.len() {
        if bytes[cursor..cursor + separator.len()] == separator[..] {
            let item = unsafe { (api.mkString)(bytes[start..cursor].as_ptr(), cursor - start) };
            result = unsafe { vector_push(api, runtime(), result, item) };
            cursor += separator.len();
            start = cursor;
        } else {
            cursor += 1;
        }
    }
    let item = unsafe { (api.mkString)(bytes[start..].as_ptr(), bytes.len() - start) };
    unsafe { vector_push(api, runtime(), result, item) }
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
    unsafe { (api.mkString)(bytes[start..end].as_ptr(), end - start) }
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
        return unsafe { (api.mkSmallInt)(0) };
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle.as_slice())
        .map_or(api.nilObj, |index| unsafe {
            (api.mkSmallInt)(index as i64)
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
    unsafe { (api.mkSmallInt)(depth) }
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
        return unsafe { (api.mkString)(haystack.as_ptr(), haystack.len()) };
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
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
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
        Some(value) => unsafe { (api.mkSmallInt)(value) },
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
        return unsafe { (api.mkSmallInt)(value) };
    }
    match text.parse::<f64>() {
        Ok(value) => unsafe { (api.mkFloat)(value) },
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

unsafe extern "C" fn object_is_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.falseObj } else { api.nilObj }
}

unsafe extern "C" fn object_not_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.trueObj } else { api.nilObj }
}

unsafe extern "C" fn nil_is_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.trueObj } else { api.nilObj }
}

unsafe extern "C" fn nil_not_nil(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 { api.falseObj } else { api.nilObj }
}

unsafe extern "C" fn nil_print_string(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count == 0 {
        unsafe { (api.mkString)(b"nil".as_ptr(), 3) }
    } else {
        api.nilObj
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
        return unsafe { (api.mkSmallInt)(0) };
    }
    let size = i128::from(end) - i128::from(start) + 1;
    if size > i128::from(i64::MAX) {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)(size as i64) }
}

unsafe extern "C" fn range_includes(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(value) } == 0 {
        return api.falseObj;
    }
    let value = unsafe { (api.smallIntValue)(value) };
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
    if unsafe { (api.isSmallInt)(index) } == 0 {
        return api.nilObj;
    }
    let index = unsafe { (api.smallIntValue)(index) };
    let (start, end) = unsafe { range_bounds(self_object) };
    let value = i128::from(start) + i128::from(index);
    if index < 0 || start > end || value > i128::from(end) || value > i128::from(i64::MAX) {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)(value as i64) }
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
        let item = unsafe { (api.mkSmallInt)(value as i64) };
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
        let item = unsafe { (api.mkSmallInt)(value as i64) };
        let call_arguments = [accumulator, item];
        accumulator = unsafe { (api.send)(arguments[1], selector, call_arguments.as_ptr(), 2) };
        if accumulator.is_null() {
            return api.nilObj;
        }
        value += 1;
    }
    accumulator
}

unsafe extern "C" fn range_reversed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut result = unsafe { empty_vector(api, runtime()) };
    let mut value = i128::from(end);
    let start = i128::from(start);
    while value >= start {
        result = unsafe { vector_push(api, runtime(), result, (api.mkSmallInt)(value as i64)) };
        value -= 1;
    }
    result
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
        let item = unsafe { (api.mkSmallInt)(value as i64) };
        let printed = unsafe { (api.send)(item, selector, ptr::null(), 0) };
        if printed.is_null() {
            return api.nilObj;
        }
        let mut length = 0;
        let bytes = unsafe { (api.stringBytes)(printed, &mut length) };
        if bytes.is_null() {
            return api.nilObj;
        }
        output.extend_from_slice(unsafe { core::slice::from_raw_parts(bytes, length) });
        value += 1;
    }
    output.push(b']');
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn range_comma(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(other) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(other) } != runtime().vector_vt as Oop {
        return api.nilObj;
    }
    let (start, end) = unsafe { range_bounds(self_object) };
    let mut result = unsafe { empty_vector(api, runtime()) };
    let mut value = i128::from(start);
    let end = i128::from(end);
    while value <= end {
        result = unsafe { vector_push(api, runtime(), result, (api.mkSmallInt)(value as i64)) };
        value += 1;
    }
    unsafe { vector_comma(result, args, count) }
}

unsafe extern "C" fn integer_to(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 || unsafe { (api.isSmallInt)(self_object) } == 0 {
        return api.nilObj;
    }
    let Some(end) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(end) } == 0 {
        return api.nilObj;
    }
    unsafe {
        empty_range(
            api,
            runtime(),
            (api.smallIntValue)(self_object),
            (api.smallIntValue)(end),
        )
    }
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
            vector_proto,
            range_vt,
            range_proto,
            map_vt,
            map_proto,
            map_node_vt,
        ) = if reloading {
            (
                unsafe { import(api, COLLECTION_VT_NAME) },
                unsafe { import(api, VECTOR_NODE_NAME) },
                unsafe { import(api, VECTOR_VT_NAME) },
                unsafe { import(api, VECTOR_NAME) },
                unsafe { import(api, RANGE_VT_NAME) },
                unsafe { import(api, RANGE_NAME) },
                unsafe { import(api, MAP_VT_NAME) },
                unsafe { import(api, MAP_NAME) },
                unsafe { import(api, MAP_NODE_NAME) },
            )
        } else {
            let collection_vt = unsafe { (api.proto)(api.object_vt) };
            let node_vt = unsafe { (api.proto)(api.object_vt) };
            let vector_vt = unsafe { (api.proto)(collection_vt) };
            let range_vt = unsafe { (api.proto)(collection_vt) };
            let map_vt = unsafe { (api.proto)(collection_vt) };
            let map_node_vt = unsafe { (api.proto)(api.object_vt) };
            (
                collection_vt,
                node_vt,
                vector_vt,
                ptr::null_mut(),
                range_vt,
                ptr::null_mut(),
                map_vt,
                ptr::null_mut(),
                map_node_vt,
            )
        };
        let (string_builder_vt, string_builder_proto) =
            if unsafe { (api.isExported)(STRING_BUILDER_NAME.as_ptr().cast()) } != 0 {
                (unsafe { import(api, STRING_BUILDER_VT_NAME) }, unsafe {
                    import(api, STRING_BUILDER_NAME)
                })
            } else {
                (unsafe { (api.proto)(api.object_vt) }, ptr::null_mut())
            };
        if [
            collection_vt,
            node_vt,
            vector_vt,
            range_vt,
            map_vt,
            map_node_vt,
            string_builder_vt,
        ]
        .iter()
        .any(|value| value.is_null())
        {
            abort("Rust Collection exports were incomplete");
        }
        let vector_proto = if vector_proto.is_null() {
            unsafe {
                empty_vector(
                    api,
                    &Runtime {
                        api: api_pointer as usize,
                        collection_vt: collection_vt as usize,
                        vector_vt: vector_vt as usize,
                        vector_proto: 0,
                        node_vt: node_vt as usize,
                        range_vt: range_vt as usize,
                        range_proto: 0,
                        map_vt: map_vt as usize,
                        map_proto: 0,
                        map_node_vt: map_node_vt as usize,
                        string_builder_vt: string_builder_vt as usize,
                        string_builder_proto: 0,
                        value: 0,
                        value1: 0,
                        value2: 0,
                        print_string: 0,
                    },
                )
            }
        } else {
            vector_proto
        };
        let range_proto = if range_proto.is_null() {
            unsafe {
                empty_range(
                    api,
                    &Runtime {
                        api: api_pointer as usize,
                        collection_vt: collection_vt as usize,
                        vector_vt: vector_vt as usize,
                        vector_proto: vector_proto as usize,
                        node_vt: node_vt as usize,
                        range_vt: range_vt as usize,
                        range_proto: 0,
                        map_vt: map_vt as usize,
                        map_proto: 0,
                        map_node_vt: map_node_vt as usize,
                        string_builder_vt: string_builder_vt as usize,
                        string_builder_proto: 0,
                        value: 0,
                        value1: 0,
                        value2: 0,
                        print_string: 0,
                    },
                    0,
                    0,
                )
            }
        } else {
            range_proto
        };
        let map_proto = if map_proto.is_null() {
            unsafe {
                empty_map(
                    api,
                    &Runtime {
                        api: api_pointer as usize,
                        collection_vt: collection_vt as usize,
                        vector_vt: vector_vt as usize,
                        vector_proto: vector_proto as usize,
                        node_vt: node_vt as usize,
                        range_vt: range_vt as usize,
                        range_proto: range_proto as usize,
                        map_vt: map_vt as usize,
                        map_proto: 0,
                        map_node_vt: map_node_vt as usize,
                        string_builder_vt: string_builder_vt as usize,
                        string_builder_proto: 0,
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
        let string_builder_proto = if string_builder_proto.is_null() {
            unsafe {
                empty_string_builder(
                    api,
                    &Runtime {
                        api: api_pointer as usize,
                        collection_vt: collection_vt as usize,
                        vector_vt: vector_vt as usize,
                        vector_proto: vector_proto as usize,
                        node_vt: node_vt as usize,
                        range_vt: range_vt as usize,
                        range_proto: range_proto as usize,
                        map_vt: map_vt as usize,
                        map_proto: map_proto as usize,
                        map_node_vt: map_node_vt as usize,
                        string_builder_vt: string_builder_vt as usize,
                        string_builder_proto: 0,
                        value: 0,
                        value1: 0,
                        value2: 0,
                        print_string: 0,
                    },
                )
            }
        } else {
            string_builder_proto
        };
        Runtime {
            api: api_pointer as usize,
            collection_vt: collection_vt as usize,
            vector_vt: vector_vt as usize,
            vector_proto: vector_proto as usize,
            node_vt: node_vt as usize,
            range_vt: range_vt as usize,
            range_proto: range_proto as usize,
            map_vt: map_vt as usize,
            map_proto: map_proto as usize,
            map_node_vt: map_node_vt as usize,
            string_builder_vt: string_builder_vt as usize,
            string_builder_proto: string_builder_proto as usize,
            value: unsafe { intern(api, b"value\0") } as usize,
            value1: unsafe { intern(api, b"value:\0") } as usize,
            value2: unsafe { intern(api, b"value:value:\0") } as usize,
            print_string: unsafe { intern(api, b"printString\0") } as usize,
        }
    });
    unsafe {
        (api.setLayout)(runtime.node_vt as Oop, ptr::null(), 0, birnam_abi::SLOT_OOP);
        (api.setLayout)(
            runtime.vector_vt as Oop,
            VECTOR_LAYOUT.as_ptr(),
            VECTOR_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
        (api.setLayout)(
            runtime.range_vt as Oop,
            RANGE_LAYOUT.as_ptr(),
            RANGE_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
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
        (api.setLayout)(
            runtime.string_builder_vt as Oop,
            STRING_BUILDER_LAYOUT.as_ptr(),
            STRING_BUILDER_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
        (api.setParent)(api.string_vt, runtime.collection_vt as Oop);
        (api.method)(api.object_vt, intern(api, b"isNil\0"), object_is_nil);
        (api.method)(api.object_vt, intern(api, b"notNil\0"), object_not_nil);
        let nil_vt = (api.vtableOf)(api.nilObj);
        (api.method)(nil_vt, intern(api, b"isNil\0"), nil_is_nil);
        (api.method)(nil_vt, intern(api, b"notNil\0"), nil_not_nil);
        (api.method)(nil_vt, intern(api, b"printString\0"), nil_print_string);
        (api.method)(api.string_vt, intern(api, b",\0"), string_comma);
        (api.method)(
            api.string_vt,
            intern(api, b"printString\0"),
            string_print_string,
        );
        (api.method)(api.string_vt, intern(api, b"=\0"), string_equals);
        (api.method)(api.string_vt, intern(api, b"size\0"), string_size);
        (api.method)(api.string_vt, intern(api, b"at:\0"), string_at);
        (api.method)(api.string_vt, intern(api, b"byteAt:\0"), string_byte_at);
        (api.method)(
            api.string_vt,
            intern(api, b"copyFrom:to:\0"),
            string_copy_from_to,
        );
        (api.method)(api.string_vt, intern(api, b"split:\0"), string_split);
        (api.method)(api.string_vt, intern(api, b"trimmed\0"), string_trimmed);
        (api.method)(api.string_vt, intern(api, b"indexOf:\0"), string_index_of);
        (api.method)(
            api.string_vt,
            intern(api, b"bracketDepth\0"),
            string_bracket_depth,
        );
        (api.method)(
            api.string_vt,
            intern(api, b"replace:with:\0"),
            string_replace_with,
        );
        (api.method)(api.string_vt, intern(api, b"asSymbol\0"), string_as_symbol);
        (api.method)(
            api.string_vt,
            intern(api, b"asInteger\0"),
            string_as_integer,
        );
        (api.method)(api.string_vt, intern(api, b"asNumber\0"), string_as_number);
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"size\0"),
            vector_size,
        );
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"at:\0"),
            vector_at_method,
        );
        (api.method)(runtime.vector_vt as Oop, intern(api, b"add:\0"), vector_add);
        (api.method)(runtime.vector_vt as Oop, intern(api, b"do:\0"), vector_do);
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"inject:into:\0"),
            vector_inject,
        );
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"printString\0"),
            vector_print_string,
        );
        (api.method)(runtime.vector_vt as Oop, intern(api, b",\0"), vector_comma);
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"indexOf:\0"),
            vector_index_of,
        );
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"copyFrom:to:\0"),
            vector_copy_from_to,
        );
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"reversed\0"),
            vector_reversed,
        );
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"sortBy:\0"),
            vector_sort_by,
        );
        (api.method)(runtime.vector_vt as Oop, intern(api, b"new\0"), vector_new);
        (api.method)(
            runtime.vector_vt as Oop,
            intern(api, b"clone\0"),
            vector_new,
        );
        (api.method)(runtime.range_vt as Oop, intern(api, b"size\0"), range_size);
        (api.method)(
            runtime.range_vt as Oop,
            intern(api, b"includes:\0"),
            range_includes,
        );
        (api.method)(runtime.range_vt as Oop, intern(api, b"do:\0"), range_do);
        (api.method)(
            runtime.range_vt as Oop,
            intern(api, b"inject:into:\0"),
            range_inject,
        );
        (api.method)(runtime.range_vt as Oop, intern(api, b"at:\0"), range_at);
        (api.method)(
            runtime.range_vt as Oop,
            intern(api, b"reversed\0"),
            range_reversed,
        );
        (api.method)(
            runtime.range_vt as Oop,
            intern(api, b"printString\0"),
            range_print_string,
        );
        (api.method)(runtime.range_vt as Oop, intern(api, b",\0"), range_comma);
        (api.method)(api.smallint_vt, intern(api, b"to:\0"), integer_to);
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
    if unsafe { (api.isExported)(VECTOR_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            (api.setOwner)(runtime.vector_vt as Oop, runtime.vector_proto as Oop);
            let _ = (api.exportObj)(
                COLLECTION_VT_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            let _ = (api.exportObj)(
                COLLECTION_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            let _ = (api.exportObj)(VECTOR_VT_NAME.as_ptr().cast(), runtime.vector_vt as Oop);
            let _ = (api.exportObj)(VECTOR_NAME.as_ptr().cast(), runtime.vector_proto as Oop);
            let _ = (api.exportObj)(VECTOR_NODE_NAME.as_ptr().cast(), runtime.node_vt as Oop);
            (api.setOwner)(runtime.range_vt as Oop, runtime.range_proto as Oop);
            let _ = (api.exportObj)(RANGE_VT_NAME.as_ptr().cast(), runtime.range_vt as Oop);
            let _ = (api.exportObj)(RANGE_NAME.as_ptr().cast(), runtime.range_proto as Oop);
            (api.setOwner)(runtime.map_vt as Oop, runtime.map_proto as Oop);
            let _ = (api.exportObj)(MAP_VT_NAME.as_ptr().cast(), runtime.map_vt as Oop);
            let _ = (api.exportObj)(MAP_NAME.as_ptr().cast(), runtime.map_proto as Oop);
            let _ = (api.exportObj)(MAP_NODE_NAME.as_ptr().cast(), runtime.map_node_vt as Oop);
            (api.setOwner)(
                runtime.string_builder_vt as Oop,
                runtime.string_builder_proto as Oop,
            );
            let _ = (api.exportObj)(
                STRING_BUILDER_VT_NAME.as_ptr().cast(),
                runtime.string_builder_vt as Oop,
            );
            let _ = (api.exportObj)(
                STRING_BUILDER_NAME.as_ptr().cast(),
                runtime.string_builder_proto as Oop,
            );
        }
    }
}

/// Identifies this explicit Rust Collection vector fixture.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_collection_rust_identity() -> *const c_char {
    RUST_COLLECTION_IDENTITY.as_ptr().cast()
}

/// Initialize the Rust Collection vector protocol against the process floor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void) -> *mut c_void {
    let _ = state;
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{PersistentVector, WIDTH};
    use std::sync::Arc;

    #[test]
    fn empty_and_bounds_are_safe() {
        let vector = PersistentVector::<usize>::new();
        assert!(vector.is_empty());
        assert_eq!(vector.get(0), None);
        assert_eq!(vector.to_vec(), Vec::<usize>::new());
    }

    #[test]
    fn append_is_persistent_and_shares_the_root_while_tail_has_room() {
        let first = PersistentVector::new().push(10).push(20);
        let second = first.push(30);
        assert_eq!(first.to_vec(), vec![10, 20]);
        assert_eq!(second.to_vec(), vec![10, 20, 30]);
        assert_eq!(first.get(1), Some(&20));
        assert!(Arc::ptr_eq(&first.root, &second.root));
    }

    #[test]
    fn full_tail_moves_into_the_trie_without_mutating_the_old_version() {
        let mut first = PersistentVector::new();
        for value in 0..WIDTH {
            first = first.push(value);
        }
        let second = first.push(WIDTH);
        assert_eq!(first.len(), WIDTH);
        assert_eq!(first.get(WIDTH), None);
        assert_eq!(second.len(), WIDTH + 1);
        assert_eq!(second.get(0), Some(&0));
        assert_eq!(second.get(WIDTH), Some(&WIDTH));
        assert_eq!(first.tail_len, WIDTH);
        assert_ne!(first.tail.as_ptr(), second.tail.as_ptr());
    }

    #[test]
    fn deep_growth_preserves_every_index() {
        let mut vector = PersistentVector::new();
        for value in 0..1_057usize {
            vector = vector.push(value);
        }
        assert_eq!(vector.len(), 1_057);
        for index in [0, 1, 31, 32, 33, 1_023, 1_024, 1_056] {
            assert_eq!(vector.get(index), Some(&index));
        }
        assert_eq!(vector.to_vec().len(), 1_057);
    }
}
