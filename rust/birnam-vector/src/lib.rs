//! Vector representation, canonical identities, methods and scoped sorting roots.
#![allow(clippy::missing_safety_doc)]
mod kernel;
use birnam_abi::vector::{ApiV4, BehaviorsV1};
use birnam_abi::{IdApi, MarkFn, Method, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
pub use kernel::PersistentVector;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};
const BITS: usize = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;
const VECTOR_NAME: &[u8] = b"Vector\0";
const VECTOR_VT_NAME: &[u8] = b"VectorVt\0";
const VECTOR_NODE_NAME: &[u8] = b"VectorNode\0";
#[repr(C)]
struct VecPayload {
    count: usize,
    shift: usize,
    root: Oop,
    tail: Oop,
    tail_len: usize,
}

static VECTOR_LAYOUT: [u8; 5] = [
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_RAW,
    birnam_abi::SLOT_OOP,
    birnam_abi::SLOT_OOP,
    birnam_abi::SLOT_RAW,
];

const _: () = {
    assert!(size_of::<VecPayload>() == 40);
    assert!(align_of::<VecPayload>() == 8);
    assert!(core::mem::offset_of!(VecPayload, root) == 16);
    assert!(core::mem::offset_of!(VecPayload, tail) == 24);
    assert!(core::mem::offset_of!(VecPayload, tail_len) == 32);
};
struct Family {
    api: usize,
    node_vt: usize,
    vector_vt: usize,
}
static FAMILY: OnceLock<Family> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
static IDENTITY_ROOTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
static READY: AtomicBool = AtomicBool::new(false);
fn abort(message: &str) -> ! {
    eprintln!("birnam Vector: {message}");
    std::process::abort();
}
fn family() -> &'static Family {
    FAMILY
        .get()
        .unwrap_or_else(|| abort("Vector family was not prepared"))
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    for root in &IDENTITY_ROOTS {
        let object = root.load(Ordering::Acquire) as Oop;
        if !object.is_null() {
            unsafe { mark(object) };
        }
    }
    if let Some(runtime) = RUNTIME.get() {
        for value in [runtime.value1, runtime.value2, runtime.print_string] {
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
unsafe extern "C" fn prepare(api_pointer: *mut IdApi, imports: *const birnam_abi::namespace_import::ApiV1, parent: Oop, adopting: i32) -> BehaviorsV1 {
    if !READY.load(Ordering::Acquire) || api_pointer.is_null() {
        abort("invalid Vector preparation");
    }
    let api = unsafe { &*api_pointer };
    ROOTS_REGISTERED.call_once(|| unsafe { (api.gcRegisterRootProvider)(mark_roots) });
    let state = FAMILY.get_or_init(|| {
        let node = if adopting != 0 {
            unsafe { birnam_abi::namespace_import::import(imports, VECTOR_NODE_NAME.as_ptr().cast()) }
        } else {
            unsafe { (api.proto)(api.object_vt) }
        };
        IDENTITY_ROOTS[0].store(node as usize, Ordering::Release);
        let vector = if adopting != 0 {
            unsafe { birnam_abi::namespace_import::import(imports, VECTOR_VT_NAME.as_ptr().cast()) }
        } else {
            unsafe { (api.proto)(parent) }
        };
        IDENTITY_ROOTS[1].store(vector as usize, Ordering::Release);
        let prototype = if adopting != 0 {
            unsafe { birnam_abi::namespace_import::import(imports, VECTOR_NAME.as_ptr().cast()) }
        } else {
            ptr::null_mut()
        };
        IDENTITY_ROOTS[2].store(prototype as usize, Ordering::Release);
        Family {
            api: api_pointer as usize,
            node_vt: node as usize,
            vector_vt: vector as usize,
        }
    });
    BehaviorsV1 {
        node: state.node_vt as Oop,
        vector: state.vector_vt as Oop,
    }
}
unsafe extern "C" fn finish_prototype(api_pointer: *mut IdApi) {
    if IDENTITY_ROOTS[2].load(Ordering::Acquire) == 0 {
        let prototype = unsafe { empty_vector(&*api_pointer, family()) };
        IDENTITY_ROOTS[2].store(prototype as usize, Ordering::Release);
    }
}
unsafe extern "C" fn set_layout(api_pointer: *mut IdApi) {
    let api = unsafe { &*api_pointer };
    unsafe {
        (api.setLayout)(
            family().node_vt as Oop,
            ptr::null(),
            0,
            birnam_abi::SLOT_OOP,
        );
        (api.setLayout)(
            family().vector_vt as Oop,
            VECTOR_LAYOUT.as_ptr(),
            VECTOR_LAYOUT.len(),
            birnam_abi::SLOT_RAW,
        );
    }
}
unsafe extern "C" fn set_owner(api_pointer: *mut IdApi) {
    unsafe {
        birnam_abi::set_vtable_owner(
            &*api_pointer,
            family().vector_vt as Oop,
            IDENTITY_ROOTS[2].load(Ordering::Acquire) as Oop,
        )
    };
}
unsafe extern "C" fn publish(_api_pointer: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1) {
    unsafe {
        birnam_abi::namespace_export::export(exports, VECTOR_VT_NAME.as_ptr().cast(), family().vector_vt as Oop);
        birnam_abi::namespace_export::export(exports,
            VECTOR_NAME.as_ptr().cast(),
            IDENTITY_ROOTS[2].load(Ordering::Acquire) as Oop,
        );
        birnam_abi::namespace_export::export(exports, VECTOR_NODE_NAME.as_ptr().cast(), family().node_vt as Oop);
    }
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

unsafe fn empty_vector(api: &IdApi, runtime: &Family) -> Oop {
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

unsafe fn vector_push(api: &IdApi, runtime: &Family, vector: Oop, value: Oop) -> Oop {
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

unsafe extern "C" fn empty() -> Oop {
    let state = family();
    unsafe { empty_vector(&*(state.api as *const IdApi), state) }
}
unsafe extern "C" fn push(vector: Oop, value: Oop) -> Oop {
    let state = family();
    unsafe { vector_push(&*(state.api as *const IdApi), state, vector, value) }
}
unsafe extern "C" fn count(vector: Oop) -> usize {
    unsafe { (*vec_payload(vector)).count }
}
unsafe extern "C" fn at(vector: Oop, index: usize) -> Oop {
    unsafe { vector_at(vec_payload(vector), index) }
}
unsafe extern "C" fn behavior() -> Oop {
    family().vector_vt as Oop
}
struct Runtime {
    api: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("Vector methods were not installed"))
}
unsafe fn api_ref() -> &'static IdApi {
    // The first Collection-selected table remains live for the process lifetime.
    unsafe { &*(runtime().api as *const IdApi) }
}
unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
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

unsafe extern "C" fn vector_size(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)(self::count(self_object) as i64) }
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
    let value = unsafe { self::at(self_object, index as usize) };
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
    unsafe { self::push(self_object, value) }
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
    let total = unsafe { self::count(self_object) };
    for index in 0..total {
        let value = unsafe { self::at(self_object, index) };
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
    let total = unsafe { self::count(self_object) };
    let mut accumulator = arguments[0];
    for index in 0..total {
        let value = unsafe { self::at(self_object, index) };
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
    let total = unsafe { self::count(self_object) };
    for index in 0..total {
        let value = unsafe { self::at(self_object, index) };
        if unsafe { birnam_abi::collection_value::same_value(api, value, needle) } {
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
    let total = unsafe { self::count(self_object) };
    if from < 0 || to < from || to as usize > total {
        return api.nilObj;
    }
    let mut result = unsafe { self::empty() };
    for index in from as usize..to as usize {
        let value = unsafe { self::at(self_object, index) };
        result = unsafe { self::push(result, value) };
    }
    result
}

unsafe extern "C" fn vector_reversed(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let total = unsafe { self::count(self_object) };
    let mut result = unsafe { self::empty() };
    for index in (0..total).rev() {
        let value = unsafe { self::at(self_object, index) };
        result = unsafe { self::push(result, value) };
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
    let total = unsafe { self::count(self_object) };
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
        let value = unsafe { self::at(self_object, index) };
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

    let mut result = unsafe { self::empty() };
    for index in order {
        result = unsafe { self::push(result, elements[index]) };
    }
    result
}

unsafe extern "C" fn vector_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let selector = runtime().print_string as Oop;
    let total = unsafe { self::count(self_object) };
    let mut output = Vec::from(&b"#["[..]);
    for index in 0..total {
        if index != 0 {
            output.push(b' ');
        }
        let value = unsafe { self::at(self_object, index) };
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
    if unsafe { (api.vtableOf)(other) } != unsafe { self::behavior() } {
        return api.nilObj;
    }
    let mut result = self_object;
    let total = unsafe { self::count(other) };
    for index in 0..total {
        let value = unsafe { self::at(other, index) };
        result = unsafe { self::push(result, value) };
    }
    result
}

unsafe extern "C" fn vector_new(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { self::empty() }
}

unsafe extern "C" fn install(
    api_pointer: *mut IdApi,
    value1: Oop,
    value2: Oop,
    print_string: Oop,
) -> Method {
    RUNTIME.get_or_init(|| Runtime {
        api: family().api,
        value1: value1 as usize,
        value2: value2 as usize,
        print_string: print_string as usize,
    });
    let api = unsafe { &*api_pointer };
    unsafe {
        (api.method)(self::behavior(), intern(api, b"size\0"), vector_size);
        (api.method)(self::behavior(), intern(api, b"at:\0"), vector_at_method);
        (api.method)(self::behavior(), intern(api, b"add:\0"), vector_add);
        (api.method)(self::behavior(), intern(api, b"do:\0"), vector_do);
        (api.method)(
            self::behavior(),
            intern(api, b"inject:into:\0"),
            vector_inject,
        );
        (api.method)(
            self::behavior(),
            intern(api, b"printString\0"),
            vector_print_string,
        );
        (api.method)(self::behavior(), intern(api, b",\0"), vector_comma);
        (api.method)(
            self::behavior(),
            intern(api, b"indexOf:\0"),
            vector_index_of,
        );
        (api.method)(
            self::behavior(),
            intern(api, b"copyFrom:to:\0"),
            vector_copy_from_to,
        );
        (api.method)(
            self::behavior(),
            intern(api, b"reversed\0"),
            vector_reversed,
        );
        (api.method)(self::behavior(), intern(api, b"sortBy:\0"), vector_sort_by);
        (api.method)(self::behavior(), intern(api, b"new\0"), vector_new);
        (api.method)(self::behavior(), intern(api, b"clone\0"), vector_new);
    }
    vector_comma
}
static API: ApiV4 = ApiV4 {
    version: 4,
    byte_size: size_of::<ApiV4>(),
    prepare: Some(prepare),
    finish_prototype: Some(finish_prototype),
    set_layout: Some(set_layout),
    set_owner: Some(set_owner),
    publish: Some(publish),
    empty: Some(empty),
    push: Some(push),
    behavior: Some(behavior),
    install: Some(install),
    for_each: Some(vector_do),
    inject: Some(vector_inject),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1) -> *mut c_void {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_vector_provider_v4() -> *const ApiV4 {
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
birnam_abi::collection_value_dependency!();
