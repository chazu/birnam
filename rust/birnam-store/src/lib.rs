//! Rust Store rung: the first allocation-owning storage core.
//!
//! This slice keeps the public protocol deliberately small. It owns the
//! historied `(entity, attribute, value)` log and the schema-aware raw/flip,
//! assert, lookup, cardinality, count, and basis operations. MOP, image, cell,
//! and Globals policy remain in the Zig Store until their dedicated migration
//! slices; no Rust reference or lock crosses an IdApi callback.

use birnam_abi::IdApi;
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::sync::OnceLock;

const RUST_STORE_IDENTITY: &[u8] = b"rust-store-n2.2-core\0";
const STORE_NAME: &[u8] = b"Store\0";
const STORE_VT_NAME: &[u8] = b"StoreVt\0";
const CELL_VT_NAME: &[u8] = b"CellVt\0";
const GLOBALS_VT_NAME: &[u8] = b"GlobalsVt\0";
const GLOBALS_NAME: &[u8] = b"Globals\0";

#[derive(Clone, Copy)]
#[repr(C)]
struct Fact {
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    tx: u64,
    alive: bool,
}

#[repr(C)]
struct StoreState {
    facts: Vec<Fact>,
    raw: bool,
    tx: u64,
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

unsafe fn append_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) {
    state.tx = state.tx.saturating_add(1);
    state.facts.push(Fact {
        e,
        a,
        v,
        tx: state.tx,
        alive: true,
    });
}

fn current_value_raw(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
) -> birnam_abi::Oop {
    state
        .facts
        .iter()
        .rev()
        .find(|fact| fact.alive && same(fact.e, e) && same(fact.a, a))
        .map_or(ptr::null_mut(), |fact| fact.v)
}

fn count_values(state: &StoreState, e: birnam_abi::Oop, a: birnam_abi::Oop) -> usize {
    state
        .facts
        .iter()
        .filter(|fact| fact.alive && same(fact.e, e) && same(fact.a, a))
        .count()
}

fn cardinality(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> usize {
    current_value_raw(state, attribute, as_oop(runtime.cardinality)) as usize
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
        );
        append_fact(
            state,
            intern(api(runtime), b"name\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
        );
        append_fact(
            state,
            intern(api(runtime), b"history\0"),
            as_oop(runtime.cardinality),
            as_oop(runtime.one),
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
        if fact.alive {
            // SAFETY: the collector supplied the callback and each field is an
            // Oop carrier retained by the Store's fact log.
            unsafe { mark(fact.e) };
            unsafe { mark(fact.a) };
            unsafe { mark(fact.v) };
        }
    }
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
    unsafe { append_fact(state, args[0], args[1], args[2]) };
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
        unsafe { append_fact(state, args[0], args[1], args[2]) };
        return self_object;
    }
    let is_many = cardinality(runtime, state, args[1]) == runtime.many;
    if is_many {
        if state.facts.iter().any(|fact| {
            fact.alive && same(fact.e, args[0]) && same(fact.a, args[1]) && same(fact.v, args[2])
        }) {
            return self_object;
        }
    } else {
        let old = current_value_raw(state, args[0], args[1]);
        if same(old, args[2]) {
            return self_object;
        }
        for fact in &mut state.facts {
            if fact.alive && same(fact.e, args[0]) && same(fact.a, args[1]) {
                fact.alive = false;
            }
        }
    }
    unsafe { append_fact(state, args[0], args[1], args[2]) };
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

unsafe extern "C" fn has_value(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    let runtime = runtime();
    // SAFETY: this selector has exactly two arguments.
    let args = unsafe { core::slice::from_raw_parts(args, 2) };
    let present = !current_value_raw(unsafe { state_mut(self_object) }, args[0], args[1]).is_null();
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
    let count = count_values(unsafe { state_mut(self_object) }, args[0], args[1]);
    // SAFETY: the floor owns the returned boxed integer.
    unsafe { (api(runtime).mkSmallInt)(count as i64) }
}

unsafe extern "C" fn history(
    self_object: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    count_args: usize,
) -> birnam_abi::Oop {
    unsafe { count(self_object, args, count_args) }
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

unsafe extern "C" fn backfill(
    self_object: birnam_abi::Oop,
    _args: *const birnam_abi::Oop,
    _count: usize,
) -> birnam_abi::Oop {
    // MOP backfill is intentionally deferred to N2.3; this is an inert
    // protocol placeholder so storage-core tests can load the rung safely.
    self_object
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
        (b"hasValue:attr:\0", has_value),
        (b"count:attr:\0", count),
        (b"history:attr:\0", history),
        (b"basis\0", basis),
        (b"cardinality:\0", cardinality_method),
        (b"new\0", new_store),
        (b"backfillMirror\0", backfill),
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
        }
    });
    unsafe { register_methods(&runtime) };
    let store = as_oop(runtime.store);
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
