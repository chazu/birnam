//! Rust Store rung: the first allocation-owning storage core.
//!
//! This slice keeps the public protocol deliberately small. It owns the
//! historied `(entity, attribute, value)` log and the schema-aware raw/flip,
//! assert, retract, lookup, cardinality, count, history, basis, as-of, and
//! live-triple operations. Image framing is delegated to the image modules
//! through the raw fact bridge below; no Rust reference or lock crosses an
//! IdApi callback.

use birnam_abi::{IdApi, foreign_address};
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
const DEFAULT_COMPACTION_TRANSITIONS: u64 = 4096;
const DEFAULT_FACT_BYTES_LIMIT: usize = 64 * 1024 * 1024;

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
    historied: bool,
    mirror: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RetentionPolicy {
    CurrentOnly,
    KeepSince(u64),
    KeepLast(u64),
}

#[repr(C)]
struct StoreState {
    facts: Vec<Fact>,
    compensation_reservations: Vec<CompensationReservation>,
    raw: bool,
    tx: u64,
    oldest_retained_basis: u64,
    retention: RetentionPolicy,
    transitions_since_compaction: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompensationReservation {
    owner: usize,
    facts: usize,
}

use birnam_abi::private::StoreCompensationApiV1;

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
            compensation_reservations: Vec::new(),
            raw: true,
            tx: 0,
            oldest_retained_basis: 0,
            retention: RetentionPolicy::CurrentOnly,
            transitions_since_compaction: 0,
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
    true_value: usize,
    compaction_transitions: u64,
    fact_bytes_limit: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Store: {message}");
    std::process::abort();
}

fn positive_config(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or_else(|| abort(&format!("{name} must be a positive integer"))),
        Err(std::env::VarError::NotPresent) => default,
        Err(std::env::VarError::NotUnicode(_)) => abort(&format!("{name} was not valid UTF-8")),
    }
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
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.store_vt,
            runtime.store,
            runtime.cell_vt,
            runtime.globals_vt,
            runtime.globals,
            runtime.cardinality,
            runtime.one,
            runtime.many,
            runtime.history,
            runtime.primitive,
            runtime.globals_entity,
            runtime.true_value,
        ] {
            // SAFETY: these are process-owned Oops cached by the live Store
            // module; none is merely an unreported native pointer.
            unsafe { mark(value as birnam_abi::Oop) };
        }
    }
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
    // SAFETY: this selector has exactly one argument. Native callback authority
    // is explicit and cannot be smuggled through a portable numeric object.
    let Some((candidate, _kind)) = (unsafe { foreign_address::value(api, *args) }) else {
        return mop_error(runtime, b"methodMutationAuthorizerNeedsForeignAddress\0");
    };
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

fn next_tx(state: &mut StoreState) -> u64 {
    let tx = state.tx;
    state.tx = state
        .tx
        .checked_add(1)
        .filter(|next| *next <= i64::MAX as u64)
        .unwrap_or_else(|| abort("Store durable basis exhausted SmallInt range"));
    state.transitions_since_compaction = state.transitions_since_compaction.saturating_add(1);
    tx
}

fn append_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
    historied: bool,
) {
    let tx = next_tx(state);
    state.facts.push(Fact {
        e,
        a,
        v,
        tx,
        op,
        historied,
        mirror: false,
    });
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
            false,
        );
        append_fact(
            state,
            intern(api(runtime), b"name\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
            Op::Assert,
            false,
        );
        append_fact(
            state,
            intern(api(runtime), b"history\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
            Op::Assert,
            false,
        );
    }
    state.raw = false;
    state.transitions_since_compaction = 0;
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
    let runtime = runtime();
    if prepare_semantic_mutation(runtime, state, true).is_err() {
        return mop_error(runtime, b"historyResourceLimit\0");
    }
    append_fact(state, args[0], args[1], args[2], Op::Assert, true);
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
        if prepare_semantic_mutation(runtime, state, true).is_err() {
            return mop_error(runtime, b"historyResourceLimit\0");
        }
        append_fact(state, args[0], args[1], args[2], Op::Assert, true);
        return self_object;
    }
    let is_many = cardinality(runtime, state, args[1]) == runtime.many;
    let historied = is_schema_fact(runtime, args[1]) || history_enabled(runtime, state, args[1]);
    if is_many {
        if is_member(state, args[0], args[1], args[2]) {
            return self_object;
        }
        if historied {
            if prepare_semantic_mutation(runtime, state, true).is_err() {
                return mop_error(runtime, b"historyResourceLimit\0");
            }
            append_fact(state, args[0], args[1], args[2], Op::Assert, true);
        } else {
            if record_projection_many(runtime, state, args[0], args[1], args[2], Op::Assert)
                .is_err()
            {
                return mop_error(runtime, b"historyResourceLimit\0");
            }
        }
    } else {
        let old = current_value_raw(state, args[0], args[1]);
        if same(old, args[2]) {
            return self_object;
        }
        if historied {
            if prepare_semantic_mutation(runtime, state, true).is_err() {
                return mop_error(runtime, b"historyResourceLimit\0");
            }
            append_fact(state, args[0], args[1], args[2], Op::Assert, true);
        } else {
            if record_projection_one(runtime, state, args[0], args[1], args[2], Op::Assert).is_err()
            {
                return mop_error(runtime, b"historyResourceLimit\0");
            }
        }
    }
    self_object
}

fn history_enabled(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> bool {
    current_value_raw(state, attribute, as_oop(runtime.history)) == unsafe { api(runtime) }.trueObj
}

fn is_schema_fact(runtime: &Runtime, attribute: birnam_abi::Oop) -> bool {
    attribute == as_oop(runtime.cardinality) || attribute == as_oop(runtime.history)
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

fn record_projection_one(
    runtime: &Runtime,
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
) -> Result<(), CompactError> {
    prepare_semantic_mutation(runtime, state, false)?;
    if let Some(index) = state
        .facts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, fact)| (same(fact.e, e) && same(fact.a, a)).then_some(index))
    {
        if !state.facts[index].historied {
            let tx = next_tx(state);
            let fact = &mut state.facts[index];
            fact.v = v;
            fact.op = op;
            fact.tx = tx;
            return Ok(());
        }
    }
    prepare_semantic_mutation(runtime, state, true)?;
    append_fact(state, e, a, v, op, false);
    Ok(())
}

fn record_projection_many(
    runtime: &Runtime,
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
) -> Result<(), CompactError> {
    prepare_semantic_mutation(runtime, state, false)?;
    if let Some(index) = state
        .facts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, fact)| {
            (same(fact.e, e) && same(fact.a, a) && same(fact.v, v)).then_some(index)
        })
    {
        if !state.facts[index].historied {
            let tx = next_tx(state);
            let fact = &mut state.facts[index];
            fact.op = op;
            fact.tx = tx;
            return Ok(());
        }
    }
    prepare_semantic_mutation(runtime, state, true)?;
    append_fact(state, e, a, v, op, false);
    Ok(())
}

fn remove_mirror_facts(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) {
    state
        .facts
        .retain(|fact| !(same(fact.e, e) && same(fact.a, a) && same(fact.v, v)));
}

fn append_mirror_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) {
    let tx = next_tx(state);
    state.facts.push(Fact {
        e,
        a,
        v,
        tx,
        op: Op::Assert,
        historied: false,
        mirror: true,
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
    if is_schema_fact(runtime, args[1]) || history_enabled(runtime, state, args[1]) {
        if prepare_semantic_mutation(runtime, state, true).is_err() {
            return mop_error(runtime, b"historyResourceLimit\0");
        }
        append_fact(state, args[0], args[1], args[2], Op::Retract, true);
    } else if cardinality(runtime, state, args[1]) == runtime.many {
        if record_projection_many(runtime, state, args[0], args[1], args[2], Op::Retract).is_err() {
            return mop_error(runtime, b"historyResourceLimit\0");
        }
    } else {
        if record_projection_one(runtime, state, args[0], args[1], args[2], Op::Retract).is_err() {
            return mop_error(runtime, b"historyResourceLimit\0");
        }
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
        remove_mirror_facts(unsafe { state_mut(self_object) }, vtable, selector, old);
    }
    let api = unsafe { api(runtime) };
    // SAFETY: the tracked MOP ABI carries the C Method address in an Oop-sized
    // carrier; the IdApi method table stores the same pointer representation.
    let method: birnam_abi::Method = unsafe { core::mem::transmute(method_carrier) };
    unsafe { (api.method)(vtable, selector, method) };
    // The derived mirror is committed only after the authoritative dictionary
    // write has succeeded. This preserves retract -> dict -> assert ordering.
    append_mirror_fact(
        unsafe { state_mut(self_object) },
        vtable,
        selector,
        method_carrier,
    );
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
    if unsafe { birnam_abi::vtable_owner(api, vtable) }.is_null() {
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
        remove_mirror_facts(unsafe { state_mut(self_object) }, vtable, selector, old);
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
        current = unsafe { birnam_abi::vtable_parent(api, current) };
    }
    false
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
            // The MOP mirror is a rebuilt cache, not a durable Store
            // transition. Rebuilding it after image load must not advance the
            // durable basis or move a retention window merely because native
            // method dictionaries were re-enumerated.
            state.facts.push(Fact {
                e: vtable,
                a: selector,
                v: as_oop(runtime.primitive),
                tx: state.tx.saturating_sub(1),
                op: Op::Assert,
                historied: false,
                mirror: true,
            });
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
        remove_mirror_facts(state, vtable, selector, old);
    }
}

fn durable_image_fact(fact: &Fact, transient_attributes: &[birnam_abi::Oop]) -> bool {
    !fact.mirror && !transient_attributes.contains(&fact.a)
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
    let selector = unsafe { intern(api, b"e:a:v:tx:op:historied:\0") };
    let transient_marker = unsafe { intern(api, b"imageTransient\0") };
    let state = unsafe { state_mut(self_object) };
    let mut transient_attributes = Vec::new();
    for fact in &state.facts {
        if !transient_attributes.contains(&fact.a)
            && current_value_raw(state, fact.a, transient_marker) == api.trueObj
        {
            transient_attributes.push(fact.a);
        }
    }
    let facts = state.facts.clone();
    for fact in facts {
        // The MOP mirror is a rebuilt cache, not independently serialized. In
        // particular, method carriers may be stale raw code pointers and must
        // never reach the heap codec.
        if !durable_image_fact(&fact, &transient_attributes) {
            continue;
        }
        let arguments = [
            fact.e,
            fact.a,
            fact.v,
            unsafe { (birnam_abi::integer::make)(fact.tx as i64) },
            unsafe { (birnam_abi::integer::make)(fact.op as u8 as i64) },
            if fact.historied {
                api.trueObj
            } else {
                api.falseObj
            },
        ];
        // SAFETY: the collector owns the current six-argument image protocol.
        let _ = unsafe { (api.send)(collector, selector, arguments.as_ptr(), arguments.len()) };
    }
    self_object
}

unsafe extern "C" fn image_load_begin(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let state = unsafe { state_mut(self_object) };
    state.facts.clear();
    state.compensation_reservations.clear();
    state.oldest_retained_basis = 0;
    state.retention = RetentionPolicy::CurrentOnly;
    state.transitions_since_compaction = 0;
    self_object
}

unsafe extern "C" fn image_add_fact_current(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let args = unsafe { core::slice::from_raw_parts(args, 6) };
    let tx = unsafe { (birnam_abi::integer::value)(args[3]) } as u64;
    let op = match unsafe { (birnam_abi::integer::value)(args[4]) } {
        0 => Op::Assert,
        1 => Op::Retract,
        _ => abort("image fact op was not assert/retract"),
    };
    let historied = if args[5] == unsafe { api(runtime) }.trueObj {
        true
    } else if args[5] == unsafe { api(runtime) }.falseObj {
        false
    } else {
        abort("image fact historied bit was not Boolean");
    };
    unsafe { state_mut(self_object) }.facts.push(Fact {
        e: args[0],
        a: args[1],
        v: args[2],
        tx,
        op,
        historied,
        mirror: false,
    });
    self_object
}

unsafe extern "C" fn image_load_end_current(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let args = unsafe { core::slice::from_raw_parts(args, 5) };
    let tx = unsafe { (birnam_abi::integer::value)(args[0]) };
    let oldest = unsafe { (birnam_abi::integer::value)(args[2]) };
    let policy = unsafe { (birnam_abi::integer::value)(args[3]) };
    let argument = unsafe { (birnam_abi::integer::value)(args[4]) };
    if tx < 0 || oldest < 0 || oldest > tx || argument < 0 {
        abort("current image Store retention metadata was invalid");
    }
    let retention = match policy {
        0 if argument == 0 => RetentionPolicy::CurrentOnly,
        1 if argument <= tx => RetentionPolicy::KeepSince(argument as u64),
        2 => RetentionPolicy::KeepLast(argument as u64),
        _ => abort("current image Store retention policy was invalid"),
    };
    let state = unsafe { state_mut(self_object) };
    state.tx = tx as u64;
    state.raw = args[1] == unsafe { api(runtime) }.trueObj;
    state.oldest_retained_basis = oldest as u64;
    state.retention = retention;
    state.transitions_since_compaction = 0;
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
    unsafe { (birnam_abi::integer::make)(count as i64) }
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
    unsafe { (birnam_abi::integer::make)(count as i64) }
}

unsafe extern "C" fn attr_at(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let index = unsafe { (birnam_abi::integer::value)(args[1]) } as usize;
    let attributes = live_attributes(unsafe { state_mut(self_object) }, args[0], runtime);
    attributes.get(index).copied().unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn history(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let history_len = unsafe { state_mut(self_object) }
        .facts
        .iter()
        .filter(|fact| same(fact.e, args[0]) && same(fact.a, args[1]))
        .count();
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (birnam_abi::integer::make)(history_len as i64) }
}

unsafe extern "C" fn basis(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let tx = unsafe { state_mut(self_object) }.tx;
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (birnam_abi::integer::make)(tx as i64) }
}

unsafe extern "C" fn oldest_retained_basis(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let basis = unsafe { state_mut(self_object) }.oldest_retained_basis;
    unsafe { (birnam_abi::integer::make)(basis as i64) }
}

unsafe extern "C" fn retained_fact_count(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let count = unsafe { state_mut(self_object) }.facts.len();
    unsafe { (birnam_abi::integer::make)(count as i64) }
}

unsafe extern "C" fn retained_fact_bytes(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let bytes = semantic_fact_count(unsafe { state_mut(self_object) }) * size_of::<Fact>();
    unsafe { (birnam_abi::integer::make)(bytes as i64) }
}

unsafe extern "C" fn reserved_compensation_fact_count_method(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let count = reserved_compensation_fact_count(unsafe { state_mut(self_object) });
    unsafe { (birnam_abi::integer::make)(count as i64) }
}

unsafe extern "C" fn reserved_compensation_fact_bytes(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let bytes = reserved_compensation_fact_count(unsafe { state_mut(self_object) })
        .saturating_mul(size_of::<Fact>());
    unsafe { (birnam_abi::integer::make)(bytes as i64) }
}

unsafe extern "C" fn fact_bytes_limit(
    _self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    unsafe { (birnam_abi::integer::make)(runtime.fact_bytes_limit as i64) }
}

unsafe extern "C" fn fact_record_bytes(
    _self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    unsafe { (birnam_abi::integer::make)(size_of::<Fact>() as i64) }
}

unsafe extern "C" fn retention_policy_code(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let code = match unsafe { state_mut(self_object) }.retention {
        RetentionPolicy::CurrentOnly => 0,
        RetentionPolicy::KeepSince(_) => 1,
        RetentionPolicy::KeepLast(_) => 2,
    };
    unsafe { (birnam_abi::integer::make)(code) }
}

unsafe extern "C" fn retention_policy_argument(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let _runtime = runtime();
    let argument = match unsafe { state_mut(self_object) }.retention {
        RetentionPolicy::CurrentOnly => 0,
        RetentionPolicy::KeepSince(value) | RetentionPolicy::KeepLast(value) => value,
    };
    unsafe { (birnam_abi::integer::make)(argument as i64) }
}

unsafe extern "C" fn retain_current_only(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    unsafe { state_mut(self_object) }.retention = RetentionPolicy::CurrentOnly;
    self_object
}

unsafe extern "C" fn retain_since(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let candidate = unsafe { (birnam_abi::integer::value)(*args) };
    let state = unsafe { state_mut(self_object) };
    if candidate < 0
        || (candidate as u64) > state.tx
        || (candidate as u64) < state.oldest_retained_basis
    {
        return history_not_retained_error(runtime);
    }
    state.retention = RetentionPolicy::KeepSince(candidate as u64);
    self_object
}

unsafe extern "C" fn retain_last(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let count = unsafe { (birnam_abi::integer::value)(*args) };
    if count < 0 {
        return history_not_retained_error(runtime);
    }
    unsafe { state_mut(self_object) }.retention = RetentionPolicy::KeepLast(count as u64);
    self_object
}

unsafe extern "C" fn compact(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    let result = compact_state(
        unsafe { state_mut(self_object) },
        as_oop(runtime.cardinality),
        as_oop(runtime.history),
        as_oop(runtime.true_value),
        as_oop(runtime.one),
        as_oop(runtime.many),
    );
    match result {
        Ok(_) => self_object,
        Err(CompactError::Allocation) => mop_error(runtime, b"historyCompactionResourceLimit\0"),
    }
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

fn relation_history_available(
    state: &StoreState,
    entity: birnam_abi::Oop,
    attribute: birnam_abi::Oop,
    basis: u64,
    history_attribute: birnam_abi::Oop,
    true_value: birnam_abi::Oop,
) -> bool {
    if basis < state.oldest_retained_basis {
        return false;
    }
    if basis >= state.tx {
        return true;
    }
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.historied
            || !same(fact.e, entity)
            || !same(fact.a, attribute)
            || fact.tx < state.oldest_retained_basis
        {
            continue;
        }
        let recovery = state.facts.iter().skip(index + 1).find(|later| {
            (later.historied && same(later.e, entity) && same(later.a, attribute))
                || (later.historied
                    && same(later.e, attribute)
                    && same(later.a, history_attribute)
                    && same(later.v, true_value)
                    && later.op == Op::Assert)
        });
        if recovery.is_none_or(|later| basis <= later.tx) {
            return false;
        }
    }
    true
}

unsafe extern "C" fn value_of_as_of(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly three arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 3) };
    let basis = unsafe { (birnam_abi::integer::value)(args[2]) } as u64;
    let state = unsafe { state_mut(self_object) };
    if !relation_history_available(
        state,
        args[0],
        args[1],
        basis,
        as_oop(runtime.history),
        unsafe { api(runtime) }.trueObj,
    ) {
        return history_not_retained_error(runtime);
    }
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

#[cfg(test)]
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

fn cardinality_as_of_values(
    state: &StoreState,
    attribute: birnam_abi::Oop,
    hi: Option<u64>,
    cardinality_attribute: birnam_abi::Oop,
    one: birnam_abi::Oop,
) -> birnam_abi::Oop {
    let value = current_value_raw_as_of(state, attribute, cardinality_attribute, hi);
    if value.is_null() { one } else { value }
}

fn same_stream(a: &Fact, b: &Fact, many: bool) -> bool {
    same(a.e, b.e) && same(a.a, b.a) && (!many || same(a.v, b.v))
}

fn latest_stream_index(state: &StoreState, candidate: usize, hi: Option<u64>, many: bool) -> bool {
    let fact = &state.facts[candidate];
    !state
        .facts
        .iter()
        .enumerate()
        .skip(candidate + 1)
        .any(|(_, later)| visible(later, hi) && same_stream(fact, later, many))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactError {
    Allocation,
}

fn requested_retention_floor(state: &StoreState) -> u64 {
    let requested = match state.retention {
        RetentionPolicy::CurrentOnly => state.tx,
        RetentionPolicy::KeepSince(basis) => basis.min(state.tx),
        RetentionPolicy::KeepLast(count) => state.tx.saturating_sub(count),
    };
    requested.max(state.oldest_retained_basis)
}

fn compact_state(
    state: &mut StoreState,
    cardinality_attribute: birnam_abi::Oop,
    history_attribute: birnam_abi::Oop,
    true_value: birnam_abi::Oop,
    one: birnam_abi::Oop,
    many_value: birnam_abi::Oop,
) -> Result<usize, CompactError> {
    let floor = requested_retention_floor(state);
    let mut keep = Vec::new();
    keep.try_reserve_exact(state.facts.len())
        .map_err(|_| CompactError::Allocation)?;
    keep.resize(state.facts.len(), false);

    // Retained history is prospective: only transitions committed while the
    // relation was historied belong to the exact suffix.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.mirror || (fact.historied && fact.tx >= floor) {
            keep[index] = true;
        }
    }

    // A history-disabled interval remains an explicit unavailable interval.
    // Preserve its final projection until a retained history-enable boundary
    // or later historied transition makes that projection an exact baseline.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.historied || fact.tx < floor {
            continue;
        }
        let has_retained_recovery = state.facts.iter().enumerate().any(|(later_index, later)| {
            later_index > index
                && keep[later_index]
                && (same(later.e, fact.e) && same(later.a, fact.a)
                    || (same(later.e, fact.a)
                        && same(later.a, history_attribute)
                        && same(later.v, true_value)
                        && later.op == Op::Assert))
        });
        if has_retained_recovery {
            keep[index] = true;
        }
    }

    // Seed the first retained basis with the state immediately before it.
    // An absent stream needs no tombstone because no older assertion survives.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.tx >= floor || fact.op != Op::Assert {
            continue;
        }
        let cardinality =
            cardinality_as_of_values(state, fact.a, Some(floor), cardinality_attribute, one);
        let is_many = cardinality == many_value;
        if latest_stream_index(state, index, Some(floor), is_many) {
            keep[index] = true;
        }
    }

    // Preserve the current projection. A tombstone is only necessary when it
    // shadows an assertion retained by the two passes above.
    for (index, fact) in state.facts.iter().enumerate() {
        let cardinality = cardinality_as_of_values(state, fact.a, None, cardinality_attribute, one);
        let is_many = cardinality == many_value;
        if !latest_stream_index(state, index, None, is_many) {
            continue;
        }
        if fact.op == Op::Assert
            || state.facts.iter().enumerate().any(|(prior_index, prior)| {
                prior_index < index
                    && keep[prior_index]
                    && prior.op == Op::Assert
                    && same_stream(fact, prior, is_many)
            })
        {
            keep[index] = true;
        }
    }

    let retained = keep.iter().filter(|selected| **selected).count();
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(retained)
        .map_err(|_| CompactError::Allocation)?;
    replacement.extend(
        state
            .facts
            .iter()
            .zip(keep)
            .filter_map(|(fact, selected)| selected.then_some(*fact)),
    );

    let dropped = state.facts.len() - replacement.len();
    state.facts = replacement;
    state.oldest_retained_basis = floor;
    state.transitions_since_compaction = 0;
    Ok(dropped)
}

fn semantic_fact_count(state: &StoreState) -> usize {
    state.facts.iter().filter(|fact| !fact.mirror).count()
}

fn reserved_compensation_fact_count(state: &StoreState) -> usize {
    state
        .compensation_reservations
        .iter()
        .fold(0usize, |total, reservation| {
            total.saturating_add(reservation.facts)
        })
}

fn prepare_fact_capacity(
    runtime: &Runtime,
    state: &mut StoreState,
    additional_reserved: usize,
    pending_appends: usize,
) -> Result<(), CompactError> {
    let max_facts = runtime.fact_bytes_limit / size_of::<Fact>();
    let requested = semantic_fact_count(state)
        .saturating_add(reserved_compensation_fact_count(state))
        .saturating_add(additional_reserved)
        .saturating_add(pending_appends);
    let at_growth_gate = state.transitions_since_compaction >= runtime.compaction_transitions;
    if at_growth_gate || requested > max_facts {
        compact_state(
            state,
            as_oop(runtime.cardinality),
            as_oop(runtime.history),
            as_oop(runtime.true_value),
            as_oop(runtime.one),
            as_oop(runtime.many),
        )?;
    }
    let requested = semantic_fact_count(state)
        .saturating_add(reserved_compensation_fact_count(state))
        .saturating_add(additional_reserved)
        .saturating_add(pending_appends);
    (requested <= max_facts)
        .then_some(())
        .ok_or(CompactError::Allocation)
}

fn prepare_semantic_mutation(
    runtime: &Runtime,
    state: &mut StoreState,
    needs_append: bool,
) -> Result<(), CompactError> {
    prepare_fact_capacity(runtime, state, 0, usize::from(needs_append))
}

fn reserve_compensation_in(
    runtime: &Runtime,
    state: &mut StoreState,
    owner: usize,
    facts: usize,
) -> Result<(), CompactError> {
    if owner == 0 || facts == 0 {
        return Err(CompactError::Allocation);
    }
    // A reservation is made immediately before its candidate-owned write. In
    // addition to the compensation records, retain capacity for that forward
    // transition itself.
    prepare_fact_capacity(runtime, state, facts, 1)?;
    if let Some(reservation) = state
        .compensation_reservations
        .iter_mut()
        .find(|reservation| reservation.owner == owner)
    {
        let Some(total) = reservation.facts.checked_add(facts) else {
            return Err(CompactError::Allocation);
        };
        reservation.facts = total;
    } else {
        state
            .compensation_reservations
            .try_reserve(1)
            .map_err(|_| CompactError::Allocation)?;
        state
            .compensation_reservations
            .push(CompensationReservation { owner, facts });
    }
    Ok(())
}

fn consume_compensation_in(state: &mut StoreState, owner: usize, facts: usize) -> bool {
    if owner == 0 || facts == 0 {
        return false;
    }
    let Some(index) = state
        .compensation_reservations
        .iter()
        .position(|reservation| reservation.owner == owner)
    else {
        return false;
    };
    if state.compensation_reservations[index].facts < facts {
        return false;
    }
    state.compensation_reservations[index].facts -= facts;
    if state.compensation_reservations[index].facts == 0 {
        state.compensation_reservations.swap_remove(index);
    }
    true
}

unsafe extern "C" fn reserve_compensation(owner: usize, facts: usize) -> core::ffi::c_int {
    let runtime = runtime();
    let state = unsafe { state_mut(as_oop(runtime.store)) };
    i32::from(reserve_compensation_in(runtime, state, owner, facts).is_ok())
}

unsafe extern "C" fn consume_compensation(owner: usize, facts: usize) -> core::ffi::c_int {
    let state = unsafe { state_mut(as_oop(runtime().store)) };
    i32::from(consume_compensation_in(state, owner, facts))
}

unsafe extern "C" fn release_compensation(owner: usize) -> usize {
    let state = unsafe { state_mut(as_oop(runtime().store)) };
    let Some(index) = state
        .compensation_reservations
        .iter()
        .position(|reservation| reservation.owner == owner)
    else {
        return 0;
    };
    state.compensation_reservations.swap_remove(index).facts
}

unsafe extern "C" fn reserved_compensation(owner: usize) -> usize {
    unsafe { state_mut(as_oop(runtime().store)) }
        .compensation_reservations
        .iter()
        .find(|reservation| reservation.owner == owner)
        .map_or(0, |reservation| reservation.facts)
}

static STORE_COMPENSATION_API_V1: StoreCompensationApiV1 = StoreCompensationApiV1 {
    abi_version: 1,
    byte_size: size_of::<StoreCompensationApiV1>(),
    reserve: reserve_compensation,
    consume: consume_compensation,
    release: release_compensation,
    reserved: reserved_compensation,
};

/// Private host-mediated Store/Codegen resource-reservation seam. This table
/// is deliberately outside `IdApi`: source sends remain malleable while the
/// retained-history ceiling and rollback guarantee stay runtime invariants.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_store_compensation_api_v1() -> *const c_void {
    (&raw const STORE_COMPENSATION_API_V1).cast()
}

fn collect_live_triples(
    runtime: &Runtime,
    state: &StoreState,
    lo: Option<u64>,
    hi: Option<u64>,
) -> Vec<Triple> {
    // All indices are invocation-local and gone before the first foreign send.
    // Physical order matters: non-historied facts can be updated in place.
    let mut by_pair = std::collections::HashMap::new();
    let mut groups: Vec<Vec<&Fact>> = Vec::new();
    let mut previous = None;
    for fact in &state.facts {
        if !visible(fact, hi) {
            continue;
        }
        let pair = (fact.e, fact.a);
        let index = match previous {
            Some((key, index)) if key == pair => index,
            _ => {
                let next = groups.len();
                let index = *by_pair.entry(pair).or_insert_with(|| {
                    groups.push(Vec::new());
                    next
                });
                previous = Some((pair, index));
                index
            }
        };
        groups[index].push(fact);
    }

    let mut triples = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in &groups {
        let last = group.last().expect("nonempty visible fact group");
        let many = by_pair
            .get(&(last.a, as_oop(runtime.cardinality)))
            .and_then(|index| groups[*index].last())
            .is_some_and(|schema| schema.op == Op::Assert && schema.v == as_oop(runtime.many));
        seen.clear();
        for fact in group.iter().rev() {
            if many && !seen.insert(fact.v) {
                continue;
            }
            if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                triples.push(Triple {
                    e: fact.e,
                    a: fact.a,
                    v: fact.v,
                });
            }
            if !many {
                break;
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
    let hi = unsafe { (birnam_abi::integer::value)(args[0]) } as u64;
    let state = unsafe { state_mut(self_object) };
    if hi < state.oldest_retained_basis {
        return history_not_retained_error(runtime);
    }
    let triples = collect_live_triples(runtime, state, None, Some(hi));
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
    let lo = unsafe { (birnam_abi::integer::value)(args[0]) } as u64;
    let state = unsafe { state_mut(self_object) };
    if lo < state.oldest_retained_basis {
        return history_not_retained_error(runtime);
    }
    let triples = collect_live_triples(runtime, state, Some(lo), None);
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
    let lo = unsafe { (birnam_abi::integer::value)(args[0]) } as u64;
    let hi = unsafe { (birnam_abi::integer::value)(args[1]) } as u64;
    let state = unsafe { state_mut(self_object) };
    if lo < state.oldest_retained_basis || hi < state.oldest_retained_basis {
        return history_not_retained_error(runtime);
    }
    let triples = collect_live_triples(runtime, state, Some(lo), Some(hi));
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
    let result = unsafe { (api.send)(store, selector, arguments.as_ptr(), arguments.len()) };
    if result == store { self_object } else { result }
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
    let result = unsafe {
        (api.send)(
            as_oop(runtime.store),
            selector,
            arguments.as_ptr(),
            arguments.len(),
        )
    };
    if result == as_oop(runtime.store) {
        args[1]
    } else {
        result
    }
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
        (b"oldestRetainedBasis\0", oldest_retained_basis),
        (b"retainedFactCount\0", retained_fact_count),
        (b"retainedFactBytes\0", retained_fact_bytes),
        (
            b"reservedCompensationFactCount\0",
            reserved_compensation_fact_count_method,
        ),
        (
            b"reservedCompensationFactBytes\0",
            reserved_compensation_fact_bytes,
        ),
        (b"factBytesLimit\0", fact_bytes_limit),
        (b"factRecordBytes\0", fact_record_bytes),
        (b"retentionPolicyCode\0", retention_policy_code),
        (b"retentionPolicyArgument\0", retention_policy_argument),
        (b"retainCurrentOnly\0", retain_current_only),
        (b"retainSince:\0", retain_since),
        (b"retainLast:\0", retain_last),
        (b"compact\0", compact),
        (b"valueOf:attr:asOf:\0", value_of_as_of),
        (b"liveTriplesDo:\0", live_triples_do),
        (b"liveTriplesAsOf:do:\0", live_triples_as_of_do),
        (b"liveTriplesSince:do:\0", live_triples_since_do),
        (b"liveTriplesBetween:and:do:\0", live_triples_between_and_do),
        (b"cardinality:\0", cardinality_method),
        (b"imageFactsInto:\0", image_facts_into),
        (b"imageLoadBegin\0", image_load_begin),
        (
            b"imageAddFact:attr:value:tx:op:historied:\0",
            image_add_fact_current,
        ),
        (
            b"imageLoadEnd:raw:oldest:policy:argument:\0",
            image_load_end_current,
        ),
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
        // All three CellState words are object references. RAW preserved stale
        // native addresses in images instead of relocating store/entity/attr.
        (api.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_OOP);
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

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, STORE_NAME.as_ptr().cast()) != 0 };
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
        let compaction_transitions = positive_config(
            "BIRNAM_STORE_COMPACTION_TRANSITIONS",
            DEFAULT_COMPACTION_TRANSITIONS as usize,
        ) as u64;
        let fact_bytes_limit =
            positive_config("BIRNAM_STORE_FACT_BYTES_LIMIT", DEFAULT_FACT_BYTES_LIMIT);
        if fact_bytes_limit < size_of::<Fact>() * 4 {
            abort("BIRNAM_STORE_FACT_BYTES_LIMIT is too small for bootstrap state");
        }
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
            true_value: oop_address(api.trueObj),
            compaction_transitions,
            fact_bytes_limit,
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
    if unsafe { birnam_abi::namespace_presence::contains(presence, STORE_NAME.as_ptr().cast()) == 0 } {
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

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust Store module against the one process-owned IdApi table.
///
/// # Safety
///
/// `api` must be the live process-owned `IdApi` table supplied by the host, or
/// null to request the documented fatal boundary. `_state` is an opaque host
/// token and is not retained by this rung.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1) -> *mut c_void {
    // SAFETY: forwarded host contract; bring_up performs the null check and
    // retains only the process-owned table address.
    unsafe { bring_up(api, presence) };
    ptr::null_mut()
}

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod tests {
    use super::{
        Fact, Op, RUST_STORE_IDENTITY, RetentionPolicy, Runtime, StoreState, append_fact,
        birnam_store_rust_identity, compact_state, consume_compensation_in,
        current_value_raw_as_of, prepare_semantic_mutation, record_projection_one,
        relation_history_available, reserve_compensation_in, reserved_compensation_fact_count,
    };
    use birnam_abi::Oop;
    use core::ptr;
    use std::ffi::CStr;

    fn oop(value: usize) -> Oop {
        value as Oop
    }

    const CARDINALITY: usize = 10;
    const HISTORY: usize = 11;
    const ONE: usize = 12;
    const MANY: usize = 13;
    const TRUE: usize = 14;

    fn compact(state: &mut StoreState) -> usize {
        compact_state(
            state,
            oop(CARDINALITY),
            oop(HISTORY),
            oop(TRUE),
            oop(ONE),
            oop(MANY),
        )
        .unwrap()
    }

    pub(super) fn test_runtime(compaction_transitions: u64, fact_bytes_limit: usize) -> Runtime {
        Runtime {
            api: 0,
            store_vt: 0,
            store: 0,
            cell_vt: 0,
            globals_vt: 0,
            globals: 0,
            cardinality: CARDINALITY,
            one: ONE,
            many: MANY,
            history: HISTORY,
            primitive: 0,
            globals_entity: 0,
            true_value: TRUE,
            compaction_transitions,
            fact_bytes_limit,
        }
    }

    #[test]
    #[ignore = "manual Store scaling investigation; no timing thresholds"]
    fn snapshot_scaling_investigation() {
        use std::hint::black_box;
        use std::time::Instant;
        let runtime = test_runtime(u64::MAX, usize::MAX);
        for count in [100, 1000, 5000] {
            let mut state = StoreState::new();
            for index in 0..count {
                append_fact(
                    &mut state,
                    oop(100 + index),
                    oop(20),
                    oop(30),
                    Op::Assert,
                    true,
                );
            }
            let mut samples = Vec::new();
            for _ in 0..5 {
                let start = Instant::now();
                let triples = super::collect_live_triples(&runtime, black_box(&state), None, None);
                samples.push(start.elapsed().as_nanos());
                assert_eq!(triples.len(), count);
                for (index, triple) in triples.iter().enumerate() {
                    assert_eq!(triple.e, oop(100 + index));
                }
                black_box(triples);
            }
            samples.sort_unstable();
            eprintln!(
                "snapshot distinct_pairs={count} median_ns={} legacy_pair_comparisons={} legacy_fact_search_visits={} legacy_schema_search_visits={}",
                samples[2],
                count * (count - 1) / 2,
                count * (count + 1) / 2,
                count * count
            );
        }
    }

    #[test]
    fn core_identity_is_a_terminated_static_string() {
        // SAFETY: the export points to the crate's immutable NUL-terminated
        // static for the process lifetime.
        let identity = unsafe { CStr::from_ptr(birnam_store_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_STORE_IDENTITY);
    }

    #[test]
    fn nonhistoried_projection_shadows_but_does_not_destroy_predecessor() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, usize::MAX);
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        record_projection_one(&runtime, &mut state, oop(20), oop(21), oop(31), Op::Assert).unwrap();
        record_projection_one(&runtime, &mut state, oop(20), oop(21), oop(32), Op::Assert).unwrap();

        assert_eq!(state.facts.len(), 2);
        assert!(state.facts[0].historied);
        assert!(!state.facts[1].historied);
        assert_eq!(state.facts[1].v, oop(32));
        assert_eq!(state.facts[1].tx, 2);
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), None),
            oop(32)
        );
    }

    #[test]
    fn current_only_compaction_keeps_only_current_asserted_projection() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);
        append_fact(&mut state, oop(22), oop(21), oop(32), Op::Assert, true);
        append_fact(&mut state, oop(22), oop(21), oop(32), Op::Retract, true);

        assert_eq!(compact(&mut state), 3);
        assert_eq!(state.oldest_retained_basis, 4);
        assert_eq!(state.facts.len(), 1);
        assert_eq!(state.facts[0].v, oop(31));
        assert_eq!(
            current_value_raw_as_of(&state, oop(22), oop(21), None),
            ptr::null_mut()
        );
    }

    #[test]
    fn keep_since_preserves_baseline_and_exact_historied_suffix() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Retract, true);
        append_fact(&mut state, oop(20), oop(21), oop(32), Op::Assert, true);
        state.retention = RetentionPolicy::KeepSince(2);

        assert_eq!(compact(&mut state), 1);
        assert_eq!(state.oldest_retained_basis, 2);
        assert_eq!(
            state.facts.iter().map(|fact| fact.tx).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(2)),
            oop(31)
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(3)),
            ptr::null_mut()
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(4)),
            oop(32)
        );
    }

    #[test]
    fn prospective_history_keeps_an_unavailable_gap_until_reenabled() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, false);
        append_fact(
            &mut state,
            oop(21),
            oop(HISTORY),
            oop(TRUE),
            Op::Assert,
            true,
        );
        append_fact(&mut state, oop(20), oop(21), oop(32), Op::Assert, true);
        state.retention = RetentionPolicy::KeepSince(0);

        assert_eq!(compact(&mut state), 0);
        assert!(!relation_history_available(
            &state,
            oop(20),
            oop(21),
            2,
            oop(HISTORY),
            oop(TRUE)
        ));
        assert!(relation_history_available(
            &state,
            oop(20),
            oop(21),
            3,
            oop(HISTORY),
            oop(TRUE)
        ));
    }

    #[test]
    fn fact_carries_commit_time_history_status() {
        assert!(core::mem::size_of::<Fact>() >= core::mem::size_of::<[usize; 3]>() + 10);
    }

    #[test]
    fn method_dictionary_mirrors_are_explicitly_nondurable() {
        let semantic = Fact {
            e: oop(20),
            a: oop(21),
            v: oop(30),
            tx: 0,
            op: Op::Assert,
            historied: false,
            mirror: false,
        };
        let mirror = Fact {
            mirror: true,
            ..semantic
        };
        assert!(super::durable_image_fact(&semantic, &[]));
        assert!(!super::durable_image_fact(&mirror, &[]));
        assert!(!super::durable_image_fact(&semantic, &[oop(21)]));
    }

    #[test]
    fn growth_gate_keeps_current_only_history_at_a_plateau() {
        let mut state = StoreState::new();
        let runtime = test_runtime(8, core::mem::size_of::<Fact>() * 32);
        for value in 0..10_000usize {
            prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
            append_fact(
                &mut state,
                oop(20),
                oop(21),
                oop(100 + value),
                Op::Assert,
                true,
            );
        }
        // One baseline plus at most one trigger window of transitions.
        assert!(
            state.facts.len() <= 9,
            "retained {} facts",
            state.facts.len()
        );
        assert!(state.oldest_retained_basis >= 9_992);
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), None),
            oop(10_099)
        );
    }

    #[test]
    fn fixed_retention_floor_rejects_before_crossing_fact_ceiling() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, core::mem::size_of::<Fact>() * 4);
        state.retention = RetentionPolicy::KeepSince(0);
        for value in 0..4usize {
            prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
            append_fact(
                &mut state,
                oop(20),
                oop(21),
                oop(100 + value),
                Op::Assert,
                true,
            );
        }
        let basis = state.tx;
        assert!(prepare_semantic_mutation(&runtime, &mut state, true).is_err());
        assert_eq!(state.tx, basis);
        assert_eq!(state.facts.len(), 4);
    }

    #[test]
    fn compensation_reservation_charges_rollback_and_forward_capacity() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, core::mem::size_of::<Fact>() * 4);
        state.retention = RetentionPolicy::KeepSince(0);
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);

        reserve_compensation_in(&runtime, &mut state, 7, 2).unwrap();
        assert_eq!(reserved_compensation_fact_count(&state), 2);
        prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);

        // Two retained facts plus two rollback slots exhaust the four-fact
        // ceiling. A second candidate-owned forward write cannot begin.
        assert!(reserve_compensation_in(&runtime, &mut state, 7, 2).is_err());
        assert_eq!(reserved_compensation_fact_count(&state), 2);

        assert!(consume_compensation_in(&mut state, 7, 1));
        prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        assert_eq!(reserved_compensation_fact_count(&state), 1);
    }
}

birnam_abi::integer_dependency!();
