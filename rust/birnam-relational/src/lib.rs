//! Authority-free relational query kernel.
//!
//! This first R4 slice evaluates a left-to-right conjunction over a borrowed
//! Store snapshot. It owns neither facts nor IdApi objects: the caller supplies
//! the current (or historical) live triples, and receives projected bindings.
//! The dynamic module/Store callback wrapper is deliberately still a probe;
//! canonical `findAll:where:` installation remains a later slice.

pub mod oop;

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

const RUST_RELATIONAL_IDENTITY: &[u8] = b"rust-relational-r4.5-project\0";
static API: OnceLock<usize> = OnceLock::new();
static ACTIVE_PROBE_BLOCK: AtomicUsize = AtomicUsize::new(0);
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Relational: {message}");
    std::process::abort();
}

unsafe fn bring_up(api: *mut IdApi) {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    let address = api as usize;
    match API.get() {
        Some(existing) if *existing != address => abort("IdApi was replaced"),
        Some(_) => {}
        None => {
            let _ = API.set(address);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClassifiedValue {
    pub tag: u8,
    pub reserved: [u8; 7],
    pub integer: i64,
    pub float: f64,
    pub identity: u64,
    pub byte_len: usize,
}

const _: () = assert!(size_of::<ClassifiedValue>() == 40);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClassifiedTriple {
    pub entity: ClassifiedValue,
    pub attribute: ClassifiedValue,
    pub value: ClassifiedValue,
}

const _: () = assert!(size_of::<ClassifiedTriple>() == 120);

fn record(value: Value) -> ClassifiedValue {
    let mut record = ClassifiedValue {
        tag: 0,
        reserved: [0; 7],
        integer: 0,
        float: 0.0,
        identity: 0,
        byte_len: 0,
    };
    match value {
        Value::Identity(identity) => record.identity = identity,
        Value::Integer(integer) => {
            record.tag = 1;
            record.integer = integer;
        }
        Value::Float(float) => {
            record.tag = 2;
            record.float = float;
        }
        Value::String(string) => {
            record.tag = 3;
            record.byte_len = string.len();
        }
    }
    record
}

unsafe extern "C" fn mark_probe_root(mark: MarkFn) {
    let block = ACTIVE_PROBE_BLOCK.load(Ordering::Acquire);
    if block != 0 {
        // SAFETY: the callback block is allocated by the live IdApi and remains
        // active until the synchronous Store send returns.
        unsafe { mark(block as Oop) };
    }
}

fn register_probe_root(api: &IdApi) {
    ROOT_REGISTERED.get_or_init(|| unsafe {
        // SAFETY: the process floor owns this root-provider registration for the
        // lifetime of the loaded Rust Relational module.
        (api.gcRegisterRootProvider)(mark_probe_root);
    });
}

/// Identifies the explicit Rust Relational probe; it is not canonical yet.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_relational_rust_identity() -> *const c_char {
    RUST_RELATIONAL_IDENTITY.as_ptr().cast()
}

/// Classifies one floor object into the pointer-free probe record.
///
/// Return values: `0` success, `-1` null output, `-2` before `module_init`,
/// `-3` malformed/unsupported object, and `-4` output byte capacity too small.
///
/// # Safety
///
/// `object` must be a live Oop for the duration of the call. On success,
/// `output` must point to writable `ClassifiedValue` storage and `bytes` must
/// point to `capacity` writable bytes when a nonempty String is classified.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_classify(
    object: Oop,
    output: *mut ClassifiedValue,
    bytes: *mut u8,
    capacity: usize,
) -> i32 {
    if output.is_null() {
        return -1;
    }
    let Some(api_address) = API.get() else {
        return -2;
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(*api_address as *const IdApi) };
    // SAFETY: object is borrowed for this call under the module probe contract.
    let Some(value) = (unsafe { oop::classify(api, object) }) else {
        return -3;
    };
    match &value {
        Value::Identity(_) | Value::Integer(_) | Value::Float(_) => {}
        Value::String(string) => {
            if string.len() > capacity || (!string.is_empty() && bytes.is_null()) {
                return -4;
            }
            // SAFETY: the caller supplied a writable buffer of `capacity` bytes;
            // the length check above bounds this copy.
            unsafe { ptr::copy_nonoverlapping(string.as_ptr(), bytes, string.len()) };
        }
    }
    let record = record(value);
    // SAFETY: output is non-null and points to caller-owned storage for the
    // frozen record layout.
    unsafe { ptr::write(output, record) };
    0
}

struct ProbeCollector {
    api: *const IdApi,
    output: *mut ClassifiedTriple,
    capacity: usize,
    written: usize,
    status: i32,
}

struct ProjectCollector {
    api: *const IdApi,
    facts: Vec<Triple>,
    status: i32,
}

/// One three-position Store pattern for the explicit relational migration
/// probes. A null position is a variable identified by its position (0, 1, or
/// 2); non-null positions are classified constants.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProjectPattern {
    pub entity: Oop,
    pub attribute: Oop,
    pub value: Oop,
}

unsafe extern "C" fn collect_live_triple(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    if self_object.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the callback vtable has one raw payload slot containing the
    // synchronous collector address installed by collect_live.
    let collector_pointer = unsafe { *(self_object.cast::<*mut ProbeCollector>()) };
    if collector_pointer.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: collect_live keeps this stack collector alive until Store's send
    // and every callback have returned.
    let collector = unsafe { &mut *collector_pointer };
    if collector.status != 0 {
        return unsafe { (*collector.api).nilObj };
    }
    // SAFETY: the frozen Store callback supplies exactly three readable Oops.
    if count != 3 {
        collector.status = -3;
        return unsafe { (*collector.api).nilObj };
    }
    let Some(args) = (unsafe { borrowed_oop_args(args, count) }) else {
        collector.status = -3;
        return unsafe { (*collector.api).nilObj };
    };
    if collector.written >= collector.capacity {
        collector.status = -5;
        return unsafe { (*collector.api).nilObj };
    }
    // SAFETY: module_init records the process-owned IdApi for the module life.
    let api = unsafe { &*collector.api };
    let Some(entity) = (unsafe { oop::classify(api, args[0]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    let Some(attribute) = (unsafe { oop::classify(api, args[1]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    let Some(value) = (unsafe { oop::classify(api, args[2]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    // SAFETY: collect_live validates output/capacity before entering Store.
    unsafe {
        ptr::write(
            collector.output.add(collector.written),
            ClassifiedTriple {
                entity: record(entity),
                attribute: record(attribute),
                value: record(value),
            },
        );
    }
    collector.written += 1;
    api.nilObj
}

unsafe extern "C" fn collect_project_triple(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if self_object.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the callback vtable has one raw payload slot containing the
    // synchronous collector address installed by project_live.
    let collector_pointer = unsafe { *(self_object.cast::<*mut ProjectCollector>()) };
    if collector_pointer.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: project_live keeps this stack collector alive until Store's send
    // and every callback have returned.
    let collector = unsafe { &mut *collector_pointer };
    if collector.status != 0 {
        return unsafe { (*collector.api).nilObj };
    }
    if count != 3 {
        collector.status = -3;
        return unsafe { (*collector.api).nilObj };
    }
    let Some(args) = (unsafe { borrowed_oop_args(args, count) }) else {
        collector.status = -3;
        return unsafe { (*collector.api).nilObj };
    };
    // Keep the bridge bounded even when a caller supplies a tiny result
    // buffer: the Store remains authoritative, but a malformed/unbounded
    // callback stream cannot grow a Rust-side snapshot without limit.
    const MAX_PROJECT_FACTS: usize = 1_000_000;
    if collector.facts.len() >= MAX_PROJECT_FACTS {
        collector.status = -5;
        return unsafe { (*collector.api).nilObj };
    }
    // SAFETY: module_init records the process-owned IdApi for the module life.
    let api = unsafe { &*collector.api };
    let Some(entity) = (unsafe { oop::classify(api, args[0]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    let Some(attribute) = (unsafe { oop::classify(api, args[1]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    let Some(value) = (unsafe { oop::classify(api, args[2]) }) else {
        collector.status = -4;
        return api.nilObj;
    };
    collector.facts.push(Triple {
        entity,
        attribute,
        value,
    });
    api.nilObj
}

unsafe fn collect_window(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
    output: *mut ClassifiedTriple,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) || store.is_null() {
        return -3;
    }
    let Some(api_address) = API.get() else {
        return -2;
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(*api_address as *const IdApi) };
    let callback_vt = unsafe { (api.proto)(api.object_vt) };
    if callback_vt.is_null() {
        return -6;
    }
    let callback_selector = unsafe { (api.intern)(c"value:value:value:".as_ptr()) };
    // SAFETY: the callback object has one raw payload slot holding a stack
    // collector address and never exposes a Rust reference to the floor.
    unsafe {
        (api.setLayout)(callback_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.method)(callback_vt, callback_selector, collect_live_triple);
    }
    let mut collector = ProbeCollector {
        api: api as *const IdApi,
        output,
        capacity,
        written: 0,
        status: 0,
    };
    let callback = unsafe { (api.alloc)(callback_vt, size_of::<*mut ProbeCollector>()) };
    if callback.is_null() {
        return -6;
    }
    // SAFETY: callback payload was allocated with exactly one raw pointer slot.
    unsafe { ptr::write(callback.cast::<*mut ProbeCollector>(), &mut collector) };
    ACTIVE_PROBE_BLOCK.store(callback as usize, Ordering::Release);
    let selector_name: &[u8] = match mode {
        0 => b"liveTriplesDo:\0",
        1 => b"liveTriplesAsOf:do:\0",
        2 => b"liveTriplesSince:do:\0",
        3 => b"liveTriplesBetween:and:do:\0",
        _ => {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return -7;
        }
    };
    if (mode == 1 || mode == 2 || mode == 3) && (lower > i64::MAX as u64 || upper > i64::MAX as u64)
    {
        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
        return -7;
    }
    let selector = unsafe { (api.intern)(selector_name.as_ptr().cast()) };
    let lower_oop = unsafe { (api.mkSmallInt)(lower as i64) };
    let upper_oop = unsafe { (api.mkSmallInt)(upper as i64) };
    let arguments: [Oop; 3] = match mode {
        0 => [callback, ptr::null_mut(), ptr::null_mut()],
        1 => [upper_oop, callback, ptr::null_mut()],
        2 => [lower_oop, callback, ptr::null_mut()],
        3 => [lower_oop, upper_oop, callback],
        _ => unreachable!(),
    };
    let argument_count = match mode {
        0 => 1,
        1 | 2 => 2,
        3 => 3,
        _ => unreachable!(),
    };
    // SAFETY: Store owns the frozen liveTriplesDo: selector and synchronously
    // invokes value:value:value: on the callback object.
    unsafe { (api.send)(store, selector, arguments.as_ptr(), argument_count) };
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    unsafe { *written = collector.written };
    collector.status
}

unsafe fn collect_project_facts(store: Oop) -> Result<(Vec<Triple>, *const IdApi), i32> {
    if store.is_null() {
        return Err(-3);
    }
    let Some(api_address) = API.get() else {
        return Err(-2);
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(*api_address as *const IdApi) };
    let callback_vt = unsafe { (api.proto)(api.object_vt) };
    if callback_vt.is_null() {
        return Err(-6);
    }
    let callback_selector = unsafe { (api.intern)(c"value:value:value:".as_ptr()) };
    // SAFETY: the callback object has one raw payload slot holding a stack
    // collector address and never exposes a Rust reference to the floor.
    unsafe {
        (api.setLayout)(callback_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.method)(callback_vt, callback_selector, collect_project_triple);
    }
    let mut collector = ProjectCollector {
        api: api as *const IdApi,
        facts: Vec::new(),
        status: 0,
    };
    let callback = unsafe { (api.alloc)(callback_vt, size_of::<*mut ProjectCollector>()) };
    if callback.is_null() {
        return Err(-6);
    }
    // SAFETY: callback payload was allocated with exactly one raw pointer slot.
    unsafe { ptr::write(callback.cast::<*mut ProjectCollector>(), &mut collector) };
    ACTIVE_PROBE_BLOCK.store(callback as usize, Ordering::Release);
    let selector = unsafe { (api.intern)(c"liveTriplesDo:".as_ptr()) };
    let arguments = [callback];
    // SAFETY: Store owns the frozen liveTriplesDo: selector and synchronously
    // invokes value:value:value: on the callback object.
    unsafe { (api.send)(store, selector, arguments.as_ptr(), arguments.len()) };
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    if collector.status != 0 {
        return Err(collector.status);
    }
    Ok((collector.facts, collector.api))
}

unsafe fn project_terms(api: &IdApi, pattern: ProjectPattern) -> Result<[Term; 3], i32> {
    let term = |object: Oop, variable: u32| -> Result<Term, i32> {
        if object.is_null() {
            return Ok(Term::Variable(variable));
        }
        // SAFETY: pattern objects are borrowed from the caller for this
        // synchronous call.
        unsafe { oop::classify(api, object) }
            .map(Term::Constant)
            .ok_or(-4)
    };
    Ok([
        term(pattern.entity, 0)?,
        term(pattern.attribute, 1)?,
        term(pattern.value, 2)?,
    ])
}

#[allow(clippy::too_many_arguments)]
unsafe fn project_live(
    store: Oop,
    projection: u8,
    entity: Oop,
    attribute: Oop,
    value: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) {
        return -3;
    }
    if projection > 2 {
        return -7;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };

    let terms = match unsafe {
        project_terms(
            api,
            ProjectPattern {
                entity,
                attribute,
                value,
            },
        )
    } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let projected = project(&facts, &[Clause(terms)], projection as u32);
    if projected.len() > capacity {
        return -5;
    }
    // SAFETY: the caller supplied capacity elements of writable output storage;
    // the length check above bounds every write.
    let projected_len = projected.len();
    for (slot, value) in projected.iter().cloned().enumerate() {
        unsafe { ptr::write(output.add(slot), record(value)) };
    }
    // `written` is the number of projected values, not the callback snapshot.
    unsafe { *written = projected_len };
    0
}

unsafe fn project_join_live(
    store: Oop,
    projection: u8,
    first: ProjectPattern,
    second: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) {
        return -3;
    }
    if projection > 2 {
        return -7;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let first_terms = match unsafe { project_terms(api, first) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let second_terms = match unsafe { project_terms(api, second) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let projected = project(
        &facts,
        &[Clause(first_terms), Clause(second_terms)],
        projection as u32,
    );
    if projected.len() > capacity {
        return -5;
    }
    let projected_len = projected.len();
    for (slot, value) in projected.iter().cloned().enumerate() {
        // SAFETY: the caller supplied capacity elements of writable output
        // storage; the length check above bounds every write.
        unsafe { ptr::write(output.add(slot), record(value)) };
    }
    // `written` is the number of projected values, not the callback snapshot.
    unsafe { *written = projected_len };
    0
}

#[allow(clippy::too_many_arguments)]
unsafe fn project_predicate_live(
    store: Oop,
    projection: u8,
    pattern: ProjectPattern,
    predicate_position: u8,
    operator: u8,
    rhs: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) {
        return -3;
    }
    if projection > 2 || predicate_position > 2 {
        return -7;
    }
    let Some(op) = CompareOp::from_u8(operator) else {
        return -7;
    };
    if rhs.is_null() {
        return -4;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let terms = match unsafe { project_terms(api, pattern) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let predicate_variable = match &terms[predicate_position as usize] {
        Term::Variable(variable) => *variable,
        Term::Constant(_) => return -7,
    };
    // SAFETY: rhs is borrowed from the caller for this synchronous call.
    let Some(rhs) = (unsafe { oop::classify(api, rhs) }) else {
        return -4;
    };
    let projected = project_predicate(
        &facts,
        &Clause(terms),
        predicate_variable,
        op,
        &rhs,
        projection as u32,
    );
    if projected.len() > capacity {
        return -5;
    }
    let projected_len = projected.len();
    for (slot, value) in projected.iter().cloned().enumerate() {
        // SAFETY: the caller supplied capacity elements of writable output
        // storage; the length check above bounds every write.
        unsafe { ptr::write(output.add(slot), record(value)) };
    }
    unsafe { *written = projected_len };
    0
}

unsafe fn project_negated_live(
    store: Oop,
    projection: u8,
    positive: ProjectPattern,
    negative: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) {
        return -3;
    }
    if projection > 2 {
        return -7;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let positive_terms = match unsafe { project_terms(api, positive) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let negative_terms = match unsafe { project_terms(api, negative) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let projected = project_negated(
        &facts,
        &Clause(positive_terms),
        &Clause(negative_terms),
        projection as u32,
    );
    if projected.len() > capacity {
        return -5;
    }
    let projected_len = projected.len();
    for (slot, value) in projected.iter().cloned().enumerate() {
        // SAFETY: the caller supplied capacity elements of writable output
        // storage; the length check above bounds every write.
        unsafe { ptr::write(output.add(slot), record(value)) };
    }
    unsafe { *written = projected_len };
    0
}

/// Enumerate the current Store snapshot through the frozen callback protocol.
///
/// Return values: `0` success, `-2` before `module_init`, `-3` invalid Store or
/// output pointers, `-4` an unsupported/malformed Oop, `-5` insufficient output
/// capacity, and `-6` failure to allocate the temporary callback object.
///
/// # Safety
///
/// `store` must be a live Store object whose `liveTriplesDo:` callback follows
/// the frozen IdApi protocol. `output` and `written` must point to caller-owned
/// writable storage for the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_collect_live(
    store: Oop,
    output: *mut ClassifiedTriple,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe { collect_window(store, 0, 0, 0, output, capacity, written) }
}

/// Enumerate one Store transaction window: mode `0` is current, `1` is as-of
/// `upper`, `2` is since `lower`, and `3` is between `[lower, upper)`.
///
/// Return values are the same as `birnam_relational_rust_collect_live`, with
/// `-7` additionally rejecting an unknown mode or a transaction outside the
/// SmallInt range.
///
/// # Safety
///
/// `store` must be a live Store object implementing the frozen window selectors.
/// `output` and `written` must point to caller-owned writable storage for the
/// declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_collect_window(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
    output: *mut ClassifiedTriple,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe { collect_window(store, mode, lower, upper, output, capacity, written) }
}

/// Project one triple-pattern variable from the current Store snapshot. A null
/// term is a variable (entity/attribute/value positions use variable ids 0/1/2);
/// non-null terms are classified constants. This is an explicit migration probe,
/// not yet the canonical Store `findAll:where:` method.
///
/// Return values: `0` success, `-2` before `module_init`, `-3` invalid pointers
/// or Store, `-4` malformed terms, `-5` insufficient output capacity, `-6`
/// callback allocation failure, and `-7` invalid projection index.
///
/// # Safety
///
/// `store` and any non-null term must remain live for the synchronous call.
/// `output` and `written` must point to caller-owned writable storage for the
/// declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_live(
    store: Oop,
    projection: u8,
    entity: Oop,
    attribute: Oop,
    value: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe {
        project_live(
            store, projection, entity, attribute, value, output, capacity, written,
        )
    }
}

/// Project one variable across two triple patterns from the current Store
/// snapshot. Null positions are variables identified by their position, so a
/// null entity in both patterns joins on the same variable 0. Non-null terms are
/// classified constants. This remains an explicit migration probe, not the
/// canonical Store `findAll:where:` method.
///
/// # Safety
///
/// `store` and any non-null pattern term must remain live for the synchronous
/// call. `output` and `written` must point to caller-owned writable storage for
/// the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_join_live(
    store: Oop,
    projection: u8,
    first: ProjectPattern,
    second: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe { project_join_live(store, projection, first, second, output, capacity, written) }
}

/// Project one variable from a triple pattern after a numeric predicate on one
/// of its variable positions. The predicate has a constant right operand and
/// runs after the single binder; this is an explicit migration probe, not the
/// canonical Store `findAll:where:` method.
///
/// Return values: `0` success, `-2` before `module_init`, `-3` invalid pointers
/// or Store, `-4` malformed terms or right operand, `-5` insufficient output
/// capacity, `-6` callback allocation failure, and `-7` invalid projection,
/// predicate position/operator, or a predicate position that is constant.
/// Operators are `0 <`, `1 <=`, `2 >`, `3 >=`, `4 =`, and `5 ~=`, with the
/// frozen Prelude numeric promotion/equality law.
///
/// # Safety
///
/// `store`, any non-null pattern term, and `rhs` must remain live for the
/// synchronous call. `output` and `written` must point to caller-owned writable
/// storage for the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_predicate_live(
    store: Oop,
    projection: u8,
    pattern: ProjectPattern,
    predicate_position: u8,
    operator: u8,
    rhs: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe {
        project_predicate_live(
            store,
            projection,
            pattern,
            predicate_position,
            operator,
            rhs,
            output,
            capacity,
            written,
        )
    }
}

/// Project one variable from a positive triple pattern while excluding rows
/// for which a second pattern matches. Null positions are position-based
/// variables shared between the patterns; variables that occur only in the
/// negative pattern are scoped to its existence check and never bind output.
/// This is an explicit migration probe, not the canonical Store
/// `findAll:where:` method.
///
/// Return values: `0` success, `-2` before `module_init`, `-3` invalid pointers
/// or Store, `-4` malformed terms, `-5` insufficient output capacity, `-6`
/// callback allocation failure, and `-7` invalid projection index.
///
/// # Safety
///
/// `store` and any non-null pattern term must remain live for the synchronous
/// call. `output` and `written` must point to caller-owned writable storage for
/// the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_negated_live(
    store: Oop,
    projection: u8,
    positive: ProjectPattern,
    negative: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe {
        project_negated_live(
            store, projection, positive, negative, output, capacity, written,
        )
    }
}

/// Initialize the explicit Rust Relational probe against the process IdApi.
///
/// # Safety
///
/// `api` must be the process-owned IdApi table supplied by the host, and the
/// host must keep that table and its floor dylib alive for the module lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    // SAFETY: forwarded frozen host initialization contract.
    unsafe { bring_up(api) };
    // SAFETY: module_init received the same process-owned IdApi recorded above.
    register_probe_root(unsafe { &*api });
    ptr::null_mut()
}

/// A value classified by the IdApi adapter before entering the query kernel.
/// Identity values compare by pointer identity; scalar values compare by value,
/// matching the Zig relational `valEq` law.
#[derive(Clone, Debug)]
pub enum Value {
    Identity(u64),
    Integer(i64),
    Float(f64),
    String(Vec<u8>),
}

impl Value {
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Identity(left), Self::Identity(right)) => left == right,
            (Self::Integer(left), Self::Integer(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            _ => false,
        }
    }
}

/// A copied triple view supplied by the Store callback seam.
#[derive(Clone, Debug)]
pub struct Triple {
    pub entity: Value,
    pub attribute: Value,
    pub value: Value,
}

/// A clause term.
#[derive(Clone, Debug)]
pub enum Term {
    Constant(Value),
    Variable(u32),
}

#[derive(Clone, Debug)]
pub struct Clause(pub [Term; 3]);

#[derive(Clone, Debug)]
pub struct Binding {
    pub variable: u32,
    pub value: Value,
}

/// The bounded comparison vocabulary used by the predicate migration probe.
/// Ordering follows the Prelude numeric law: Integer/Float pairs promote to
/// Float for ordering, while equality remains type-strict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CompareOp {
    Less = 0,
    LessEqual = 1,
    Greater = 2,
    GreaterEqual = 3,
    Equal = 4,
    NotEqual = 5,
}

impl CompareOp {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Less,
            1 => Self::LessEqual,
            2 => Self::Greater,
            3 => Self::GreaterEqual,
            4 => Self::Equal,
            5 => Self::NotEqual,
            _ => return None,
        })
    }
}

impl Value {
    /// Apply one Prelude-shaped comparison without sending a borrowed Oop
    /// across the Rust boundary. Unknown/non-numeric ordering operands fail the
    /// row, matching the relational filter's error-to-no-row behavior.
    pub fn compares(&self, op: CompareOp, other: &Self) -> bool {
        match op {
            CompareOp::Equal => self.matches(other),
            CompareOp::NotEqual => !self.matches(other),
            CompareOp::Less
            | CompareOp::LessEqual
            | CompareOp::Greater
            | CompareOp::GreaterEqual => {
                let ordering = match (self, other) {
                    (Self::Integer(left), Self::Integer(right)) => left.partial_cmp(right),
                    (Self::Integer(left), Self::Float(right)) => (*left as f64).partial_cmp(right),
                    (Self::Float(left), Self::Integer(right)) => left.partial_cmp(&(*right as f64)),
                    (Self::Float(left), Self::Float(right)) => left.partial_cmp(right),
                    _ => None,
                };
                matches!(
                    (op, ordering),
                    (CompareOp::Less, Some(std::cmp::Ordering::Less))
                        | (CompareOp::LessEqual, Some(std::cmp::Ordering::Less))
                        | (CompareOp::LessEqual, Some(std::cmp::Ordering::Equal))
                        | (CompareOp::Greater, Some(std::cmp::Ordering::Greater))
                        | (CompareOp::GreaterEqual, Some(std::cmp::Ordering::Greater))
                        | (CompareOp::GreaterEqual, Some(std::cmp::Ordering::Equal))
                )
            }
        }
    }
}

fn lookup(solution: &[Binding], variable: u32) -> Option<&Value> {
    solution
        .iter()
        .find(|binding| binding.variable == variable)
        .map(|binding| &binding.value)
}

fn match_term(solution: &[Binding], fresh: &mut Vec<Binding>, term: &Term, value: &Value) -> bool {
    match term {
        Term::Constant(expected) => expected.matches(value),
        Term::Variable(variable) => {
            let bound = lookup(solution, *variable).or_else(|| lookup(fresh, *variable));
            match bound {
                Some(expected) => expected.matches(value),
                None => {
                    fresh.push(Binding {
                        variable: *variable,
                        value: value.clone(),
                    });
                    true
                }
            }
        }
    }
}

fn evaluate_clauses(facts: &[Triple], clauses: &[Clause]) -> Vec<Vec<Binding>> {
    let mut solutions: Vec<Vec<Binding>> = vec![Vec::new()];
    for Clause(terms) in clauses {
        let mut next = Vec::new();
        for solution in &solutions {
            for triple in facts {
                let values = [&triple.entity, &triple.attribute, &triple.value];
                let mut fresh = Vec::new();
                if terms
                    .iter()
                    .zip(values)
                    .all(|(term, value)| match_term(solution, &mut fresh, term, value))
                {
                    let mut extended = solution.clone();
                    extended.extend(fresh);
                    next.push(extended);
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

fn project_solutions(solutions: Vec<Vec<Binding>>, result: u32) -> Vec<Value> {
    let mut result_values: Vec<Value> = Vec::new();
    for solution in solutions {
        let Some(value) = lookup(&solution, result) else {
            continue;
        };
        if !result_values.iter().any(|seen| seen.matches(value)) {
            result_values.push(value.clone());
        }
    }
    result_values
}

/// Evaluate a left-to-right conjunction and project one variable.
///
/// Fact order is preserved and projected values are set-like: the first
/// occurrence wins, matching the existing relational surface's deterministic
/// result order and distinct projection semantics. No query result borrows the
/// input snapshot.
pub fn project(facts: &[Triple], clauses: &[Clause], result: u32) -> Vec<Value> {
    project_solutions(evaluate_clauses(facts, clauses), result)
}

/// Evaluate one triple clause, filter its bound variable with a comparison,
/// and project another variable. This is deliberately narrower than the
/// canonical query protocol: the predicate is constant-rhs, runs after the
/// single binder, and does not reorder or bind solutions.
pub fn project_predicate(
    facts: &[Triple],
    clause: &Clause,
    predicate_variable: u32,
    op: CompareOp,
    rhs: &Value,
    result: u32,
) -> Vec<Value> {
    let solutions = evaluate_clauses(facts, std::slice::from_ref(clause));
    let filtered = solutions.into_iter().filter(|solution| {
        lookup(solution, predicate_variable).is_some_and(|left| left.compares(op, rhs))
    });
    project_solutions(filtered.collect(), result)
}

fn clause_matches_solution(facts: &[Triple], solution: &[Binding], clause: &Clause) -> bool {
    let Clause(terms) = clause;
    facts.iter().any(|triple| {
        let values = [&triple.entity, &triple.attribute, &triple.value];
        let mut fresh = Vec::new();
        terms
            .iter()
            .zip(values)
            .all(|(term, value)| match_term(solution, &mut fresh, term, value))
    })
}

/// Evaluate one positive binder, then apply one negation-as-failure clause.
/// Variables already present in the positive solution are shared by position;
/// variables introduced only by the negative clause are scoped to its existence
/// check and never escape into the projected result.
pub fn project_negated(
    facts: &[Triple],
    positive: &Clause,
    negative: &Clause,
    result: u32,
) -> Vec<Value> {
    let solutions = evaluate_clauses(facts, std::slice::from_ref(positive));
    let kept = solutions
        .into_iter()
        .filter(|solution| !clause_matches_solution(facts, solution, negative));
    project_solutions(kept.collect(), result)
}

#[cfg(test)]
mod tests {
    use super::{
        Clause, CompareOp, Term, Triple, Value, project, project_negated, project_predicate,
    };

    fn id(value: u64) -> Value {
        Value::Identity(value)
    }

    fn integer(value: i64) -> Value {
        Value::Integer(value)
    }

    fn float(value: f64) -> Value {
        Value::Float(value)
    }

    fn fact(entity: u64, attribute: u64, value: u64) -> Triple {
        Triple {
            entity: id(entity),
            attribute: id(attribute),
            value: id(value),
        }
    }

    #[test]
    fn single_clause_projects_distinct_values_in_store_order() {
        let facts = [fact(1, 10, 100), fact(2, 10, 100), fact(3, 10, 200)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(1),
        ])];

        let values = project(&facts, &clauses, 1);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(100)));
        assert!(values[1].matches(&id(200)));
    }

    #[test]
    fn conjunction_joins_on_shared_variable() {
        let facts = [
            fact(1, 10, 100),
            fact(2, 10, 200),
            fact(1, 20, 300),
            fact(3, 20, 400),
        ];
        let clauses = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(2)]),
        ];

        let values = project(&facts, &clauses, 2);
        assert_eq!(values.len(), 1);
        assert!(values[0].matches(&id(300)));
    }

    #[test]
    fn repeated_variable_within_clause_must_agree() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(0),
        ])];

        assert!(project(&facts, &clauses, 0).is_empty());
    }

    #[test]
    fn unbound_result_is_not_invented() {
        let facts = [fact(1, 10, 2)];
        let clauses = [Clause([
            Term::Constant(id(1)),
            Term::Constant(id(10)),
            Term::Constant(id(2)),
        ])];

        assert!(project(&facts, &clauses, 99).is_empty());
    }

    #[test]
    fn scalar_constants_match_by_value_and_deduplicate() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(7),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(7),
            },
        ];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Constant(integer(7)),
        ])];

        let values = project(&facts, &clauses, 0);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(1)));
        assert!(values[1].matches(&id(2)));
    }

    #[test]
    fn scalar_projection_deduplicates_equal_values() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(7),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(7),
            },
        ];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(1),
        ])];

        let values = project(&facts, &clauses, 1);
        assert_eq!(values.len(), 1);
        assert!(values[0].matches(&integer(7)));
    }

    #[test]
    fn predicate_filters_after_binding_with_numeric_promotion_and_strict_equality() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(18),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(30),
            },
            Triple {
                entity: id(3),
                attribute: id(10),
                value: float(30.0),
            },
        ];
        let clause = Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]);

        let over_eighteen =
            project_predicate(&facts, &clause, 1, CompareOp::Greater, &integer(18), 0);
        assert_eq!(over_eighteen.len(), 2);
        assert!(over_eighteen[0].matches(&id(2)));
        assert!(over_eighteen[1].matches(&id(3)));

        let exact_integer =
            project_predicate(&facts, &clause, 1, CompareOp::Equal, &integer(30), 0);
        assert_eq!(exact_integer.len(), 1);
        assert!(exact_integer[0].matches(&id(2)));

        let not_integer =
            project_predicate(&facts, &clause, 1, CompareOp::NotEqual, &integer(30), 0);
        assert_eq!(not_integer.len(), 2);
        assert!(not_integer[0].matches(&id(1)));
        assert!(not_integer[1].matches(&id(3)));
    }

    #[test]
    fn negation_excludes_matching_rows_without_binding_negative_variables() {
        let facts = [
            fact(1, 10, 100),
            fact(1, 20, 200),
            fact(2, 10, 100),
            fact(2, 20, 300),
            fact(3, 10, 100),
        ];
        let positive = Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Constant(id(100)),
        ]);
        let forbidden = Clause([
            Term::Variable(0),
            Term::Constant(id(20)),
            Term::Constant(id(200)),
        ]);

        let values = project_negated(&facts, &positive, &forbidden, 0);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(2)));
        assert!(values[1].matches(&id(3)));
    }
}
