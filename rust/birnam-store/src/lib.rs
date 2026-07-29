//! Rust Store rung: the first allocation-owning storage core.
//!
//! This slice keeps the public protocol deliberately small. It owns the
//! historied `(entity, attribute, value)` log and the schema-aware raw/flip,
//! assert, retract, lookup, cardinality, count, history, basis, as-of, and
//! live-triple operations. Image framing is delegated to the image modules
//! through the raw fact bridge below; no Rust reference or lock crosses an
//! IdApi callback.

use birnam_abi::IdApi;
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const RUST_STORE_IDENTITY: &[u8] = b"rust-store-n2.2-core\0";
const STORE_NAME: &[u8] = b"Store\0";
const STORE_VT_NAME: &[u8] = b"StoreVt\0";
const CELL_VT_NAME: &[u8] = b"CellVt\0";
const GLOBALS_VT_NAME: &[u8] = b"GlobalsVt\0";
const GLOBALS_NAME: &[u8] = b"Globals\0";

static METHOD_OBSERVER: AtomicUsize = AtomicUsize::new(0);
static METHOD_AUTHORIZER: AtomicUsize = AtomicUsize::new(0);
static STORE_ROOT: AtomicUsize = AtomicUsize::new(0);
static ROOT_PROVIDER_REGISTERED: AtomicBool = AtomicBool::new(false);
static OVERWRITE_HOOK_REGISTERED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Op {
    Assert = 0,
    Retract = 1,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Fact {
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    tx: u64,
    op: Op,
}

#[repr(C)]
struct StoreState {
    facts: Vec<Fact>,
    raw: bool,
    tx: u64,
}

#[repr(C)]
struct CellState {
    store: birnam_abi::Oop,
    entity: birnam_abi::Oop,
    attribute: birnam_abi::Oop,
}

impl StoreState {
    fn new() -> Self {
        Self {
            facts: Vec::new(),
            raw: true,
            tx: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    store_vt: usize,
    store: usize,
    cell_vt: usize,
    globals_vt: usize,
    globals: usize,
    cardinality: usize,
    one: usize,
    many: usize,
    history: usize,
    primitive: usize,
    globals_entity: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Store: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

fn as_oop(value: usize) -> birnam_abi::Oop {
    value as birnam_abi::Oop
}

fn oop_address(value: birnam_abi::Oop) -> usize {
    value as usize
}

unsafe fn api(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi table and the host
    // keeps the selected floor dylib loaded for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> birnam_abi::Oop {
    debug_assert!(name.last() == Some(&0));
    // SAFETY: every caller supplies a static NUL-terminated name.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn import(api: &IdApi, name: &[u8]) -> birnam_abi::Oop {
    // SAFETY: module names are static NUL-terminated strings.
    unsafe { (api.import)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn state_mut(store: birnam_abi::Oop) -> &'static mut StoreState {
    if store.is_null() {
        abort("Store object was null");
    }
    // SAFETY: every Rust Store object is allocated with exactly this payload
    // shape by the selected IdApi floor.
    unsafe { &mut *(store.cast::<StoreState>()) }
}

fn same(a: birnam_abi::Oop, b: birnam_abi::Oop) -> bool {
    a == b
}

type MethodMutationAuthorizer = unsafe extern "C" fn() -> birnam_abi::Oop;

fn mop_error(runtime: &Runtime, kind: &[u8]) -> birnam_abi::Oop {
    let api = unsafe { api(runtime) };
    let error = unsafe { import(api, b"Error\0") };
    if error.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:\0") };
    let kind = unsafe { intern(api, kind) };
    let arguments = [kind];
    // SAFETY: the imported Error object owns the frozen `kind:` protocol.
    unsafe { (api.send)(error, selector, arguments.as_ptr(), arguments.len()) }
}

unsafe extern "C" fn mark_mop_roots(mark: birnam_abi::MarkFn) {
    let observer = METHOD_OBSERVER.load(Ordering::Acquire);
    if observer != 0 {
        // SAFETY: the observer is an Oop registered through the live IdApi and
        // remains process-owned until the provider is replaced (which is
        // rejected by the registration law).
        unsafe { mark(observer as birnam_abi::Oop) };
    }
}

fn authorize(runtime: &Runtime) -> Option<birnam_abi::Oop> {
    let encoded = METHOD_AUTHORIZER.load(Ordering::Acquire);
    if encoded == 0 {
        return None;
    }
    // SAFETY: registration accepts only the encoded C callback address; the
    // provider dylib remains pinned for process lifetime by the host.
    let callback: MethodMutationAuthorizer = unsafe { core::mem::transmute(encoded) };
    Some(unsafe { callback() }).filter(|answer| *answer != unsafe { api(runtime) }.trueObj)
}

unsafe extern "C" fn set_method_observer(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let candidate = unsafe { *args } as usize;
    let current = METHOD_OBSERVER.load(Ordering::Acquire);
    if current != 0 && current != candidate {
        return mop_error(runtime, b"methodMutationObserverAlreadySet\0");
    }
    if current == 0 {
        let _ = METHOD_OBSERVER.compare_exchange(0, candidate, Ordering::AcqRel, Ordering::Acquire);
    }
    self_object
}

unsafe extern "C" fn set_method_authorizer(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    // SAFETY: this selector has exactly one SmallInt argument carrying the
    // callback address, matching the frozen Store callback ABI.
    let candidate = unsafe { (api.smallIntValue)(*args) } as usize;
    let current = METHOD_AUTHORIZER.load(Ordering::Acquire);
    if current != 0 && current != candidate {
        return mop_error(runtime, b"methodMutationAuthorizerAlreadySet\0");
    }
    if current == 0 {
        let _ =
            METHOD_AUTHORIZER.compare_exchange(0, candidate, Ordering::AcqRel, Ordering::Acquire);
    }
    self_object
}

unsafe extern "C" fn notify_method_observer(
    runtime: &Runtime,
    selector: birnam_abi::Oop,
    vtable: birnam_abi::Oop,
) {
    let observer = METHOD_OBSERVER.load(Ordering::Acquire);
    if observer == 0 {
        return;
    }
    let api = unsafe { api(runtime) };
    let message = unsafe { intern(api, b"managedMethodChanged:on:\0") };
    let arguments = [selector, vtable];
    // SAFETY: observer registration roots the receiver and the callback's
    // answer is deliberately ignored by the MOP law.
    let _ = unsafe {
        (api.send)(
            observer as birnam_abi::Oop,
            message,
            arguments.as_ptr(),
            arguments.len(),
        )
    };
}

unsafe fn append_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
) {
    let tx = state.tx;
    state.tx = state.tx.saturating_add(1);
    state.facts.push(Fact { e, a, v, tx, op });
}

fn current_value_raw_as_of(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    basis: Option<u64>,
) -> birnam_abi::Oop {
    state
        .facts
        .iter()
        .rev()
        .find(|fact| {
            same(fact.e, e) && same(fact.a, a) && basis.is_none_or(|watermark| fact.tx < watermark)
        })
        .map_or(ptr::null_mut(), |fact| {
            if fact.op == Op::Assert {
                fact.v
            } else {
                ptr::null_mut()
            }
        })
}

fn current_value_raw(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
) -> birnam_abi::Oop {
    current_value_raw_as_of(state, e, a, None)
}

fn count_values(state: &StoreState, e: birnam_abi::Oop, a: birnam_abi::Oop) -> usize {
    let mut seen = Vec::new();
    let mut count = 0;
    for fact in state.facts.iter().rev() {
        if !same(fact.e, e) || !same(fact.a, a) || seen.contains(&fact.v) {
            continue;
        }
        seen.push(fact.v);
        if fact.op == Op::Assert {
            count += 1;
        }
    }
    count
}

fn cardinality(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> usize {
    let value = current_value_raw(state, attribute, as_oop(runtime.cardinality));
    if value.is_null() {
        runtime.one
    } else {
        value as usize
    }
}

unsafe fn seed(runtime: &Runtime, store: birnam_abi::Oop) {
    let state = unsafe { state_mut(store) };
    // The schema is data. This self-referential knot makes cardinality itself
    // resolvable without a hardcoded special case in the assert path.
    unsafe {
        append_fact(
            state,
            as_oop(runtime.cardinality),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
            Op::Assert,
        );
        append_fact(
            state,
            intern(api(runtime), b"name\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
            Op::Assert,
        );
        append_fact(
            state,
            intern(api(runtime), b"history\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
            Op::Assert,
        );
    }
    state.raw = false;
}

unsafe fn fresh_store(runtime: &Runtime) -> birnam_abi::Oop {
    let api = unsafe { api(runtime) };
    // SAFETY: the floor allocates a payload of the requested byte extent and
    // returns its object base, matching StoreState's C-compatible carrier.
    let store = unsafe { (api.alloc)(as_oop(runtime.store_vt), size_of::<StoreState>()) };
    if store.is_null() {
        abort("could not allocate Rust Store state");
    }
    // SAFETY: the allocation above is exclusively owned by this new Store
    // object and has exactly StoreState's extent and alignment.
    unsafe { ptr::write(store.cast::<StoreState>(), StoreState::new()) };
    unsafe { seed(runtime, store) };
    store
}

unsafe extern "C" fn trace_store(store: birnam_abi::Oop, mark: birnam_abi::MarkFn) {
    if store.is_null() {
        return;
    }
    // Read the vector metadata without holding a Rust reference across the
    // foreign mark callback. Fact is Copy, so each callback receives a local
    // carrier rather than a borrow into the Store state.
    let state = store.cast::<StoreState>();
    // SAFETY: `state` points at a live Rust Store payload; the callback contract
    // supplies a valid mark function for this synchronous trace.
    let (facts, length) = unsafe { ((*state).facts.as_ptr(), (*state).facts.len()) };
    for index in 0..length {
        // SAFETY: the vector remains owned by the reachable Store object during
        // this trace and `index` is bounded by its captured length.
        let fact = unsafe { *facts.add(index) };
        // SAFETY: the collector supplied the callback and each field is an Oop
        // carrier retained by the Store's fact log. Retraction entries retain
        // their value for history and must therefore remain roots too.
        unsafe { mark(fact.e) };
        unsafe { mark(fact.a) };
        unsafe { mark(fact.v) };
    }
}

unsafe extern "C" fn trace_cell(cell: birnam_abi::Oop, mark: birnam_abi::MarkFn) {
    if cell.is_null() {
        return;
    }
    // SAFETY: Cell objects are allocated with exactly this payload shape and
    // the callback is synchronous, so copy the carriers before marking.
    let state = unsafe { &*(cell.cast::<CellState>()) };
    let (store, entity, attribute) = (state.store, state.entity, state.attribute);
    // SAFETY: the collector supplied the callback and all three carriers are
    // retained by the reachable Cell handle.
    unsafe { mark(store) };
    unsafe { mark(entity) };
    unsafe { mark(attribute) };
}

unsafe extern "C" fn raw_assert(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let state = unsafe { state_mut(self_object) };
    // SAFETY: IdApi Method calls carry the declared non-null argument buffer
    // for this three-argument selector.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    unsafe { append_fact(state, args[0], args[1], args[2], Op::Assert) };
    self_object
}

unsafe extern "C" fn flip(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    unsafe { state_mut(self_object) }.raw = false;
    self_object
}

unsafe extern "C" fn assert_fact(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let state = unsafe { state_mut(self_object) };
    if state.raw {
        unsafe { append_fact(state, args[0], args[1], args[2], Op::Assert) };
        return self_object;
    }
    let is_many = cardinality(runtime, state, args[1]) == runtime.many;
    if is_many {
        if is_member(state, args[0], args[1], args[2]) {
            return self_object;
        }
        unsafe { append_fact(state, args[0], args[1], args[2], Op::Assert) };
    } else {
        let old = current_value_raw(state, args[0], args[1]);
        if same(old, args[2]) {
            return self_object;
        }
        if history_enabled(runtime, state, args[1]) {
            unsafe { append_fact(state, args[0], args[1], args[2], Op::Assert) };
        } else {
            replace_one(state, args[0], args[1], args[2]);
        }
    }
    self_object
}

fn history_enabled(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> bool {
    current_value_raw(state, attribute, as_oop(runtime.history)) == unsafe { api(runtime) }.trueObj
}

fn is_member(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) -> bool {
    state
        .facts
        .iter()
        .rev()
        .find(|fact| same(fact.e, e) && same(fact.a, a) && same(fact.v, v))
        .is_some_and(|fact| fact.op == Op::Assert)
}

fn replace_one(state: &mut StoreState, e: birnam_abi::Oop, a: birnam_abi::Oop, v: birnam_abi::Oop) {
    if let Some(fact) = state
        .facts
        .iter_mut()
        .rev()
        .find(|fact| same(fact.e, e) && same(fact.a, a))
    {
        fact.v = v;
        fact.op = Op::Assert;
        fact.tx = state.tx;
        state.tx = state.tx.saturating_add(1);
    } else {
        unsafe { append_fact(state, e, a, v, Op::Assert) };
    }
}

fn remove_live(state: &mut StoreState, e: birnam_abi::Oop, a: birnam_abi::Oop, v: birnam_abi::Oop) {
    state.facts.retain(|fact| {
        !(same(fact.e, e) && same(fact.a, a) && same(fact.v, v) && fact.op == Op::Assert)
    });
}

unsafe extern "C" fn retract(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let state = unsafe { state_mut(self_object) };
    if !is_member(state, args[0], args[1], args[2]) {
        return self_object;
    }
    if history_enabled(runtime, state, args[1]) {
        unsafe { append_fact(state, args[0], args[1], args[2], Op::Retract) };
    } else {
        remove_live(state, args[0], args[1], args[2]);
    }
    self_object
}

unsafe extern "C" fn tracked_add_method(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    if let Some(answer) = authorize(runtime) {
        return answer;
    }
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let (selector, vtable, method_carrier) = (args[0], args[1], args[2]);
    let old = current_value_raw(unsafe { state_mut(self_object) }, vtable, selector);
    if !old.is_null() {
        let retract_args = [vtable, selector, old];
        unsafe { retract(self_object, retract_args.as_ptr(), retract_args.len()) };
    }
    let api = unsafe { api(runtime) };
    // SAFETY: the tracked MOP ABI carries the C Method address in an Oop-sized
    // carrier; the IdApi method table stores the same pointer representation.
    let method: birnam_abi::Method = unsafe { core::mem::transmute(method_carrier) };
    unsafe { (api.method)(vtable, selector, method) };
    // The derived mirror is committed only after the authoritative dictionary
    // write has succeeded. This preserves retract -> dict -> assert ordering.
    let mirror_args = [vtable, selector, method_carrier];
    unsafe { assert_fact(self_object, mirror_args.as_ptr(), mirror_args.len()) };
    unsafe { notify_method_observer(runtime, selector, vtable) };
    self_object
}

unsafe extern "C" fn tracked_remove_method(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    if let Some(answer) = authorize(runtime) {
        return answer;
    }
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let selector = args[0];
    let vtable = args[1];
    let api = unsafe { api(runtime) };
    if unsafe { (api.vtableOwner)(vtable) }.is_null() {
        return mop_error(runtime, b"unstableMethodTarget\0");
    }
    let mut direct = false;
    let mut index = 0;
    loop {
        let bound = unsafe { (api.vtableSelectorAt)(vtable, index) };
        if bound.is_null() {
            break;
        }
        if bound == selector {
            direct = true;
            break;
        }
        index += 1;
    }
    if !direct {
        return mop_error(runtime, b"missingDirectMethod\0");
    }
    let old = current_value_raw(unsafe { state_mut(self_object) }, vtable, selector);
    if !old.is_null() {
        let retract_args = [vtable, selector, old];
        unsafe { retract(self_object, retract_args.as_ptr(), retract_args.len()) };
    }
    let remove_selector = unsafe { intern(api, b"removeMethod:\0") };
    let remove_args = [selector];
    // SAFETY: the vtable owns the frozen floor `removeMethod:` protocol.
    let _ = unsafe { (api.send)(vtable, remove_selector, remove_args.as_ptr(), 1) };
    unsafe { notify_method_observer(runtime, selector, vtable) };
    self_object
}

fn is_type_vtable(api: &birnam_abi::IdApi, vtable: birnam_abi::Oop) -> bool {
    let mut current = vtable;
    while !current.is_null() {
        if current == api.object_vt {
            return true;
        }
        current = unsafe { (api.vtableParent)(current) };
    }
    false
}

fn is_registered_type_vtable(api: &birnam_abi::IdApi, object: birnam_abi::Oop) -> bool {
    let count = unsafe { (api.vtableCount)() };
    (0..count).any(|index| {
        let vtable = unsafe { (api.vtableAt)(index) };
        vtable == object && is_type_vtable(api, vtable)
    })
}

unsafe extern "C" fn backfill_mirror(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    // Collect the foreign vtable/selector carriers before borrowing StoreState;
    // vtable enumeration is an IdApi callback boundary.
    let mut pairs = Vec::new();
    let vtable_count = unsafe { (api.vtableCount)() };
    for index in 0..vtable_count {
        let vtable = unsafe { (api.vtableAt)(index) };
        if vtable.is_null() || !is_type_vtable(api, vtable) {
            continue;
        }
        let mut selector_index = 0;
        loop {
            let selector = unsafe { (api.vtableSelectorAt)(vtable, selector_index) };
            if selector.is_null() {
                break;
            }
            pairs.push((vtable, selector));
            selector_index += 1;
        }
    }
    let state = unsafe { state_mut(self_object) };
    for (vtable, selector) in pairs {
        if current_value_raw(state, vtable, selector).is_null() {
            unsafe {
                append_fact(
                    state,
                    vtable,
                    selector,
                    as_oop(runtime.primitive),
                    Op::Assert,
                )
            };
        }
    }
    self_object
}

unsafe extern "C" fn mirror_overwrite_hook(vtable: birnam_abi::Oop, selector: birnam_abi::Oop) {
    let store = STORE_ROOT.load(Ordering::Acquire);
    if store == 0 {
        return;
    }
    // SAFETY: the Store object is rooted by the provider registered at bring-up
    // and its payload shape is fixed for this Rust rung.
    let state = unsafe { state_mut(store as birnam_abi::Oop) };
    let old = current_value_raw(state, vtable, selector);
    if !old.is_null() {
        remove_live(state, vtable, selector, old);
    }
}

unsafe extern "C" fn image_facts_into(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    // SAFETY: this selector has exactly one argument.
    let collector = unsafe { *args };
    let selector = unsafe { intern(api, b"e:a:v:tx:op:\0") };
    let facts = unsafe { state_mut(self_object) }.facts.clone();
    for fact in facts {
        // The MOP mirror is a rebuilt cache, not independently serialized. In
        // particular, method carriers may be stale raw code pointers and must
        // never reach the heap codec.
        if is_registered_type_vtable(api, fact.e) {
            continue;
        }
        let arguments = [
            fact.e,
            fact.a,
            fact.v,
            unsafe { (api.mkSmallInt)(fact.tx as i64) },
            unsafe { (api.mkSmallInt)(fact.op as u8 as i64) },
        ];
        // SAFETY: the collector owns the frozen five-argument image protocol.
        let _ = unsafe { (api.send)(collector, selector, arguments.as_ptr(), arguments.len()) };
    }
    self_object
}

unsafe extern "C" fn image_load_begin(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    unsafe { state_mut(self_object) }.facts.clear();
    self_object
}

unsafe extern "C" fn image_add_fact(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly five arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 5) };
    let tx = unsafe { (api(runtime).smallIntValue)(args[3]) } as u64;
    let op = match unsafe { (api(runtime).smallIntValue)(args[4]) } {
        0 => Op::Assert,
        1 => Op::Retract,
        _ => abort("image fact op was not assert/retract"),
    };
    unsafe { state_mut(self_object) }.facts.push(Fact {
        e: args[0],
        a: args[1],
        v: args[2],
        tx,
        op,
    });
    self_object
}

unsafe extern "C" fn image_load_end(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let state = unsafe { state_mut(self_object) };
    state.tx = unsafe { (api(runtime).smallIntValue)(args[0]) } as u64;
    state.raw = args[1] == unsafe { api(runtime) }.trueObj;
    self_object
}

unsafe extern "C" fn raw_current_value(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    current_value_raw(unsafe { state_mut(self_object) }, args[0], args[1])
}

unsafe extern "C" fn current_value_method(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let value = current_value_raw(unsafe { state_mut(self_object) }, args[0], args[1]);
    if value.is_null() {
        // SAFETY: runtime.api is the live floor table.
        unsafe { api(runtime) }.nilObj
    } else {
        value
    }
}

unsafe extern "C" fn current_value_if_absent(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let value = current_value_raw(unsafe { state_mut(self_object) }, args[0], args[1]);
    if !value.is_null() {
        return value;
    }
    let selector = unsafe { intern(api(runtime), b"value\0") };
    // SAFETY: the fallback owns the frozen zero-argument `value` protocol.
    unsafe { (api(runtime).send)(args[2], selector, ptr::null(), 0) }
}

unsafe extern "C" fn has_value(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let state = unsafe { state_mut(self_object) };
    let present = if cardinality(runtime, state, args[1]) == runtime.many {
        count_values(state, args[0], args[1]) != 0
    } else {
        !current_value_raw(state, args[0], args[1]).is_null()
    };
    if present {
        unsafe { api(runtime) }.trueObj
    } else {
        unsafe { api(runtime) }.falseObj
    }
}

unsafe extern "C" fn count(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let state = unsafe { state_mut(self_object) };
    let count = if cardinality(runtime, state, args[1]) == runtime.one {
        usize::from(!current_value_raw(state, args[0], args[1]).is_null())
    } else {
        count_values(state, args[0], args[1])
    };
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (api(runtime).mkSmallInt)(count as i64) }
}

fn live_attributes(
    state: &StoreState,
    entity: birnam_abi::Oop,
    runtime: &Runtime,
) -> Vec<birnam_abi::Oop> {
    let mut attributes = Vec::new();
    for fact in &state.facts {
        if !same(fact.e, entity) || attributes.contains(&fact.a) {
            continue;
        }
        attributes.push(fact.a);
    }
    attributes.retain(|attribute| {
        if cardinality(runtime, state, *attribute) == runtime.many {
            count_values(state, entity, *attribute) != 0
        } else {
            !current_value_raw(state, entity, *attribute).is_null()
        }
    });
    attributes
}

unsafe extern "C" fn attr_count(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let entity = unsafe { *args };
    let state = unsafe { state_mut(self_object) };
    let count = live_attributes(state, entity, runtime).len();
    unsafe { (api(runtime).mkSmallInt)(count as i64) }
}

unsafe extern "C" fn attr_at(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let index = unsafe { (api(runtime).smallIntValue)(args[1]) } as usize;
    let attributes = live_attributes(unsafe { state_mut(self_object) }, args[0], runtime);
    attributes.get(index).copied().unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn history(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let history_len = unsafe { state_mut(self_object) }
        .facts
        .iter()
        .filter(|fact| same(fact.e, args[0]) && same(fact.a, args[1]))
        .count();
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (api(runtime).mkSmallInt)(history_len as i64) }
}

unsafe extern "C" fn basis(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let tx = unsafe { state_mut(self_object) }.tx;
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (api(runtime).mkSmallInt)(tx as i64) }
}

fn history_not_retained_error(runtime: &Runtime) -> birnam_abi::Oop {
    let api = unsafe { api(runtime) };
    let error = unsafe { import(api, b"Error\0") };
    if error.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:\0") };
    let kind = unsafe { intern(api, b"historyNotRetained\0") };
    let arguments = [kind];
    // SAFETY: the imported Error object owns the frozen `kind:` protocol.
    unsafe { (api.send)(error, selector, arguments.as_ptr(), arguments.len()) }
}

unsafe extern "C" fn value_of_as_of(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let basis = unsafe { (api(runtime).smallIntValue)(args[2]) } as u64;
    let state = unsafe { state_mut(self_object) };
    let value = current_value_raw_as_of(state, args[0], args[1], Some(basis));
    if !value.is_null() {
        return value;
    }
    let history = current_value_raw(state, args[1], as_oop(runtime.history));
    let has_later_fact = state
        .facts
        .iter()
        .any(|fact| same(fact.e, args[0]) && same(fact.a, args[1]) && fact.tx >= basis);
    if history != unsafe { api(runtime) }.trueObj && has_later_fact {
        return history_not_retained_error(runtime);
    }
    unsafe { api(runtime) }.nilObj
}

#[derive(Clone, Copy)]
struct Triple {
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
}

fn visible(fact: &Fact, hi: Option<u64>) -> bool {
    hi.is_none_or(|watermark| fact.tx < watermark)
}

fn in_lower_bound(fact: &Fact, lo: Option<u64>) -> bool {
    lo.is_none_or(|watermark| fact.tx >= watermark)
}

fn cardinality_as_of(
    runtime: &Runtime,
    state: &StoreState,
    attribute: birnam_abi::Oop,
    hi: Option<u64>,
) -> birnam_abi::Oop {
    let value = current_value_raw_as_of(state, attribute, as_oop(runtime.cardinality), hi);
    if value.is_null() {
        as_oop(runtime.one)
    } else {
        value
    }
}

fn collect_live_triples(
    runtime: &Runtime,
    state: &StoreState,
    lo: Option<u64>,
    hi: Option<u64>,
) -> Vec<Triple> {
    let mut pairs: Vec<(birnam_abi::Oop, birnam_abi::Oop)> = Vec::new();
    for fact in &state.facts {
        if !visible(fact, hi)
            || pairs
                .iter()
                .any(|(entity, attribute)| same(*entity, fact.e) && same(*attribute, fact.a))
        {
            continue;
        }
        pairs.push((fact.e, fact.a));
    }

    let mut triples = Vec::new();
    for (entity, attribute) in pairs {
        let cardinality = cardinality_as_of(runtime, state, attribute, hi);
        if cardinality == as_oop(runtime.many) {
            let mut seen = Vec::new();
            for fact in state.facts.iter().rev() {
                if !same(fact.e, entity) || !same(fact.a, attribute) || !visible(fact, hi) {
                    continue;
                }
                if seen.contains(&fact.v) {
                    continue;
                }
                seen.push(fact.v);
                if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                    triples.push(Triple {
                        e: entity,
                        a: attribute,
                        v: fact.v,
                    });
                }
            }
        } else {
            for fact in state.facts.iter().rev() {
                if same(fact.e, entity) && same(fact.a, attribute) && visible(fact, hi) {
                    if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                        triples.push(Triple {
                            e: entity,
                            a: attribute,
                            v: fact.v,
                        });
                    }
                    break;
                }
            }
        }
    }
    triples
}

unsafe fn emit_live_triples(runtime: &Runtime, block: birnam_abi::Oop, triples: Vec<Triple>) {
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"value:value:value:\0") };
    // Materialization above finishes all Store borrows before this first
    // callback. Each argument array is a local carrier, so no Rust reference
    // spans a foreign send or a possible re-entry into the Store.
    for triple in triples {
        let arguments = [triple.e, triple.a, triple.v];
        // SAFETY: the block owns the frozen three-argument callback protocol.
        let _ = unsafe { (api.send)(block, selector, arguments.as_ptr(), arguments.len()) };
    }
}

unsafe extern "C" fn live_triples_do(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let block = unsafe { *args };
    let triples = collect_live_triples(runtime, unsafe { state_mut(self_object) }, None, None);
    unsafe { emit_live_triples(runtime, block, triples) };
    self_object
}

unsafe extern "C" fn live_triples_as_of_do(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let hi = unsafe { (api(runtime).smallIntValue)(args[0]) } as u64;
    let triples = collect_live_triples(runtime, unsafe { state_mut(self_object) }, None, Some(hi));
    unsafe { emit_live_triples(runtime, args[1], triples) };
    self_object
}

unsafe extern "C" fn live_triples_since_do(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let lo = unsafe { (api(runtime).smallIntValue)(args[0]) } as u64;
    let triples = collect_live_triples(runtime, unsafe { state_mut(self_object) }, Some(lo), None);
    unsafe { emit_live_triples(runtime, args[1], triples) };
    self_object
}

unsafe extern "C" fn live_triples_between_and_do(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let lo = unsafe { (api(runtime).smallIntValue)(args[0]) } as u64;
    let hi = unsafe { (api(runtime).smallIntValue)(args[1]) } as u64;
    let triples = collect_live_triples(
        runtime,
        unsafe { state_mut(self_object) },
        Some(lo),
        Some(hi),
    );
    unsafe { emit_live_triples(runtime, args[2], triples) };
    self_object
}

unsafe extern "C" fn cardinality_method(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let args = unsafe { core::slice::from_raw_parts(args, 1) };
    let value = current_value_raw(
        unsafe { state_mut(self_object) },
        args[0],
        as_oop(runtime.cardinality),
    );
    if value.is_null() {
        as_oop(runtime.one)
    } else {
        value
    }
}

unsafe extern "C" fn new_store(
    _self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    unsafe { fresh_store(runtime) }
}

unsafe extern "C" fn cell_for(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let api = unsafe { api(runtime) };
    // SAFETY: the floor allocates a payload of the requested extent and the
    // Cell vtable carries the trace hook installed during module bring-up.
    let cell = unsafe { (api.alloc)(as_oop(runtime.cell_vt), size_of::<CellState>()) };
    if cell.is_null() {
        abort("could not allocate Rust Cell state");
    }
    // SAFETY: this allocation is exclusively owned by the new Cell object.
    unsafe {
        ptr::write(
            cell.cast::<CellState>(),
            CellState {
                store: self_object,
                entity: args[0],
                attribute: args[1],
            },
        )
    };
    cell
}

unsafe extern "C" fn cell_value(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // Copy all carriers before the foreign send so no Rust reference spans the
    // callback or a possible re-entry into the Store.
    let (store, entity, attribute) = unsafe {
        let state = &*(self_object.cast::<CellState>());
        (state.store, state.entity, state.attribute)
    };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"currentValue:attr:\0") };
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe { (api.send)(store, selector, [entity, attribute].as_ptr(), 2) }
}

unsafe extern "C" fn cell_set(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let value = unsafe { *args };
    let (store, entity, attribute) = unsafe {
        let state = &*(self_object.cast::<CellState>());
        (state.store, state.entity, state.attribute)
    };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"assert:attr:value:\0") };
    let arguments = [entity, attribute, value];
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe { (api.send)(store, selector, arguments.as_ptr(), arguments.len()) };
    self_object
}

unsafe extern "C" fn cell_history(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let (store, entity, attribute) = unsafe {
        let state = &*(self_object.cast::<CellState>());
        (state.store, state.entity, state.attribute)
    };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"history:attr:\0") };
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe { (api.send)(store, selector, [entity, attribute].as_ptr(), 2) }
}

unsafe extern "C" fn globals_at(
    _self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let key = unsafe { *args };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"currentValue:attr:\0") };
    let arguments = [as_oop(runtime.globals_entity), key];
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe {
        (api.send)(
            as_oop(runtime.store),
            selector,
            arguments.as_ptr(),
            arguments.len(),
        )
    }
}

unsafe extern "C" fn globals_at_if_absent(
    _self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let api = unsafe { api(runtime) };
    let read_selector = unsafe { intern(api, b"rawCurrentValue:attr:\0") };
    let read_arguments = [as_oop(runtime.globals_entity), args[0]];
    // SAFETY: the raw read preserves the absent/bound-nil distinction.
    let value = unsafe {
        (api.send)(
            as_oop(runtime.store),
            read_selector,
            read_arguments.as_ptr(),
            read_arguments.len(),
        )
    };
    if !value.is_null() {
        return value;
    }
    let value_selector = unsafe { intern(api, b"value\0") };
    // SAFETY: the absent block owns the frozen `value` protocol.
    unsafe { (api.send)(args[1], value_selector, ptr::null(), 0) }
}

unsafe extern "C" fn globals_at_put(
    _self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"assert:attr:value:\0") };
    let arguments = [as_oop(runtime.globals_entity), args[0], args[1]];
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe {
        (api.send)(
            as_oop(runtime.store),
            selector,
            arguments.as_ptr(),
            arguments.len(),
        )
    };
    args[1]
}

unsafe extern "C" fn globals_includes(
    _self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly one argument.
    let key = unsafe { *args };
    let api = unsafe { api(runtime) };
    let selector = unsafe { intern(api, b"hasValue:attr:\0") };
    let arguments = [as_oop(runtime.globals_entity), key];
    // SAFETY: selector and argument buffer match the frozen IdApi send ABI.
    unsafe {
        (api.send)(
            as_oop(runtime.store),
            selector,
            arguments.as_ptr(),
            arguments.len(),
        )
    }
}

unsafe fn register_methods(runtime: &Runtime) {
    let api = unsafe { api(runtime) };
    let vt = as_oop(runtime.store_vt);
    let methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"rawAssert:attr:value:\0", raw_assert),
        (b"flip\0", flip),
        (b"assert:attr:value:\0", assert_fact),
        (b"rawCurrentValue:attr:\0", raw_current_value),
        (b"currentValue:attr:\0", current_value_method),
        (b"currentValue:attr:ifAbsent:\0", current_value_if_absent),
        (b"hasValue:attr:\0", has_value),
        (b"count:attr:\0", count),
        (b"attrCount:\0", attr_count),
        (b"attrAt:index:\0", attr_at),
        (b"history:attr:\0", history),
        (b"retract:attr:value:\0", retract),
        (b"basis\0", basis),
        (b"valueOf:attr:asOf:\0", value_of_as_of),
        (b"liveTriplesDo:\0", live_triples_do),
        (b"liveTriplesAsOf:do:\0", live_triples_as_of_do),
        (b"liveTriplesSince:do:\0", live_triples_since_do),
        (b"liveTriplesBetween:and:do:\0", live_triples_between_and_do),
        (b"cardinality:\0", cardinality_method),
        (b"imageFactsInto:\0", image_facts_into),
        (b"imageLoadBegin\0", image_load_begin),
        (b"imageAddFact:attr:value:tx:op:\0", image_add_fact),
        (b"imageLoadEnd:raw:\0", image_load_end),
        (b"new\0", new_store),
        (b"cellFor:attr:\0", cell_for),
        (b"addMethod:on:with:\0", tracked_add_method),
        (b"removeMethod:on:\0", tracked_remove_method),
        (b"setMethodMutationObserver:\0", set_method_observer),
        (b"setMethodMutationAuthorizer:\0", set_method_authorizer),
        (b"backfillMirror\0", backfill_mirror),
    ];
    for (selector, method) in methods {
        // SAFETY: selector names are static C strings and each function matches
        // the frozen Method ABI.
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(vt, selector, *method) };
    }
    // The Store payload is raw except for Oops held in its external Vec; the
    // trace hook marks those facts whenever the exported Store object is live.
    unsafe {
        (api.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(vt, trace_store);
    }
}

unsafe fn register_cell_methods(runtime: &Runtime) {
    let api = unsafe { api(runtime) };
    let vt = as_oop(runtime.cell_vt);
    let methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"value\0", cell_value),
        (b"set:\0", cell_set),
        (b"history\0", cell_history),
    ];
    for (selector, method) in methods {
        // SAFETY: selector names are static C strings and each function matches
        // the frozen Method ABI.
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(vt, selector, *method) };
    }
    unsafe {
        (api.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(vt, trace_cell);
    }
}

unsafe fn register_globals_methods(runtime: &Runtime) {
    let api = unsafe { api(runtime) };
    let vt = as_oop(runtime.globals_vt);
    let methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"at:\0", globals_at),
        (b"at:ifAbsent:\0", globals_at_if_absent),
        (b"at:put:\0", globals_at_put),
        (b"includes:\0", globals_includes),
    ];
    for (selector, method) in methods {
        // SAFETY: selector names are static C strings and each function matches
        // the frozen Method ABI.
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(vt, selector, *method) };
    }
    unsafe { (api.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW) };
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(STORE_NAME.as_ptr().cast()) != 0 };
        let (store_vt, store, cell_vt, globals_vt, globals) = if reloading {
            (
                unsafe { import(api, STORE_VT_NAME) },
                unsafe { import(api, STORE_NAME) },
                unsafe { import(api, CELL_VT_NAME) },
                unsafe { import(api, GLOBALS_VT_NAME) },
                unsafe { import(api, GLOBALS_NAME) },
            )
        } else {
            let store_vt = unsafe { (api.proto)(api.object_vt) };
            let cell_vt = unsafe { (api.proto)(api.object_vt) };
            let globals_vt = unsafe { (api.proto)(api.object_vt) };
            let store = unsafe { (api.alloc)(store_vt, size_of::<StoreState>()) };
            if store.is_null() {
                abort("could not allocate Rust Store object");
            }
            // SAFETY: this is the first and only initialization of the payload.
            unsafe { ptr::write(store.cast::<StoreState>(), StoreState::new()) };
            let globals = unsafe { (api.alloc)(globals_vt, 0) };
            (store_vt, store, cell_vt, globals_vt, globals)
        };
        if [store_vt, store, cell_vt, globals_vt, globals]
            .iter()
            .any(|value| value.is_null())
        {
            abort("Store exports were incomplete");
        }
        let cardinality = unsafe { intern(api, b"cardinality\0") };
        let one = unsafe { intern(api, b"one\0") };
        let many = unsafe { intern(api, b"many\0") };
        let history = unsafe { intern(api, b"history\0") };
        let primitive = unsafe { intern(api, b"primitive\0") };
        let globals_entity = unsafe { intern(api, b"__globals__\0") };
        Runtime {
            api: api_pointer as usize,
            store_vt: oop_address(store_vt),
            store: oop_address(store),
            cell_vt: oop_address(cell_vt),
            globals_vt: oop_address(globals_vt),
            globals: oop_address(globals),
            cardinality: oop_address(cardinality),
            one: oop_address(one),
            many: oop_address(many),
            history: oop_address(history),
            primitive: oop_address(primitive),
            globals_entity: oop_address(globals_entity),
        }
    });
    unsafe {
        register_methods(&runtime);
        register_cell_methods(&runtime);
        register_globals_methods(&runtime);
    };
    let store = as_oop(runtime.store);
    STORE_ROOT.store(runtime.store, Ordering::Release);
    if !ROOT_PROVIDER_REGISTERED.swap(true, Ordering::AcqRel) {
        unsafe { (api.gcRegisterRootProvider)(mark_mop_roots) };
    }
    if !OVERWRITE_HOOK_REGISTERED.swap(true, Ordering::AcqRel) {
        unsafe { (api.setMethodOverwriteHook)(Some(mirror_overwrite_hook)) };
    }
    let state = unsafe { state_mut(store) };
    if state.facts.is_empty() {
        unsafe { seed(&runtime, store) };
    }
    if unsafe { (api.isExported)(STORE_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            let _ = (api.exportObj)(STORE_VT_NAME.as_ptr().cast(), as_oop(runtime.store_vt));
            let _ = (api.exportObj)(STORE_NAME.as_ptr().cast(), store);
            let _ = (api.exportObj)(CELL_VT_NAME.as_ptr().cast(), as_oop(runtime.cell_vt));
            let _ = (api.exportObj)(GLOBALS_VT_NAME.as_ptr().cast(), as_oop(runtime.globals_vt));
            let _ = (api.exportObj)(GLOBALS_NAME.as_ptr().cast(), as_oop(runtime.globals));
        }
    }
}

/// Identifies the Rust Store implementation for build and audit tools.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_store_rust_identity() -> *const c_char {
    RUST_STORE_IDENTITY.as_ptr().cast()
}

/// Initialize the Rust Store module against the one process-owned IdApi table.
///
/// # Safety
///
/// `api` must be the live process-owned `IdApi` table supplied by the host, or
/// null to request the documented fatal boundary. `_state` is an opaque host
/// token and is not retained by this rung.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    // SAFETY: forwarded host contract; bring_up performs the null check and
    // retains only the process-owned table address.
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_STORE_IDENTITY, birnam_store_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn core_identity_is_a_terminated_static_string() {
        // SAFETY: the export points to the crate's immutable NUL-terminated
        // static for the process lifetime.
        let identity = unsafe { CStr::from_ptr(birnam_store_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_STORE_IDENTITY);
    }
}
