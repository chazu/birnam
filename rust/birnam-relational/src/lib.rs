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

/// A grouped-query pattern with explicit variable identities. The older
/// `ProjectPattern` intentionally derives variable ids from positions; grouped
/// joins need two value positions to remain distinct (for example city versus
/// age), so this probe carries the ids beside each nullable term.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GroupedPattern {
    pub entity: Oop,
    pub attribute: Oop,
    pub value: Oop,
    pub entity_variable: u32,
    pub attribute_variable: u32,
    pub value_variable: u32,
    pub reserved: u32,
}

/// One node in the pointer-free pull specification supplied by the explicit
/// migration probe. `kind == 0` is a leaf attribute and `kind == 1` is a
/// reference whose child nodes form the nested pull spec. Child ranges are
/// indexes into the same flat node array; the caller supplies the number of
/// top-level nodes separately from the total node count.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PullSpecNode {
    pub attribute: Oop,
    pub kind: u8,
    pub reserved: [u8; 7],
    pub child_start: u32,
    pub child_len: u32,
}

/// A pointer-free flattened leaf produced by the explicit pull probe. `path`
/// contains the attribute path from the requested root to the leaf, in source
/// spec order. The fixed bound keeps the C ABI independent of Rust allocations.
pub const PULL_MAX_DEPTH: usize = 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PulledRecord {
    pub depth: u8,
    pub reserved: [u8; 7],
    pub path: [ClassifiedValue; PULL_MAX_DEPTH],
    pub value: ClassifiedValue,
}

/// One pointer-free grouped aggregate result. `aggregate_status == 0` carries
/// `aggregate`; `aggregate_status == 1` represents the language-level nil
/// result for an empty min/max/avg group.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GroupedRecord {
    pub group: ClassifiedValue,
    pub aggregate: ClassifiedValue,
    pub aggregate_status: u8,
    pub reserved: [u8; 7],
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

unsafe fn collect_project_facts_window(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
) -> Result<(Vec<Triple>, *const IdApi), i32> {
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
    let selector_name: &[u8] = match mode {
        0 => b"liveTriplesDo:\0",
        1 => b"liveTriplesAsOf:do:\0",
        2 => b"liveTriplesSince:do:\0",
        3 => b"liveTriplesBetween:and:do:\0",
        _ => {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return Err(-7);
        }
    };
    if (mode == 1 || mode == 2 || mode == 3) && (lower > i64::MAX as u64 || upper > i64::MAX as u64)
    {
        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
        return Err(-7);
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
    // SAFETY: Store owns the frozen current/history selector and synchronously
    // invokes value:value:value: on the callback object.
    unsafe { (api.send)(store, selector, arguments.as_ptr(), argument_count) };
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    if collector.status != 0 {
        return Err(collector.status);
    }
    Ok((collector.facts, collector.api))
}

unsafe fn collect_project_facts(store: Oop) -> Result<(Vec<Triple>, *const IdApi), i32> {
    // SAFETY: mode zero uses the current liveTriplesDo: callback protocol.
    unsafe { collect_project_facts_window(store, 0, 0, 0) }
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

unsafe fn grouped_terms(api: &IdApi, pattern: GroupedPattern) -> Result<[Term; 3], i32> {
    let term = |object: Oop, variable: u32| -> Result<Term, i32> {
        if object.is_null() {
            return Ok(Term::Variable(variable));
        }
        // SAFETY: grouped pattern objects are borrowed for this synchronous
        // call.
        unsafe { oop::classify(api, object) }
            .map(Term::Constant)
            .ok_or(-4)
    };
    Ok([
        term(pattern.entity, pattern.entity_variable)?,
        term(pattern.attribute, pattern.attribute_variable)?,
        term(pattern.value, pattern.value_variable)?,
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

#[allow(clippy::too_many_arguments)]
unsafe fn project_window_live(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
    projection: u8,
    pattern: ProjectPattern,
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
    let (facts, api_address) =
        match unsafe { collect_project_facts_window(store, mode, lower, upper) } {
            Ok(snapshot) => snapshot,
            Err(status) => return status,
        };
    // SAFETY: collect_project_facts_window returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let terms = match unsafe { project_terms(api, pattern) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let projected = project(&facts, &[Clause(terms)], projection as u32);
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

unsafe fn project_disjunction_live(
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
    let projected = project_disjunction(
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
    unsafe { *written = projected_len };
    0
}

unsafe fn project_recursive_live(
    store: Oop,
    projection: u8,
    edge: ProjectPattern,
    root: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if written.is_null() || (capacity != 0 && output.is_null()) || root.is_null() {
        return -3;
    }
    if projection != 0 && projection != 2 {
        return -7;
    }
    if !edge.entity.is_null() || edge.attribute.is_null() || !edge.value.is_null() {
        return -7;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let edge_terms = match unsafe { project_terms(api, edge) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    // SAFETY: root is borrowed from the caller for this synchronous call.
    let Some(root) = (unsafe { oop::classify(api, root) }) else {
        return -4;
    };
    let projected = project_recursive(&facts, &Clause(edge_terms), &root, projection as u32);
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

unsafe fn aggregate_live(
    store: Oop,
    projection: u8,
    pattern: ProjectPattern,
    operation: u8,
    output: *mut ClassifiedValue,
) -> i32 {
    if output.is_null() || store.is_null() {
        return -3;
    }
    if projection > 2 {
        return -7;
    }
    let Some(operation) = AggregateOp::from_u8(operation) else {
        return -7;
    };
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
    let values = project(&facts, &[Clause(terms)], projection as u32);
    let value = match aggregate_values(&values, operation) {
        Ok(Some(value)) => value,
        Ok(None) => {
            // `min`, `max`, and `avg` preserve the Zig aggregate surface's
            // nil on empty input. The caller gets an explicit status rather
            // than an ambiguous pointer-free identity-zero record.
            return -8;
        }
        Err(AggregateError::UnsupportedValue | AggregateError::Overflow) => return -4,
    };
    // SAFETY: output is non-null and points to caller-owned result storage.
    unsafe { ptr::write(output, record(value)) };
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

/// Project one triple-pattern variable from a Store transaction window. Mode
/// `0` is current, `1` is as-of `upper`, `2` is since `lower`, and `3` is the
/// half-open `[lower, upper)` window. This composes the frozen Store history
/// selectors with the same authority-free projection kernel.
///
/// # Safety
///
/// `store` and any non-null pattern term must remain live for the synchronous
/// call. `output` and `written` must point to caller-owned writable storage for
/// the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_window_live(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
    projection: u8,
    pattern: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe {
        project_window_live(
            store, mode, lower, upper, projection, pattern, output, capacity, written,
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

/// Project one variable from the union of two positive triple patterns. Each
/// branch runs against the same Store snapshot from the empty solution set;
/// branch order and first occurrence are preserved, while projected values are
/// deduplicated by the frozen value law. This is an explicit migration probe,
/// not the canonical Store `findAll:where:` method.
///
/// # Safety
///
/// `store` and any non-null pattern term must remain live for the synchronous
/// call. `output` and `written` must point to caller-owned writable storage for
/// the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_disjunction_live(
    store: Oop,
    projection: u8,
    first: ProjectPattern,
    second: ProjectPattern,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe { project_disjunction_live(store, projection, first, second, output, capacity, written) }
}

/// Project a root or reachable target from the transitive closure of one
/// positive edge pattern. The bounded probe requires a variable entity and
/// value with a constant attribute; it computes a deterministic finite
/// fixpoint over the copied Store snapshot. Projection `0` returns the root
/// side and projection `2` returns reachable targets. This is an explicit
/// migration probe, not rule registration, rule-cache persistence, mutual
/// recursion, or canonical Store `findAll:where:` installation.
///
/// # Safety
///
/// `store`, `root`, and the non-null edge attribute must remain live for the
/// synchronous call. `output` and `written` must point to caller-owned writable
/// storage for the declared capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_recursive_live(
    store: Oop,
    projection: u8,
    edge: ProjectPattern,
    root: Oop,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffers.
    unsafe { project_recursive_live(store, projection, edge, root, output, capacity, written) }
}

/// Aggregate one projected variable from a single triple pattern. Operations
/// are `0 count`, `1 sum`, `2 min`, `3 max`, and `4 avg`. Results are copied into
/// `ClassifiedValue`; count/sum return integer zero for empty input, while
/// min/max/avg return `-8` for the Zig-compatible empty nil result. This is an
/// explicit migration probe, not canonical Store aggregate installation.
///
/// # Safety
///
/// `store` and any non-null pattern term must remain live for the synchronous
/// call. `output` must point to caller-owned writable result storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_aggregate_live(
    store: Oop,
    projection: u8,
    pattern: ProjectPattern,
    operation: u8,
    output: *mut ClassifiedValue,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store object and output buffer.
    unsafe { aggregate_live(store, projection, pattern, operation, output) }
}

/// Flatten a nested pull specification over the current Store snapshot. The
/// node array is pointer-free and uses explicit child ranges; cardinality is
/// read from Store's `cardinality:` schema for each attribute. This is an
/// explicit migration probe, not the canonical `pull:spec:` installation.
///
/// Return values: `0` success, `-2` before `module_init`, `-3` invalid Store,
/// root, spec, or output pointers, `-4` malformed Oops/schema, `-5` output
/// capacity too small, `-6` Store callback failure, and `-7` malformed shape,
/// unknown node kind, excessive depth, or excessive node count.
///
/// # Safety
///
/// `store`, `root`, and every non-null spec attribute must remain live for the
/// synchronous call. `nodes` must point to `node_count` caller-owned records;
/// `root_count` names the top-level prefix of that array. `output` and
/// `written` must point to caller-owned writable storage for the declared
/// capacity and count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_pull_live(
    store: Oop,
    root: Oop,
    nodes: *const PullSpecNode,
    root_count: usize,
    node_count: usize,
    output: *mut PulledRecord,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store, spec, and output contracts.
    unsafe {
        pull_live(
            store, root, nodes, root_count, node_count, output, capacity, written,
        )
    }
}

/// Group two positive triple-pattern clauses and fold distinct values by
/// `operation` (`0 count`, `1 sum`, `2 min`, `3 max`, `4 avg`). Group order is
/// first appearance and the result is a caller-owned flat array rather than a
/// Rust Map, so this remains an explicit migration probe.
///
/// # Safety
///
/// `store`, any non-null pattern terms, and the output/count pointers must obey
/// the same synchronous borrowed-object and capacity contracts as the other
/// relational probes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_group_aggregate_live(
    store: Oop,
    first: GroupedPattern,
    second: GroupedPattern,
    group_variable: u32,
    value_variable: u32,
    operation: u8,
    output: *mut GroupedRecord,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store, patterns, and output
    // contracts.
    unsafe {
        group_aggregate_live(
            store,
            first,
            second,
            group_variable,
            value_variable,
            operation,
            output,
            capacity,
            written,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullKind {
    Leaf,
    Reference,
}

/// Owned, authority-free pull specification used by the pure kernel. The FFI
/// adapter builds this tree from borrowed Oops and immediately copies every
/// attribute into a `Value`.
#[derive(Clone, Debug)]
pub struct PullSpec {
    pub attribute: Value,
    pub kind: PullKind,
    pub many: bool,
    pub children: Vec<PullSpec>,
}

#[derive(Clone, Debug)]
pub struct PullRecord {
    pub path: Vec<Value>,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct GroupedAggregate {
    pub group: Value,
    pub aggregate: Option<Value>,
}

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

/// The bounded aggregate vocabulary used by the migration probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AggregateOp {
    Count = 0,
    Sum = 1,
    Min = 2,
    Max = 3,
    Average = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregateError {
    UnsupportedValue,
    Overflow,
}

impl AggregateOp {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Count,
            1 => Self::Sum,
            2 => Self::Min,
            3 => Self::Max,
            4 => Self::Average,
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

/// Evaluate two positive branches from the same empty solution set and union
/// their projected values. Branch order is preserved and projection remains
/// distinct under the existing value law. The bounded FFI probe uses one
/// triple clause per branch; the kernel keeps this helper independent of the
/// Store and IdApi.
pub fn project_disjunction(facts: &[Triple], branches: &[Clause], result: u32) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    for branch in branches {
        for value in project(facts, std::slice::from_ref(branch), result) {
            if !values.iter().any(|seen| seen.matches(&value)) {
                values.push(value);
            }
        }
    }
    values
}

/// Compute the transitive closure of a bounded binary edge clause. The clause
/// must bind variables `0` and `2`; its middle term is the edge attribute. New
/// `(source, target)` pairs are appended in deterministic breadth-first
/// fixpoint order and compared by the existing value law. The result projects
/// either side for rows whose source equals `root`.
pub fn project_recursive(facts: &[Triple], edge: &Clause, root: &Value, result: u32) -> Vec<Value> {
    if result != 0 && result != 2 {
        return Vec::new();
    }
    let solutions = evaluate_clauses(facts, std::slice::from_ref(edge));
    let mut edges: Vec<(Value, Value)> = Vec::new();
    for solution in solutions {
        let (Some(source), Some(target)) = (lookup(&solution, 0), lookup(&solution, 2)) else {
            continue;
        };
        if !edges.iter().any(|(seen_source, seen_target)| {
            seen_source.matches(source) && seen_target.matches(target)
        }) {
            edges.push((source.clone(), target.clone()));
        }
    }

    let mut cursor = 0;
    while cursor < edges.len() {
        let (source, middle) = edges[cursor].clone();
        for (next_source, target) in edges.clone() {
            if !middle.matches(&next_source)
                || edges.iter().any(|(seen_source, seen_target)| {
                    seen_source.matches(&source) && seen_target.matches(&target)
                })
            {
                continue;
            }
            edges.push((source.clone(), target));
        }
        cursor += 1;
    }

    let mut values: Vec<Value> = Vec::new();
    for (source, target) in edges {
        if !source.matches(root) {
            continue;
        }
        let value = if result == 0 { source } else { target };
        if !values.iter().any(|seen| seen.matches(&value)) {
            values.push(value);
        }
    }
    values
}

/// Fold distinct projected values with the frozen aggregate law. Count and
/// empty sum return integer zero. Min/max/average return `None` on empty input
/// (the caller maps that to the explicit nil status) and reject nonnumeric
/// values rather than inventing a cross-floor coercion.
pub fn aggregate_values(
    values: &[Value],
    operation: AggregateOp,
) -> Result<Option<Value>, AggregateError> {
    match operation {
        AggregateOp::Count => Ok(Some(Value::Integer(values.len() as i64))),
        AggregateOp::Sum | AggregateOp::Average => {
            if values.is_empty() {
                return Ok(Some(Value::Integer(0)));
            }
            let has_float = values.iter().any(|value| matches!(value, Value::Float(_)));
            if has_float {
                let mut sum = 0.0;
                for value in values {
                    sum += match value {
                        Value::Integer(integer) => *integer as f64,
                        Value::Float(float) => *float,
                        _ => return Err(AggregateError::UnsupportedValue),
                    };
                }
                if operation == AggregateOp::Average {
                    Ok(Some(Value::Float(sum / values.len() as f64)))
                } else {
                    Ok(Some(Value::Float(sum)))
                }
            } else {
                let mut sum = 0i64;
                for value in values {
                    let Value::Integer(integer) = value else {
                        return Err(AggregateError::UnsupportedValue);
                    };
                    sum = sum.checked_add(*integer).ok_or(AggregateError::Overflow)?;
                }
                if operation == AggregateOp::Average {
                    Ok(Some(Value::Integer(sum / values.len() as i64)))
                } else {
                    Ok(Some(Value::Integer(sum)))
                }
            }
        }
        AggregateOp::Min | AggregateOp::Max => {
            let mut best: Option<&Value> = None;
            for value in values {
                if !matches!(value, Value::Integer(_) | Value::Float(_)) {
                    return Err(AggregateError::UnsupportedValue);
                }
                let replace = match best {
                    None => true,
                    Some(current) => {
                        let op = if operation == AggregateOp::Min {
                            CompareOp::Less
                        } else {
                            CompareOp::Greater
                        };
                        value.compares(op, current)
                    }
                };
                if replace {
                    best = Some(value);
                }
            }
            Ok(best.cloned())
        }
    }
}

const MAX_PULL_NODES: usize = 4_096;

fn pull_values(facts: &[Triple], entity: &Value, attribute: &Value) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    for triple in facts {
        if !triple.entity.matches(entity) || !triple.attribute.matches(attribute) {
            continue;
        }
        if !values.iter().any(|seen| seen.matches(&triple.value)) {
            values.push(triple.value.clone());
        }
    }
    values
}

fn pull_into(
    facts: &[Triple],
    entity: &Value,
    specs: &[PullSpec],
    path: &mut Vec<Value>,
    output: &mut Vec<PullRecord>,
) {
    for spec in specs {
        let values = pull_values(facts, entity, &spec.attribute);
        if values.is_empty() {
            continue;
        }
        path.push(spec.attribute.clone());
        match spec.kind {
            PullKind::Leaf => {
                let selected = if spec.many {
                    values.into_iter().collect::<Vec<_>>()
                } else {
                    values.into_iter().take(1).collect::<Vec<_>>()
                };
                for value in selected {
                    output.push(PullRecord {
                        path: path.clone(),
                        value,
                    });
                }
            }
            PullKind::Reference => {
                let selected = if spec.many {
                    values.into_iter().collect::<Vec<_>>()
                } else {
                    values.into_iter().take(1).collect::<Vec<_>>()
                };
                for reference in selected {
                    // The FFI parser bounds this tree to PULL_MAX_DEPTH. The
                    // guard also makes the pure helper total if called directly
                    // by a future Rust consumer with a malformed tree.
                    if path.len() >= PULL_MAX_DEPTH {
                        continue;
                    }
                    pull_into(facts, &reference, &spec.children, path, output);
                }
            }
        }
        path.pop();
    }
}

/// Flatten a nested pull specification over a copied Store snapshot. This is
/// the authority-free kernel seam for the later canonical `pull:spec:` method:
/// facts remain Store-owned, cardinality is supplied by the adapter, and the
/// result owns only copied values and attribute paths.
pub fn pull(facts: &[Triple], root: &Value, specs: &[PullSpec]) -> Vec<PullRecord> {
    let mut output = Vec::new();
    let mut path = Vec::new();
    pull_into(facts, root, specs, &mut path, &mut output);
    output
}

/// Group the solutions of two positive clauses by one bound variable and fold
/// distinct values of another. This is the bounded kernel seam for the later
/// canonical `groupBy:aggregate:of:where:` method: group order follows first
/// appearance, values use the same value law, and facts remain Store-owned.
pub fn group_aggregate(
    facts: &[Triple],
    clauses: &[Clause],
    group_variable: u32,
    value_variable: u32,
    operation: AggregateOp,
) -> Result<Vec<GroupedAggregate>, AggregateError> {
    let solutions = evaluate_clauses(facts, clauses);
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for solution in solutions {
        let (Some(group), Some(value)) = (
            lookup(&solution, group_variable),
            lookup(&solution, value_variable),
        ) else {
            continue;
        };
        let Some(existing) = groups.iter_mut().find(|(seen, _)| seen.matches(group)) else {
            groups.push((group.clone(), vec![value.clone()]));
            continue;
        };
        if !existing.1.iter().any(|seen| seen.matches(value)) {
            existing.1.push(value.clone());
        }
    }
    groups
        .into_iter()
        .map(|(group, values)| {
            Ok(GroupedAggregate {
                group,
                aggregate: aggregate_values(&values, operation)?,
            })
        })
        .collect()
}

unsafe fn parse_pull_specs(
    api: &IdApi,
    store: Oop,
    nodes: &[PullSpecNode],
    start: usize,
    length: usize,
    depth: usize,
) -> Result<Vec<PullSpec>, i32> {
    if depth >= PULL_MAX_DEPTH || length > nodes.len().saturating_sub(start) {
        return Err(-7);
    }
    let cardinality_selector = unsafe { (api.intern)(c"cardinality:".as_ptr()) };
    let many_symbol = unsafe { (api.intern)(c"many".as_ptr()) };
    let mut specs = Vec::with_capacity(length);
    for node in &nodes[start..start + length] {
        if node.kind > 1 {
            return Err(-7);
        }
        let child_start = node.child_start as usize;
        let child_len = node.child_len as usize;
        if child_len > nodes.len().saturating_sub(child_start) {
            return Err(-7);
        }
        if node.kind == 0 && child_len != 0 {
            return Err(-7);
        }
        if node.attribute.is_null() {
            return Err(-4);
        }
        // SAFETY: the spec attribute is borrowed for this synchronous call.
        let Some(attribute) = (unsafe { oop::classify(api, node.attribute) }) else {
            return Err(-4);
        };
        let arguments = [node.attribute];
        // SAFETY: Store owns the frozen cardinality: selector and returns a
        // borrowed schema symbol for this synchronous query.
        let cardinality = unsafe { (api.send)(store, cardinality_selector, arguments.as_ptr(), 1) };
        if cardinality.is_null() {
            return Err(-4);
        }
        let children = if node.kind == 1 {
            // SAFETY: child_start/child_len were checked against the same
            // caller-owned flat array; recursive depth is bounded above.
            unsafe { parse_pull_specs(api, store, nodes, child_start, child_len, depth + 1)? }
        } else {
            Vec::new()
        };
        specs.push(PullSpec {
            attribute,
            kind: if node.kind == 0 {
                PullKind::Leaf
            } else {
                PullKind::Reference
            },
            many: cardinality == many_symbol,
            children,
        });
    }
    Ok(specs)
}

#[allow(clippy::too_many_arguments)]
unsafe fn pull_live(
    store: Oop,
    root: Oop,
    nodes: *const PullSpecNode,
    root_count: usize,
    node_count: usize,
    output: *mut PulledRecord,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if store.is_null()
        || root.is_null()
        || written.is_null()
        || (node_count != 0 && nodes.is_null())
        || (capacity != 0 && output.is_null())
    {
        return -3;
    }
    if node_count > MAX_PULL_NODES || root_count > node_count {
        return -7;
    }
    let Some(api_address) = API.get() else {
        return -2;
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(*api_address as *const IdApi) };
    let (facts, collected_api) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    debug_assert_eq!(*api_address as *const IdApi, collected_api);
    // SAFETY: the caller validated the pointer/count pair above and retains the
    // flat specification for this synchronous call.
    let nodes: &[PullSpecNode] = if node_count == 0 {
        &[]
    } else {
        // SAFETY: the pointer/count pair was validated above.
        unsafe { std::slice::from_raw_parts(nodes, node_count) }
    };
    let specs = match unsafe { parse_pull_specs(api, store, nodes, 0, root_count, 0) } {
        Ok(specs) => specs,
        Err(status) => return status,
    };
    // SAFETY: root is borrowed from the caller for this synchronous call.
    let Some(root) = (unsafe { oop::classify(api, root) }) else {
        return -4;
    };
    let records = pull(&facts, &root, &specs);
    if records.len() > capacity {
        return -5;
    }
    let blank = record(Value::Identity(0));
    for (slot, pulled) in records.iter().enumerate() {
        let mut path = [blank; PULL_MAX_DEPTH];
        for (index, attribute) in pulled.path.iter().enumerate() {
            path[index] = record(attribute.clone());
        }
        // SAFETY: the caller supplied capacity records and the length check
        // above bounds every write.
        unsafe {
            ptr::write(
                output.add(slot),
                PulledRecord {
                    depth: pulled.path.len() as u8,
                    reserved: [0; 7],
                    path,
                    value: record(pulled.value.clone()),
                },
            )
        };
    }
    unsafe { *written = records.len() };
    0
}

#[allow(clippy::too_many_arguments)]
unsafe fn group_aggregate_live(
    store: Oop,
    first: GroupedPattern,
    second: GroupedPattern,
    group_variable: u32,
    value_variable: u32,
    operation: u8,
    output: *mut GroupedRecord,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if store.is_null() || written.is_null() || (capacity != 0 && output.is_null()) {
        return -3;
    }
    let Some(operation) = AggregateOp::from_u8(operation) else {
        return -7;
    };
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let first_terms = match unsafe { grouped_terms(api, first) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let second_terms = match unsafe { grouped_terms(api, second) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let grouped = match group_aggregate(
        &facts,
        &[Clause(first_terms), Clause(second_terms)],
        group_variable,
        value_variable,
        operation,
    ) {
        Ok(grouped) => grouped,
        Err(AggregateError::UnsupportedValue | AggregateError::Overflow) => return -4,
    };
    if grouped.len() > capacity {
        return -5;
    }
    for (slot, result) in grouped.iter().enumerate() {
        let (aggregate, aggregate_status) = match &result.aggregate {
            Some(value) => (record(value.clone()), 0),
            None => (record(Value::Identity(0)), 1),
        };
        // SAFETY: the caller supplied capacity records and the length check
        // above bounds every write.
        unsafe {
            ptr::write(
                output.add(slot),
                GroupedRecord {
                    group: record(result.group.clone()),
                    aggregate,
                    aggregate_status,
                    reserved: [0; 7],
                },
            )
        };
    }
    unsafe { *written = grouped.len() };
    0
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
        AggregateOp, Clause, CompareOp, PullKind, PullSpec, Term, Triple, Value, aggregate_values,
        group_aggregate, project, project_disjunction, project_negated, project_predicate,
        project_recursive, pull,
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

    #[test]
    fn disjunction_unions_branches_in_branch_order_and_deduplicates_projection() {
        let facts = [
            fact(1, 10, 100),
            fact(2, 10, 100),
            fact(2, 20, 200),
            fact(3, 20, 200),
        ];
        let branches = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(1)]),
        ];

        let values = project_disjunction(&facts, &branches, 0);
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&id(1)));
        assert!(values[1].matches(&id(2)));
        assert!(values[2].matches(&id(3)));
    }

    #[test]
    fn recursive_edge_projection_reaches_a_finite_fixpoint_in_order() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3), fact(3, 10, 4)];
        let edge = Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(2)]);

        let values = project_recursive(&facts, &edge, &id(1), 2);
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&id(2)));
        assert!(values[1].matches(&id(3)));
        assert!(values[2].matches(&id(4)));
    }

    #[test]
    fn aggregates_preserve_distinct_numeric_and_empty_laws() {
        let values = [integer(18), integer(30), integer(40)];
        assert!(
            aggregate_values(&values, AggregateOp::Count)
                .unwrap()
                .unwrap()
                .matches(&integer(3))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Sum)
                .unwrap()
                .unwrap()
                .matches(&integer(88))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Min)
                .unwrap()
                .unwrap()
                .matches(&integer(18))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Max)
                .unwrap()
                .unwrap()
                .matches(&integer(40))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Average)
                .unwrap()
                .unwrap()
                .matches(&integer(29))
        );
        assert!(
            aggregate_values(&[], AggregateOp::Sum)
                .unwrap()
                .unwrap()
                .matches(&integer(0))
        );
        assert!(aggregate_values(&[], AggregateOp::Min).unwrap().is_none());
    }

    #[test]
    fn pull_flattens_many_references_and_preserves_spec_order() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(2),
            },
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(3),
            },
            Triple {
                entity: id(2),
                attribute: id(20),
                value: Value::String(b"Bob".to_vec()),
            },
            Triple {
                entity: id(3),
                attribute: id(20),
                value: Value::String(b"Carol".to_vec()),
            },
        ];
        let specs = [PullSpec {
            attribute: id(10),
            kind: PullKind::Reference,
            many: true,
            children: vec![PullSpec {
                attribute: id(20),
                kind: PullKind::Leaf,
                many: false,
                children: Vec::new(),
            }],
        }];

        let records = pull(&facts, &id(1), &specs);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].path.len(), 2);
        assert!(records[0].path[0].matches(&id(10)));
        assert!(records[0].path[1].matches(&id(20)));
        assert!(records[0].value.matches(&Value::String(b"Bob".to_vec())));
        assert!(records[1].value.matches(&Value::String(b"Carol".to_vec())));
    }

    #[test]
    fn grouped_aggregate_preserves_group_order_and_distinct_values() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(100),
            },
            Triple {
                entity: id(1),
                attribute: id(20),
                value: integer(20),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: id(100),
            },
            Triple {
                entity: id(2),
                attribute: id(20),
                value: integer(40),
            },
            Triple {
                entity: id(3),
                attribute: id(10),
                value: id(200),
            },
            Triple {
                entity: id(3),
                attribute: id(20),
                value: integer(30),
            },
            Triple {
                entity: id(4),
                attribute: id(10),
                value: id(200),
            },
            Triple {
                entity: id(4),
                attribute: id(20),
                value: integer(30),
            },
        ];
        let clauses = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(2)]),
        ];

        let sums = group_aggregate(&facts, &clauses, 1, 2, AggregateOp::Sum).unwrap();
        assert_eq!(sums.len(), 2);
        assert!(sums[0].group.matches(&id(100)));
        assert!(sums[0].aggregate.as_ref().unwrap().matches(&integer(60)));
        assert!(sums[1].group.matches(&id(200)));
        assert!(sums[1].aggregate.as_ref().unwrap().matches(&integer(30)));

        let counts = group_aggregate(&facts, &clauses, 1, 0, AggregateOp::Count).unwrap();
        assert!(counts[0].aggregate.as_ref().unwrap().matches(&integer(2)));
        assert!(counts[1].aggregate.as_ref().unwrap().matches(&integer(2)));
    }
}
