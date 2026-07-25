//! Persistent collection storage for the Rust Collection rung.
//!
//! This first R1 slice deliberately keeps the storage kernel independent from
//! the foreign `IdApi` callbacks.  `PersistentVector` has the same durable
//! shape as the Zig vector: a 32-way path-copied trie, a short tail, and an
//! immutable append operation.  The dynamic protocol wrapper is added only
//! after this kernel's sharing and boundary behavior is characterized.

#![allow(clippy::missing_safety_doc)]

use std::array;
use std::sync::Arc;

use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::sync::OnceLock;

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

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    collection_vt: usize,
    vector_vt: usize,
    vector_proto: usize,
    node_vt: usize,
    range_vt: usize,
    range_proto: usize,
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
        let (collection_vt, node_vt, vector_vt, vector_proto, range_vt, range_proto) = if reloading
        {
            (
                unsafe { import(api, COLLECTION_VT_NAME) },
                unsafe { import(api, VECTOR_NODE_NAME) },
                unsafe { import(api, VECTOR_VT_NAME) },
                unsafe { import(api, VECTOR_NAME) },
                unsafe { import(api, RANGE_VT_NAME) },
                unsafe { import(api, RANGE_NAME) },
            )
        } else {
            let collection_vt = unsafe { (api.proto)(api.object_vt) };
            let node_vt = unsafe { (api.proto)(api.object_vt) };
            let vector_vt = unsafe { (api.proto)(collection_vt) };
            let range_vt = unsafe { (api.proto)(collection_vt) };
            (
                collection_vt,
                node_vt,
                vector_vt,
                ptr::null_mut(),
                range_vt,
                ptr::null_mut(),
            )
        };
        if [collection_vt, node_vt, vector_vt, range_vt]
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
        Runtime {
            api: api_pointer as usize,
            collection_vt: collection_vt as usize,
            vector_vt: vector_vt as usize,
            vector_proto: vector_proto as usize,
            node_vt: node_vt as usize,
            range_vt: range_vt as usize,
            range_proto: range_proto as usize,
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
        (api.setParent)(api.string_vt, runtime.collection_vt as Oop);
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
