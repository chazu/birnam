//! Native Store collection, relational selectors and caller-owned output.
//! Owns the module API, cached vtables, logic variables and existing root providers.
use crate::oop;
use crate::evaluator::*;

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const RUST_RELATIONAL_IDENTITY: &[u8] = b"rust-relational-r4.5-project\0";
static API: OnceLock<(usize, usize)> = OnceLock::new();
static ACTIVE_PROBE_BLOCK: AtomicUsize = AtomicUsize::new(0);
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
static ENTITY_VT: OnceLock<usize> = OnceLock::new();
static LOGICVAR_VT: OnceLock<usize> = OnceLock::new();
static LOGICVAR_FACTORY_VT: OnceLock<usize> = OnceLock::new();
static LOGICVAR_FACTORY: OnceLock<usize> = OnceLock::new();
static REL_COLLECTOR_VT: OnceLock<usize> = OnceLock::new();
static LOGICVARS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Relational: {message}");
    std::process::abort();
}

unsafe fn bring_up(api: *mut IdApi, imports: *const birnam_abi::namespace_import::ApiV1) {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    let address = api as usize;
    match API.get() {
        Some(existing) if existing.0 != address => abort("IdApi was replaced"),
        Some(_) => {}
        None => {
            let _ = API.set((address, imports as usize));
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
    borrowed_record(&value)
}

fn borrowed_record(value: &Value) -> ClassifiedValue {
    let mut record = ClassifiedValue {
        tag: 0,
        reserved: [0; 7],
        integer: 0,
        float: 0.0,
        identity: 0,
        byte_len: 0,
    };
    match value {
        Value::Identity(identity) => record.identity = *identity,
        Value::Integer(integer) => {
            record.tag = 1;
            record.integer = *integer;
        }
        Value::Float(float) => {
            record.tag = 2;
            record.float = *float;
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

unsafe extern "C" fn mark_relational_roots(mark: MarkFn) {
    unsafe { mark_probe_root(mark) };
    for root in [
        ENTITY_VT.get(),
        LOGICVAR_VT.get(),
        LOGICVAR_FACTORY_VT.get(),
        LOGICVAR_FACTORY.get(),
        REL_COLLECTOR_VT.get(),
    ]
    .into_iter()
    .flatten()
    {
        // SAFETY: these are the relational module's process-owned cached Oops.
        unsafe { mark(*root as Oop) };
    }
    let Some(cache) = LOGICVARS.get() else {
        return;
    };
    let Ok(cache) = cache.lock() else {
        abort("Rust Relational logic-variable cache was poisoned");
    };
    for value in cache.values() {
        // SAFETY: every cached value was allocated through this process's IdApi
        // and remains live for the duration of collection.
        unsafe { mark(*value as Oop) };
    }
}

fn register_probe_root(api: &IdApi) {
    ROOT_REGISTERED.get_or_init(|| unsafe {
        // SAFETY: the process floor owns this root-provider registration for the
        // lifetime of the loaded Rust Relational module.
        (api.gcRegisterRootProvider)(mark_relational_roots);
    });
}

unsafe extern "C" fn rust_logicvar_named(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 1 || arguments[0].is_null() {
        return api.nilObj;
    }
    let cache = LOGICVARS.get_or_init(|| Mutex::new(HashMap::new()));
    let key = arguments[0] as usize;
    let mut cache = cache
        .lock()
        .unwrap_or_else(|_| abort("Rust Relational logic-variable cache was poisoned"));
    if let Some(value) = cache.get(&key) {
        return *value as Oop;
    }
    let Some(vtable) = LOGICVAR_VT.get() else {
        return api.nilObj;
    };
    let value = unsafe { (api.alloc)(*vtable as Oop, size_of::<Oop>()) };
    if value.is_null() {
        return api.nilObj;
    }
    // SAFETY: LogicVar's frozen payload is one Oop containing its interned name.
    unsafe { ptr::write(value.cast::<Oop>(), arguments[0]) };
    cache.insert(key, value as usize);
    value
}

unsafe extern "C" fn rust_logicvar_name(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    if count != 0 || self_object.is_null() {
        return api.nilObj;
    }
    // SAFETY: callers only dispatch this method on the exact LogicVar family.
    unsafe { *self_object.cast::<Oop>() }
}

/// Exact native fact; selector names and factory caching confer no authority.
/// See abi/native-field-projection-v1.md.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_field_projection_v1(method: *const c_void, nil: Oop) -> u32 {
    let Some(api_address) = API.get() else {
        return 0;
    };
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    field_projection_declaration(method, api.nilObj, nil)
}

fn field_projection_declaration(
    method: *const c_void,
    initialized_nil: Oop,
    supplied_nil: Oop,
) -> u32 {
    u32::from(initialized_nil == supplied_nil && method == rust_logicvar_name as *const c_void)
}

#[cfg(test)]
#[test]
fn native_field_projection_declares_only_the_exact_pure_getter() {
    let nil = core::ptr::with_exposed_provenance_mut(8);
    let other = core::ptr::with_exposed_provenance_mut(16);
    assert_eq!(
        field_projection_declaration(rust_logicvar_name as *const c_void, nil, nil),
        1
    );
    assert_eq!(
        field_projection_declaration(rust_logicvar_name as *const c_void, nil, other),
        0
    );
    assert_eq!(
        field_projection_declaration(rust_logicvar_named as *const c_void, nil, nil),
        0
    );
    assert_eq!(
        field_projection_declaration(rust_logicvar_print_string as *const c_void, nil, nil),
        0
    );
    assert_eq!(field_projection_declaration(core::ptr::null(), nil, nil), 0);
}

unsafe extern "C" fn rust_logicvar_print_string(
    self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    if count != 0 || self_object.is_null() {
        return api.nilObj;
    }
    let name = unsafe { rust_logicvar_name(self_object, ptr::null(), 0) };
    if name.is_null() {
        return api.nilObj;
    }
    let name = unsafe { std::ffi::CStr::from_ptr((api.symbolName)(name)) }.to_bytes();
    let mut text = Vec::with_capacity(name.len() + 1);
    text.push(b'?');
    text.extend_from_slice(name);
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

unsafe fn ensure_relational_contract(api: &IdApi, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1) {
    let logicvar_vt = unsafe { birnam_abi::namespace_import::import(imports, c"LogicVarVt".as_ptr()) };
    let logicvar_vt = if logicvar_vt.is_null() {
        let created = unsafe { (api.proto)(api.object_vt) };
        if created.is_null() {
            abort("Rust Relational could not create LogicVarVt");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"LogicVarVt".as_ptr(), created) };
        created
    } else {
        logicvar_vt
    };
    let _ = LOGICVAR_VT.set(logicvar_vt as usize);

    let factory_vt = unsafe { birnam_abi::namespace_import::import(imports, c"LogicVarFactoryVt".as_ptr()) };
    let factory_vt = if factory_vt.is_null() {
        let created = unsafe { (api.proto)(api.object_vt) };
        if created.is_null() {
            abort("Rust Relational could not create LogicVarFactoryVt");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"LogicVarFactoryVt".as_ptr(), created) };
        created
    } else {
        factory_vt
    };
    let _ = LOGICVAR_FACTORY_VT.set(factory_vt as usize);

    let factory = unsafe { birnam_abi::namespace_import::import(imports, c"LogicVar".as_ptr()) };
    let factory = if factory.is_null() {
        let created = unsafe { (api.alloc)(factory_vt, 0) };
        if created.is_null() {
            abort("Rust Relational could not create LogicVar factory");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"LogicVar".as_ptr(), created) };
        created
    } else {
        factory
    };
    let _ = LOGICVAR_FACTORY.set(factory as usize);

    let entity_vt = unsafe { birnam_abi::namespace_import::import(imports, c"EntityVt".as_ptr()) };
    let entity_vt = if entity_vt.is_null() {
        let created = unsafe { (api.proto)(api.object_vt) };
        if created.is_null() {
            abort("Rust Relational could not create EntityVt");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"EntityVt".as_ptr(), created) };
        created
    } else {
        entity_vt
    };
    let _ = ENTITY_VT.set(entity_vt as usize);

    let collector_vt = unsafe { birnam_abi::namespace_import::import(imports, c"RelCollectorVt".as_ptr()) };
    let collector_vt = if collector_vt.is_null() {
        let created = unsafe { (api.proto)(api.object_vt) };
        if created.is_null() {
            abort("Rust Relational could not create RelCollectorVt");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"RelCollectorVt".as_ptr(), created) };
        created
    } else {
        collector_vt
    };
    let _ = REL_COLLECTOR_VT.set(collector_vt as usize);

    unsafe {
        (api.setLayout)(logicvar_vt, ptr::null(), 0, birnam_abi::SLOT_OOP);
        (api.method)(
            factory_vt,
            (api.intern)(c"named:".as_ptr()),
            rust_logicvar_named,
        );
        (api.method)(
            logicvar_vt,
            (api.intern)(c"name".as_ptr()),
            rust_logicvar_name,
        );
        (api.method)(
            logicvar_vt,
            (api.intern)(c"printString".as_ptr()),
            rust_logicvar_print_string,
        );
        birnam_abi::namespace_export::export(exports, c"LogicVarVt".as_ptr(), logicvar_vt);
        birnam_abi::namespace_export::export(exports, c"LogicVarFactoryVt".as_ptr(), factory_vt);
        birnam_abi::namespace_export::export(exports, c"LogicVar".as_ptr(), factory);
        birnam_abi::namespace_export::export(exports, c"EntityVt".as_ptr(), entity_vt);
        birnam_abi::namespace_export::export(exports, c"RelCollectorVt".as_ptr(), collector_vt);
    }
}

unsafe extern "C" fn rust_entity(_self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(vtable) = ENTITY_VT.get() else {
        return ptr::null_mut();
    };
    // SAFETY: the canonical EntityVt has an empty payload and the returned
    // object is owned by the live floor heap.
    unsafe { (api.alloc)(*vtable as Oop, 0) }
}

unsafe fn value_oop(_api: &IdApi, value: Value) -> Oop {
    match value {
        Value::Identity(identity) => identity as usize as Oop,
        Value::Integer(integer) => unsafe { (birnam_abi::integer::make)(integer) },
        Value::Float(float) => unsafe { (birnam_abi::float::make)(float) },
        Value::String(bytes) => unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) },
    }
}

unsafe fn smallint_u64(_api: &IdApi, object: Oop) -> Option<u64> {
    if object.is_null() || unsafe { (birnam_abi::integer::is_integer)(object) } == 0 {
        return None;
    }
    let value = unsafe { (birnam_abi::integer::value)(object) };
    (value >= 0).then_some(value as u64)
}

unsafe fn rust_find_all_window(store: Oop, args: *const Oop, count: usize, mode: u8) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let expected_count = match mode {
        0 => 2,
        1 | 2 => 3,
        3 => 4,
        _ => return api.nilObj,
    };
    if arguments.len() != expected_count || store.is_null() {
        return api.nilObj;
    }
    let logicvar_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"LogicVarVt".as_ptr()) };
    if logicvar_vt.is_null() {
        return api.nilObj;
    }
    let name_selector = unsafe { (api.intern)(c"name".as_ptr()) };
    let mut variables = Vec::new();
    let result = match unsafe {
        parse_store_term(
            api,
            arguments[0],
            logicvar_vt,
            name_selector,
            &mut variables,
        )
    } {
        Ok(Term::Variable(variable)) => variable,
        _ => return api.nilObj,
    };
    let body = match unsafe {
        parse_store_rule_body(
            api,
            arguments[1],
            logicvar_vt,
            name_selector,
            &mut variables,
        )
    } {
        Ok(body) => body,
        Err(_) => return api.nilObj,
    };
    let (lower, upper) = match mode {
        0 => (0, 0),
        1 => (
            0,
            unsafe { smallint_u64(api, arguments[2]) }.unwrap_or(u64::MAX),
        ),
        2 => (
            unsafe { smallint_u64(api, arguments[2]) }.unwrap_or(u64::MAX),
            0,
        ),
        3 => (
            unsafe { smallint_u64(api, arguments[2]) }.unwrap_or(u64::MAX),
            unsafe { smallint_u64(api, arguments[3]) }.unwrap_or(u64::MAX),
        ),
        _ => unreachable!(),
    };
    if (mode != 0 && (lower == u64::MAX || upper == u64::MAX)) || (mode == 3 && lower > upper) {
        return api.nilObj;
    }
    let rule_name = unsafe { (api.intern)(c"__rule-name__".as_ptr()) };
    let (facts, pairs, _) = match unsafe {
        collect_project_facts_window_with_rules(store, mode, lower, upper, rule_name)
    } {
        Ok(snapshot) => snapshot,
        Err(_) => return api.nilObj,
    };
    let definitions = match unsafe { parse_store_rule_definitions(api, &pairs) } {
        Ok(definitions) => definitions,
        Err(_) => return api.nilObj,
    };
    let values = project_rule_body(&facts, &definitions, &body, result);
    let vector_factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) };
    if vector_factory.is_null() {
        return api.nilObj;
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let empty: [Oop; 0] = [];
    let mut output = unsafe { (api.send)(vector_factory, new_selector, empty.as_ptr(), 0) };
    if output.is_null() {
        return api.nilObj;
    }
    // Root the growing Vector across any floor sends performed by `add:`.
    ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    for value in values {
        let object = unsafe { value_oop(api, value) };
        output = unsafe { (api.send)(output, add_selector, &object, 1) };
        if output.is_null() {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return api.nilObj;
        }
        ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    }
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    output
}

unsafe extern "C" fn rust_find_all(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_find_all_window(_self_object, args, count, 0) }
}

unsafe extern "C" fn rust_find_all_as_of(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_find_all_window(_self_object, args, count, 1) }
}

unsafe extern "C" fn rust_find_all_since(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_find_all_window(_self_object, args, count, 2) }
}

unsafe extern "C" fn rust_find_all_between(
    _self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_find_all_window(_self_object, args, count, 3) }
}

unsafe fn rust_aggregate(store: Oop, args: *const Oop, count: usize, kind: u8) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let values = unsafe { rust_find_all_window(store, args, count, 0) };
    if values.is_null() || values == api.nilObj {
        return api.nilObj;
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(values, size_selector, ptr::null(), 0) };
    if size.is_null() || unsafe { (birnam_abi::integer::is_integer)(size) } == 0 {
        return api.nilObj;
    }
    let count_value = unsafe { (birnam_abi::integer::value)(size) };
    if count_value < 0 {
        return api.nilObj;
    }
    if kind == 0 {
        return size;
    }
    if count_value == 0 {
        return if kind == 1 {
            unsafe { (birnam_abi::integer::make)(0) }
        } else {
            api.nilObj
        };
    }
    ACTIVE_PROBE_BLOCK.store(values as usize, Ordering::Release);
    let mut copied = Vec::with_capacity(count_value as usize);
    for index in 0..count_value {
        let argument = unsafe { (birnam_abi::integer::make)(index) };
        let value = unsafe { (api.send)(values, at_selector, &argument, 1) };
        let Some(value) = (unsafe { oop::classify(api, value) }) else {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return api.nilObj;
        };
        copied.push(value);
    }
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    let operation = match kind {
        1 => AggregateOp::Sum,
        2 => AggregateOp::Min,
        3 => AggregateOp::Max,
        4 => AggregateOp::Average,
        _ => return api.nilObj,
    };
    match aggregate_values(&copied, operation) {
        Ok(Some(value)) => unsafe { value_oop(api, value) },
        Ok(None) | Err(AggregateError::UnsupportedValue | AggregateError::Overflow) => api.nilObj,
    }
}

unsafe extern "C" fn rust_count(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_aggregate(_self_object, args, count, 0) }
}

unsafe extern "C" fn rust_sum(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_aggregate(_self_object, args, count, 1) }
}

unsafe extern "C" fn rust_min(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_aggregate(_self_object, args, count, 2) }
}

unsafe extern "C" fn rust_max(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_aggregate(_self_object, args, count, 3) }
}

unsafe extern "C" fn rust_avg(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { rust_aggregate(_self_object, args, count, 4) }
}

unsafe fn parse_aggregate_kind(api: &IdApi, object: Oop) -> Option<AggregateOp> {
    let intern = |name: &'static [u8]| unsafe { (api.intern)(name.as_ptr().cast()) };
    Some(if object == intern(b"count\0") {
        AggregateOp::Count
    } else if object == intern(b"sum\0") {
        AggregateOp::Sum
    } else if object == intern(b"min\0") {
        AggregateOp::Min
    } else if object == intern(b"max\0") {
        AggregateOp::Max
    } else if object == intern(b"avg\0") {
        AggregateOp::Average
    } else {
        return None;
    })
}

unsafe extern "C" fn rust_group_by(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 4 {
        return api.nilObj;
    }
    let logicvar_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"LogicVarVt".as_ptr()) };
    let Some(operation) = (unsafe { parse_aggregate_kind(api, arguments[1]) }) else {
        return api.nilObj;
    };
    if logicvar_vt.is_null() {
        return api.nilObj;
    }
    let name_selector = unsafe { (api.intern)(c"name".as_ptr()) };
    let mut variables = Vec::new();
    let group_variable = match unsafe {
        parse_store_term(
            api,
            arguments[0],
            logicvar_vt,
            name_selector,
            &mut variables,
        )
    } {
        Ok(Term::Variable(variable)) => variable,
        _ => return api.nilObj,
    };
    let value_variable = match unsafe {
        parse_store_term(
            api,
            arguments[2],
            logicvar_vt,
            name_selector,
            &mut variables,
        )
    } {
        Ok(Term::Variable(variable)) => variable,
        _ => return api.nilObj,
    };
    let body = match unsafe {
        parse_store_rule_body(
            api,
            arguments[3],
            logicvar_vt,
            name_selector,
            &mut variables,
        )
    } {
        Ok(body) => body,
        Err(_) => return api.nilObj,
    };
    let rule_name = unsafe { (api.intern)(c"__rule-name__".as_ptr()) };
    let (facts, pairs, _) =
        match unsafe { collect_project_facts_and_rules(_self_object, rule_name) } {
            Ok(snapshot) => snapshot,
            Err(_) => return api.nilObj,
        };
    let definitions = match unsafe { parse_store_rule_definitions(api, &pairs) } {
        Ok(definitions) => definitions,
        Err(_) => return api.nilObj,
    };
    let grouped = match group_rule_body(
        &facts,
        &definitions,
        &body,
        group_variable,
        value_variable,
        operation,
    ) {
        Ok(grouped) => grouped,
        Err(_) => return api.nilObj,
    };
    let map_factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Map".as_ptr()) };
    if map_factory.is_null() {
        return api.nilObj;
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let at_put_selector = unsafe { (api.intern)(c"at:put:".as_ptr()) };
    let empty: [Oop; 0] = [];
    let mut output = unsafe { (api.send)(map_factory, new_selector, empty.as_ptr(), 0) };
    if output.is_null() {
        return api.nilObj;
    }
    ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    for result in grouped {
        let group = unsafe { value_oop(api, result.group) };
        let aggregate = unsafe {
            result
                .aggregate
                .map(|value| value_oop(api, value))
                .unwrap_or(api.nilObj)
        };
        let arguments = [group, aggregate];
        output = unsafe { (api.send)(output, at_put_selector, arguments.as_ptr(), 2) };
        if output.is_null() {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return api.nilObj;
        }
        ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    }
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    output
}

unsafe fn parse_store_pull_specs(
    api: &IdApi,
    store: Oop,
    spec: Oop,
    depth: usize,
) -> Result<Vec<PullSpec>, i32> {
    if depth >= PULL_MAX_DEPTH {
        return Err(-7);
    }
    let elements = unsafe { vector_elements(api, spec)? };
    let vector_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"VectorVt".as_ptr()) };
    if vector_vt.is_null() {
        return Err(-6);
    }
    let cardinality_selector = unsafe { (api.intern)(c"cardinality:".as_ptr()) };
    let many_symbol = unsafe { (api.intern)(c"many".as_ptr()) };
    let mut specs = Vec::with_capacity(elements.len());
    for element in elements {
        let (attribute, kind, children) = if unsafe { (api.vtableOf)(element) } == vector_vt {
            let nested = unsafe { vector_elements(api, element)? };
            if nested.len() != 2 {
                return Err(-7);
            }
            let Some(attribute) = (unsafe { oop::classify(api, nested[0]) }) else {
                return Err(-4);
            };
            let children = unsafe { parse_store_pull_specs(api, store, nested[1], depth + 1)? };
            (attribute, PullKind::Reference, children)
        } else {
            let Some(attribute) = (unsafe { oop::classify(api, element) }) else {
                return Err(-4);
            };
            (attribute, PullKind::Leaf, Vec::new())
        };
        let argument = unsafe { value_oop(api, attribute.clone()) };
        let cardinality = unsafe { (api.send)(store, cardinality_selector, &argument, 1) };
        specs.push(PullSpec {
            attribute,
            kind,
            many: cardinality == many_symbol,
            children,
        });
    }
    Ok(specs)
}

unsafe fn new_vector(api: &IdApi) -> Oop {
    let factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) };
    if factory.is_null() {
        return ptr::null_mut();
    }
    let selector = unsafe { (api.intern)(c"new".as_ptr()) };
    unsafe { (api.send)(factory, selector, ptr::null(), 0) }
}

unsafe fn materialize_pull_entity(
    api: &IdApi,
    facts: &[Triple],
    entity: &Value,
    specs: &[PullSpec],
) -> Oop {
    let factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Map".as_ptr()) };
    if factory.is_null() {
        return ptr::null_mut();
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let at_put_selector = unsafe { (api.intern)(c"at:put:".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let mut output = unsafe { (api.send)(factory, new_selector, ptr::null(), 0) };
    if output.is_null() {
        return ptr::null_mut();
    }
    ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    for spec in specs {
        let values = pull_values(facts, entity, &spec.attribute);
        if values.is_empty() {
            continue;
        }
        let result = match spec.kind {
            PullKind::Leaf => {
                if spec.many {
                    let mut vector = unsafe { new_vector(api) };
                    if vector.is_null() {
                        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
                        return ptr::null_mut();
                    }
                    ACTIVE_PROBE_BLOCK.store(vector as usize, Ordering::Release);
                    for value in values {
                        let object = unsafe { value_oop(api, value) };
                        vector = unsafe { (api.send)(vector, add_selector, &object, 1) };
                        if vector.is_null() {
                            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
                            return ptr::null_mut();
                        }
                        ACTIVE_PROBE_BLOCK.store(vector as usize, Ordering::Release);
                    }
                    ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
                    vector
                } else {
                    unsafe { value_oop(api, values[0].clone()) }
                }
            }
            PullKind::Reference => {
                if spec.many {
                    let mut vector = unsafe { new_vector(api) };
                    if vector.is_null() {
                        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
                        return ptr::null_mut();
                    }
                    ACTIVE_PROBE_BLOCK.store(vector as usize, Ordering::Release);
                    for value in values {
                        let child =
                            unsafe { materialize_pull_entity(api, facts, &value, &spec.children) };
                        if child.is_null() {
                            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
                            return ptr::null_mut();
                        }
                        vector = unsafe { (api.send)(vector, add_selector, &child, 1) };
                        if vector.is_null() {
                            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
                            return ptr::null_mut();
                        }
                        ACTIVE_PROBE_BLOCK.store(vector as usize, Ordering::Release);
                    }
                    ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
                    vector
                } else {
                    unsafe { materialize_pull_entity(api, facts, &values[0], &spec.children) }
                }
            }
        };
        if result.is_null() {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return ptr::null_mut();
        }
        let attribute = unsafe { value_oop(api, spec.attribute.clone()) };
        let arguments = [attribute, result];
        output = unsafe { (api.send)(output, at_put_selector, arguments.as_ptr(), 2) };
        if output.is_null() {
            ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
            return ptr::null_mut();
        }
        ACTIVE_PROBE_BLOCK.store(output as usize, Ordering::Release);
    }
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    output
}

unsafe extern "C" fn rust_pull(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 {
        return api.nilObj;
    }
    let Some(entity) = (unsafe { oop::classify(api, arguments[0]) }) else {
        return api.nilObj;
    };
    let specs = match unsafe { parse_store_pull_specs(api, _self_object, arguments[1], 0) } {
        Ok(specs) => specs,
        Err(_) => return api.nilObj,
    };
    let (facts, _) = match unsafe { collect_project_facts(_self_object) } {
        Ok(snapshot) => snapshot,
        Err(_) => return api.nilObj,
    };
    let result = unsafe { materialize_pull_entity(api, &facts, &entity, &specs) };
    if result.is_null() { api.nilObj } else { result }
}

/// Persist one canonical rule definition as a Store fact. Rule queries rebuild
/// their derived relation from these facts on every call, so this write-through
/// bridge keeps the Rust Store surface self-contained without retaining a
/// second Rust-owned rule cache.
unsafe extern "C" fn rust_define_rule(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 3
        || self_object.is_null()
        || arguments.iter().any(|argument| argument.is_null())
    {
        return api.nilObj;
    }
    let vector_factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) };
    if vector_factory.is_null() {
        return api.nilObj;
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let empty: [Oop; 0] = [];
    let mut pair = unsafe { (api.send)(vector_factory, new_selector, empty.as_ptr(), 0) };
    if pair.is_null() {
        return api.nilObj;
    }
    // The pair is the only newly allocated object here. Root it while the two
    // `add:` sends can allocate or collect.
    ACTIVE_PROBE_BLOCK.store(pair as usize, Ordering::Release);
    pair = unsafe { (api.send)(pair, add_selector, &arguments[1], 1) };
    if pair.is_null() {
        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
        return api.nilObj;
    }
    ACTIVE_PROBE_BLOCK.store(pair as usize, Ordering::Release);
    pair = unsafe { (api.send)(pair, add_selector, &arguments[2], 1) };
    if pair.is_null() {
        ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
        return api.nilObj;
    }
    ACTIVE_PROBE_BLOCK.store(pair as usize, Ordering::Release);
    let rule_name = unsafe { (api.intern)(c"__rule-name__".as_ptr()) };
    let assert_selector = unsafe { (api.intern)(c"assert:attr:value:".as_ptr()) };
    let assertion = [pair, rule_name, arguments[0]];
    let result = unsafe { (api.send)(self_object, assert_selector, assertion.as_ptr(), 3) };
    ACTIVE_PROBE_BLOCK.store(0, Ordering::Release);
    if result.is_null() {
        return api.nilObj;
    }
    self_object
}

/// Rule definitions are canonical Store facts. Queries reify them directly,
/// so rebuilding the old cache is intentionally a successful no-op.
unsafe extern "C" fn rust_rebuild_rules(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let Some(api_address) = API.get() else {
        return ptr::null_mut();
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    if count != 0 || self_object.is_null() {
        return api.nilObj;
    }
    self_object
}

unsafe fn install_entity_method(api: &IdApi, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1) {
    let store_vt = unsafe { birnam_abi::namespace_import::import(imports, c"StoreVt".as_ptr()) };
    if store_vt.is_null() {
        // The classifier-only probe may load this dylib before Store. The
        // canonical bridge is installed once StoreVt is available.
        return;
    }
    let store = unsafe { birnam_abi::namespace_import::import(imports, c"Store".as_ptr()) };
    if store.is_null() {
        return;
    }
    let entity_vt = unsafe { birnam_abi::namespace_import::import(imports, c"EntityVt".as_ptr()) };
    let entity_vt = if entity_vt.is_null() {
        // SAFETY: the new vtable delegates to the process Object vtable and
        // carries no payload, matching the EntityVt contract.
        let created = unsafe { (api.proto)(api.object_vt) };
        if created.is_null() {
            abort("Rust Relational could not create EntityVt");
        }
        unsafe { birnam_abi::namespace_export::export(exports, c"EntityVt".as_ptr(), created) };
        created
    } else {
        entity_vt
    };
    let _ = ENTITY_VT.set(entity_vt as usize);
    let selector = unsafe { (api.intern)(c"entity".as_ptr()) };
    // SAFETY: the Store vtable and selector are process-owned and the method
    // has the frozen IdApi callback ABI.
    unsafe { (api.method)(store_vt, selector, rust_entity) };
    if unsafe { birnam_abi::namespace_import::import(imports, c"Vector".as_ptr()) }.is_null()
        || unsafe { birnam_abi::namespace_import::import(imports, c"Map".as_ptr()) }.is_null()
    {
        return;
    }
    let find_all_selector = unsafe { (api.intern)(c"findAll:where:".as_ptr()) };
    // SAFETY: this bridge is installed only when the live collection and
    // LogicVar contracts are already present; the callback keeps Store as its
    // sole fact authority and returns the shared Vector family.
    unsafe {
        (api.method)(
            store_vt,
            (api.intern)(c"defineRule:params:body:".as_ptr()),
            rust_define_rule,
        );
        (api.method)(
            store_vt,
            (api.intern)(c"rebuildRules".as_ptr()),
            rust_rebuild_rules,
        );
        (api.method)(store_vt, find_all_selector, rust_find_all);
    }
    let as_of_selector = unsafe { (api.intern)(c"findAll:where:asOf:".as_ptr()) };
    let since_selector = unsafe { (api.intern)(c"findAll:where:since:".as_ptr()) };
    let between_selector = unsafe { (api.intern)(c"findAll:where:between:and:".as_ptr()) };
    unsafe {
        (api.method)(store_vt, as_of_selector, rust_find_all_as_of);
        (api.method)(store_vt, since_selector, rust_find_all_since);
        (api.method)(store_vt, between_selector, rust_find_all_between);
        (api.method)(store_vt, (api.intern)(c"count:where:".as_ptr()), rust_count);
        (api.method)(store_vt, (api.intern)(c"sum:where:".as_ptr()), rust_sum);
        (api.method)(store_vt, (api.intern)(c"min:where:".as_ptr()), rust_min);
        (api.method)(store_vt, (api.intern)(c"max:where:".as_ptr()), rust_max);
        (api.method)(store_vt, (api.intern)(c"avg:where:".as_ptr()), rust_avg);
        (api.method)(
            store_vt,
            (api.intern)(c"groupBy:aggregate:of:where:".as_ptr()),
            rust_group_by,
        );
        (api.method)(store_vt, (api.intern)(c"pull:spec:".as_ptr()), rust_pull);
        // Publish the shared Store as `db` only after the complete relational
        // and collection contract is available. A partial export would force
        // roll Relational down its reload path with missing vtables.
        birnam_abi::namespace_export::export(exports, c"db".as_ptr(), store);
    }
}

/// Identifies the Rust Relational module; only a bounded Store selector family
/// is canonical at this checkpoint.
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
    let api = unsafe { &*(api_address.0 as *const IdApi) };
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
    rule_name: Oop,
    rule_pairs: Vec<(Oop, Oop)>,
    status: i32,
}

type ProjectRuleSnapshot = (Vec<Triple>, Vec<(Oop, Oop)>, *const IdApi);

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
    if !collector.rule_name.is_null() && args[1] == collector.rule_name {
        collector.rule_pairs.push((args[0], args[2]));
    }
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
    let api = unsafe { &*(api_address.0 as *const IdApi) };
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
    let lower_oop = unsafe { (birnam_abi::integer::make)(lower as i64) };
    let upper_oop = unsafe { (birnam_abi::integer::make)(upper as i64) };
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

unsafe fn collect_project_facts_window_with_rules(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
    rule_name: Oop,
) -> Result<ProjectRuleSnapshot, i32> {
    if store.is_null() {
        return Err(-3);
    }
    let Some(api_address) = API.get() else {
        return Err(-2);
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
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
        rule_name,
        rule_pairs: Vec::new(),
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
    let lower_oop = unsafe { (birnam_abi::integer::make)(lower as i64) };
    let upper_oop = unsafe { (birnam_abi::integer::make)(upper as i64) };
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
    Ok((collector.facts, collector.rule_pairs, collector.api))
}

unsafe fn collect_project_facts_window(
    store: Oop,
    mode: u8,
    lower: u64,
    upper: u64,
) -> Result<(Vec<Triple>, *const IdApi), i32> {
    let (facts, _, api) = unsafe {
        collect_project_facts_window_with_rules(store, mode, lower, upper, ptr::null_mut())?
    };
    Ok((facts, api))
}

unsafe fn collect_project_facts(store: Oop) -> Result<(Vec<Triple>, *const IdApi), i32> {
    // SAFETY: mode zero uses the current liveTriplesDo: callback protocol.
    unsafe { collect_project_facts_window(store, 0, 0, 0) }
}

unsafe fn collect_project_facts_and_rules(
    store: Oop,
    rule_name: Oop,
) -> Result<ProjectRuleSnapshot, i32> {
    unsafe { collect_project_facts_window_with_rules(store, 0, 0, 0, rule_name) }
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

/// Publish only after every callback and borrowed-input read has completed.
/// Callers retain their early pointer checks. Raw writes permit existing input/
/// output and count/output overlap; no exclusive output slice is introduced.
unsafe fn write_projected_values(
    values: &[Value],
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if values.len() > capacity {
        return -5;
    }
    for (slot, value) in values.iter().enumerate() {
        // SAFETY: the caller supplied writable record storage; the complete
        // length check bounds writes and all input access has already ended.
        unsafe { ptr::write(output.add(slot), borrowed_record(value)) };
    }
    unsafe { *written = values.len() };
    0
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
    unsafe { write_projected_values(&projected, output, capacity, written) }
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
            // `min`, `max`, and `avg` preserve the aggregate surface's
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
/// min/max/avg return `-8` for the compatible empty nil result. This is an
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

/// Evaluate a bounded set of positive/recursive rule definitions over the
/// current Store snapshot and project one query-rule variable. Definitions and
/// derived tuples are caller-scoped Rust data; this explicit probe does not
/// install a second Store rule cache or the canonical `defineRule:` selectors.
///
/// # Safety
///
/// All input arrays and Oops are borrowed for this synchronous call. Output and
/// count pointers must name caller-owned storage for `capacity` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_rule_live(
    store: Oop,
    definitions: *const RuleDefinitionNode,
    definition_count: usize,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store, rule descriptors, and
    // output contracts.
    unsafe {
        project_rule_live(
            store,
            definitions,
            definition_count,
            query_name,
            query_terms,
            query_term_count,
            result_variable,
            output,
            capacity,
            written,
        )
    }
}

/// Evaluate tree-shaped rule definitions with bounded nested negation and
/// disjunction. This remains an explicit probe; it does not install Store
/// methods or persist a Rust-owned rule cache.
///
/// # Safety
///
/// All descriptor arrays, Oops, and output pointers are borrowed for this
/// synchronous call and must remain valid for its duration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_rule_tree_live(
    store: Oop,
    definitions: *const RuleTreeDefinitionNode,
    definition_count: usize,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store, tree descriptors, and
    // output contracts for one synchronous probe call.
    unsafe {
        project_rule_tree_live(
            store,
            definitions,
            definition_count,
            query_name,
            query_terms,
            query_term_count,
            result_variable,
            output,
            capacity,
            written,
        )
    }
}

/// Reify the canonical `[pair __rule-name__ name]` facts from the supplied
/// Store snapshot and project one rule query. This is still an explicit probe:
/// it copies definitions synchronously and does not install Store selectors or
/// retain a second authoritative cache.
///
/// # Safety
///
/// The Store, query descriptors, and output pointers are borrowed for this
/// synchronous call and must remain valid for its duration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_relational_rust_project_rule_store_live(
    store: Oop,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies the frozen Store, query descriptors, and
    // output contracts for one synchronous probe call.
    unsafe {
        project_rule_store_live(
            store,
            query_name,
            query_terms,
            query_term_count,
            result_variable,
            output,
            capacity,
            written,
        )
    }
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the explicit Rust Relational probe against the process IdApi.
///
/// # Safety
///
/// `api` must be the process-owned IdApi table supplied by the host, and the
/// host must keep that table and its floor dylib alive for the module lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1) -> *mut c_void {
    // SAFETY: forwarded frozen host initialization contract.
    unsafe { bring_up(api, imports) };
    // Publish the complete shared relational object contract before exposing
    // the Store as `db`; this is safe to repeat across reloads.
    unsafe { ensure_relational_contract(&*api, imports, exports) };
    // SAFETY: module_init received the process-owned IdApi used to install the
    // Store-facing entity constructor.
    unsafe { install_entity_method(&*api, imports, exports) };
    // SAFETY: module_init received the same process-owned IdApi recorded above.
    register_probe_root(unsafe { &*api });
    ptr::null_mut()
}


/// Pointer-free input records for the bounded rule-engine probe. A term is a
/// constant when `kind == 0` (and `object` is classified) or a local variable
/// when `kind == 1` (using `variable`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuleTermNode {
    pub object: Oop,
    pub variable: u32,
    pub kind: u8,
    pub reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuleClauseNode {
    pub kind: u8,
    pub reserved: [u8; 3],
    pub name: Oop,
    pub terms: *const RuleTermNode,
    pub term_count: u32,
    pub reserved_tail: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuleDefinitionNode {
    pub name: Oop,
    pub params: *const u32,
    pub param_count: u32,
    pub reserved_params: u32,
    pub body: *const RuleClauseNode,
    pub body_count: u32,
    pub reserved_body: u32,
}

/// Tree-shaped rule body node used by the richer explicit probe. The original
/// flat `RuleClauseNode` ABI remains unchanged; kind 3 is negation, kind 4 is
/// disjunction, and kind 5 is a disjunction branch container whose children
/// are ordinary body nodes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuleBodyNode {
    pub kind: u8,
    pub reserved: [u8; 3],
    pub name: Oop,
    pub terms: *const RuleTermNode,
    pub term_count: u32,
    pub reserved_terms: u32,
    pub children: *const RuleBodyNode,
    pub child_count: u32,
    pub reserved_children: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuleTreeDefinitionNode {
    pub name: Oop,
    pub params: *const u32,
    pub param_count: u32,
    pub reserved_params: u32,
    pub body: *const RuleBodyNode,
    pub body_count: u32,
    pub reserved_body: u32,
}


const MAX_PULL_NODES: usize = 4_096;


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
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let (facts, collected_api) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    debug_assert_eq!(api_address.0 as *const IdApi, collected_api);
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

const MAX_RULE_DEFINITIONS: usize = 128;
const MAX_RULE_BODY_CLAUSES: usize = 512;
const MAX_RULE_TERMS: usize = 8;
const MAX_RULE_NESTING: usize = 16;

unsafe fn parse_rule_term(api: &IdApi, node: RuleTermNode) -> Result<Term, i32> {
    match node.kind {
        0 => {
            if node.object.is_null() {
                return Err(-4);
            }
            // SAFETY: constants are borrowed for this synchronous call.
            unsafe { oop::classify(api, node.object) }
                .map(Term::Constant)
                .ok_or(-4)
        }
        1 => Ok(Term::Variable(node.variable)),
        _ => Err(-7),
    }
}

unsafe fn parse_rule_terms(
    api: &IdApi,
    nodes: *const RuleTermNode,
    count: usize,
) -> Result<Vec<Term>, i32> {
    if count > MAX_RULE_TERMS || (count != 0 && nodes.is_null()) {
        return Err(-7);
    }
    let nodes = if count == 0 {
        &[]
    } else {
        // SAFETY: the caller validated the pointer/count pair above.
        unsafe { std::slice::from_raw_parts(nodes, count) }
    };
    nodes
        .iter()
        .copied()
        .map(|node| unsafe { parse_rule_term(api, node) })
        .collect()
}

unsafe fn parse_rule_compare_op(api: &IdApi, name: Oop) -> Result<CompareOp, i32> {
    if name.is_null() {
        return Err(-4);
    }
    // SAFETY: operator symbols are interned in the process symbol table and
    // compared by the same identity law as the relational selector.
    let intern = |bytes: &'static [u8]| unsafe { (api.intern)(bytes.as_ptr().cast()) };
    let operation = if name == intern(b"lt\0") || name == intern(b"<\0") {
        CompareOp::Less
    } else if name == intern(b"le\0") || name == intern(b"<=\0") {
        CompareOp::LessEqual
    } else if name == intern(b"gt\0") || name == intern(b">\0") {
        CompareOp::Greater
    } else if name == intern(b"ge\0") || name == intern(b">=\0") {
        CompareOp::GreaterEqual
    } else if name == intern(b"eq\0") || name == intern(b"=\0") {
        CompareOp::Equal
    } else if name == intern(b"ne\0") || name == intern(b"~=\0") || name == intern(b"!=\0") {
        CompareOp::NotEqual
    } else {
        return Err(-7);
    };
    Ok(operation)
}

unsafe fn parse_rule_body_tree(
    api: &IdApi,
    nodes: *const RuleBodyNode,
    count: usize,
    depth: usize,
) -> Result<Vec<RuleClause>, i32> {
    if depth > MAX_RULE_NESTING || count > MAX_RULE_BODY_CLAUSES || (count != 0 && nodes.is_null())
    {
        return Err(-7);
    }
    let nodes = if count == 0 {
        &[]
    } else {
        // SAFETY: the caller validated the pointer/count pair above.
        unsafe { std::slice::from_raw_parts(nodes, count) }
    };
    let mut body = Vec::with_capacity(nodes.len());
    for node in nodes {
        let terms = unsafe { parse_rule_terms(api, node.terms, node.term_count as usize)? };
        let children = match node.kind {
            3 => unsafe {
                parse_rule_body_tree(api, node.children, node.child_count as usize, depth + 1)?
            },
            4 => Vec::new(),
            _ if node.child_count == 0 => Vec::new(),
            _ => return Err(-7),
        };
        match node.kind {
            0 if terms.len() == 3 && children.is_empty() => {
                body.push(RuleClause::Triple(Clause([
                    terms[0].clone(),
                    terms[1].clone(),
                    terms[2].clone(),
                ])))
            }
            1 if children.is_empty() => {
                if node.name.is_null() {
                    return Err(-4);
                }
                // SAFETY: rule call names are borrowed for this synchronous parse.
                let Some(name) = (unsafe { oop::classify(api, node.name) }) else {
                    return Err(-4);
                };
                body.push(RuleClause::Call { name, args: terms });
            }
            2 if terms.len() == 2 && children.is_empty() => body.push(RuleClause::Predicate {
                left: terms[0].clone(),
                operation: unsafe { parse_rule_compare_op(api, node.name)? },
                right: terms[1].clone(),
            }),
            3 if terms.is_empty() && node.name.is_null() => {
                body.push(RuleClause::Negated(children));
            }
            4 if terms.is_empty() && node.name.is_null() => {
                let branch_nodes = if node.child_count == 0 {
                    &[]
                } else {
                    // SAFETY: the child pointer/count pair was validated by
                    // the recursive parse above; branch containers are read
                    // from the same borrowed tree for this synchronous call.
                    unsafe { std::slice::from_raw_parts(node.children, node.child_count as usize) }
                };
                let mut branches = Vec::with_capacity(branch_nodes.len());
                for branch in branch_nodes {
                    if branch.kind != 5
                        || !branch.terms.is_null()
                        || branch.term_count != 0
                        || !branch.name.is_null()
                    {
                        return Err(-7);
                    }
                    branches.push(unsafe {
                        parse_rule_body_tree(
                            api,
                            branch.children,
                            branch.child_count as usize,
                            depth + 1,
                        )?
                    });
                }
                body.push(RuleClause::Disjunction(branches));
            }
            _ => return Err(-7),
        }
    }
    Ok(body)
}

unsafe fn parse_rule_tree_definitions(
    api: &IdApi,
    nodes: *const RuleTreeDefinitionNode,
    count: usize,
) -> Result<Vec<RuleDefinition>, i32> {
    if count > MAX_RULE_DEFINITIONS || (count != 0 && nodes.is_null()) {
        return Err(-7);
    }
    let nodes = if count == 0 {
        &[]
    } else {
        // SAFETY: the caller validated the pointer/count pair above.
        unsafe { std::slice::from_raw_parts(nodes, count) }
    };
    let mut definitions = Vec::with_capacity(count);
    for definition in nodes {
        if definition.name.is_null()
            || definition.param_count as usize > MAX_RULE_TERMS
            || (definition.param_count != 0 && definition.params.is_null())
            || definition.body_count as usize > MAX_RULE_BODY_CLAUSES
            || (definition.body_count != 0 && definition.body.is_null())
        {
            return Err(-7);
        }
        // SAFETY: rule names and parameter/body arrays are borrowed for this
        // synchronous call.
        let Some(name) = (unsafe { oop::classify(api, definition.name) }) else {
            return Err(-4);
        };
        let params = if definition.param_count == 0 {
            Vec::new()
        } else {
            // SAFETY: the pointer/count pair was validated above.
            unsafe {
                std::slice::from_raw_parts(definition.params, definition.param_count as usize)
                    .to_vec()
            }
        };
        let body = unsafe {
            parse_rule_body_tree(api, definition.body, definition.body_count as usize, 0)?
        };
        definitions.push(RuleDefinition { name, params, body });
    }
    Ok(definitions)
}

unsafe fn vector_elements(api: &IdApi, vector: Oop) -> Result<Vec<Oop>, i32> {
    if vector.is_null() {
        return Err(-4);
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(vector, size_selector, ptr::null(), 0) };
    if size.is_null() || unsafe { (birnam_abi::integer::is_integer)(size) } == 0 {
        return Err(-7);
    }
    let size = unsafe { (birnam_abi::integer::value)(size) };
    if size < 0 || size as usize > MAX_RULE_BODY_CLAUSES {
        return Err(-7);
    }
    let mut elements = Vec::with_capacity(size as usize);
    for index in 0..size {
        let argument = unsafe { (birnam_abi::integer::make)(index) };
        let element = unsafe { (api.send)(vector, at_selector, &argument, 1) };
        if element.is_null() {
            return Err(-4);
        }
        elements.push(element);
    }
    Ok(elements)
}

unsafe fn parse_store_term(
    api: &IdApi,
    term: Oop,
    logicvar_vt: Oop,
    name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<Term, i32> {
    if term.is_null() {
        return Err(-4);
    }
    if unsafe { (api.vtableOf)(term) } == logicvar_vt {
        let name = unsafe { (api.send)(term, name_selector, ptr::null(), 0) };
        if name.is_null() {
            return Err(-4);
        }
        let variable = match variables.iter().position(|existing| *existing == name) {
            Some(index) => index as u32,
            None => {
                variables.push(name);
                (variables.len() - 1) as u32
            }
        };
        return Ok(Term::Variable(variable));
    }
    // SAFETY: non-logic terms are copied by value under the frozen classifier.
    unsafe { oop::classify(api, term) }
        .map(Term::Constant)
        .ok_or(-4)
}

unsafe fn parse_store_rule_body(
    api: &IdApi,
    body: Oop,
    logicvar_vt: Oop,
    name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<Vec<RuleClause>, i32> {
    let clauses = unsafe { vector_elements(api, body)? };
    let mut parsed = Vec::with_capacity(clauses.len());
    for clause in clauses {
        parsed.push(unsafe {
            parse_store_rule_clause(api, clause, logicvar_vt, name_selector, variables)?
        });
    }
    Ok(parsed)
}

unsafe fn parse_store_rule_clause(
    api: &IdApi,
    clause: Oop,
    logicvar_vt: Oop,
    name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<RuleClause, i32> {
    let terms = unsafe { vector_elements(api, clause)? };
    let rule_symbol = unsafe { (api.intern)(c"rule".as_ptr()) };
    let not_symbol = unsafe { (api.intern)(c"not".as_ptr()) };
    let or_symbol = unsafe { (api.intern)(c"or".as_ptr()) };
    if terms.len() == 3
        && terms[0] != rule_symbol
        && terms[0] != not_symbol
        && terms[0] != or_symbol
    {
        let left =
            unsafe { parse_store_term(api, terms[0], logicvar_vt, name_selector, variables)? };
        let middle =
            unsafe { parse_store_term(api, terms[1], logicvar_vt, name_selector, variables)? };
        let right =
            unsafe { parse_store_term(api, terms[2], logicvar_vt, name_selector, variables)? };
        if let Ok(operation) = unsafe { parse_rule_compare_op(api, terms[1]) } {
            return Ok(RuleClause::Predicate {
                left,
                operation,
                right,
            });
        }
        return Ok(RuleClause::Triple(Clause([left, middle, right])));
    }
    if terms.len() >= 2 && terms[0] == rule_symbol {
        let Some(name) = (unsafe { oop::classify(api, terms[1]) }) else {
            return Err(-4);
        };
        let mut args = Vec::with_capacity(terms.len() - 2);
        for term in &terms[2..] {
            args.push(unsafe {
                parse_store_term(api, *term, logicvar_vt, name_selector, variables)?
            });
        }
        return Ok(RuleClause::Call { name, args });
    }
    if terms.first().is_some_and(|term| *term == not_symbol) {
        let mut inner = Vec::with_capacity(terms.len().saturating_sub(1));
        for term in &terms[1..] {
            inner.push(unsafe {
                parse_store_rule_clause(api, *term, logicvar_vt, name_selector, variables)?
            });
        }
        return Ok(RuleClause::Negated(inner));
    }
    if terms.first().is_some_and(|term| *term == or_symbol) {
        let mut branches = Vec::with_capacity(terms.len().saturating_sub(1));
        for term in &terms[1..] {
            branches.push(vec![unsafe {
                parse_store_rule_clause(api, *term, logicvar_vt, name_selector, variables)?
            }]);
        }
        return Ok(RuleClause::Disjunction(branches));
    }
    Err(-7)
}

unsafe fn parse_store_rule_definitions(
    api: &IdApi,
    pairs: &[(Oop, Oop)],
) -> Result<Vec<RuleDefinition>, i32> {
    let logicvar_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"LogicVarVt".as_ptr()) };
    if logicvar_vt.is_null() {
        return Err(-6);
    }
    let name_selector = unsafe { (api.intern)(c"name".as_ptr()) };
    let mut definitions = Vec::with_capacity(pairs.len());
    for (pair, name_oop) in pairs {
        let pair_values = unsafe { vector_elements(api, *pair)? };
        if pair_values.len() != 2 {
            return Err(-7);
        }
        let Some(name) = (unsafe { oop::classify(api, *name_oop) }) else {
            return Err(-4);
        };
        let params = unsafe { vector_elements(api, pair_values[0])? };
        let mut variables = Vec::with_capacity(params.len());
        let mut parameter_ids = Vec::with_capacity(params.len());
        for parameter in params {
            let Term::Variable(variable) = (unsafe {
                parse_store_term(api, parameter, logicvar_vt, name_selector, &mut variables)?
            }) else {
                return Err(-7);
            };
            parameter_ids.push(variable);
        }
        let body = unsafe {
            parse_store_rule_body(
                api,
                pair_values[1],
                logicvar_vt,
                name_selector,
                &mut variables,
            )?
        };
        definitions.push(RuleDefinition {
            name,
            params: parameter_ids,
            body,
        });
    }
    Ok(definitions)
}

unsafe fn parse_rule_definitions(
    api: &IdApi,
    nodes: *const RuleDefinitionNode,
    count: usize,
) -> Result<Vec<RuleDefinition>, i32> {
    if count > MAX_RULE_DEFINITIONS || (count != 0 && nodes.is_null()) {
        return Err(-7);
    }
    let nodes = if count == 0 {
        &[]
    } else {
        // SAFETY: the caller validated the pointer/count pair above.
        unsafe { std::slice::from_raw_parts(nodes, count) }
    };
    let mut definitions = Vec::with_capacity(count);
    for definition in nodes {
        if definition.name.is_null()
            || definition.param_count as usize > MAX_RULE_TERMS
            || (definition.param_count != 0 && definition.params.is_null())
            || definition.body_count as usize > MAX_RULE_BODY_CLAUSES
            || (definition.body_count != 0 && definition.body.is_null())
        {
            return Err(-7);
        }
        // SAFETY: rule names and parameter/body arrays are borrowed for this
        // synchronous call.
        let Some(name) = (unsafe { oop::classify(api, definition.name) }) else {
            return Err(-4);
        };
        let params = if definition.param_count == 0 {
            Vec::new()
        } else {
            // SAFETY: the pointer/count pair was validated above.
            unsafe {
                std::slice::from_raw_parts(definition.params, definition.param_count as usize)
                    .to_vec()
            }
        };
        let body_nodes = if definition.body_count == 0 {
            &[]
        } else {
            // SAFETY: the pointer/count pair was validated above.
            unsafe { std::slice::from_raw_parts(definition.body, definition.body_count as usize) }
        };
        let mut body = Vec::with_capacity(body_nodes.len());
        for clause in body_nodes {
            let terms = unsafe { parse_rule_terms(api, clause.terms, clause.term_count as usize)? };
            match clause.kind {
                0 if terms.len() == 3 => body.push(RuleClause::Triple(Clause([
                    terms[0].clone(),
                    terms[1].clone(),
                    terms[2].clone(),
                ]))),
                1 => {
                    if clause.name.is_null() {
                        return Err(-4);
                    }
                    // SAFETY: rule call names are borrowed for this synchronous
                    // parse.
                    let Some(call_name) = (unsafe { oop::classify(api, clause.name) }) else {
                        return Err(-4);
                    };
                    body.push(RuleClause::Call {
                        name: call_name,
                        args: terms,
                    });
                }
                2 if terms.len() == 2 => body.push(RuleClause::Predicate {
                    left: terms[0].clone(),
                    operation: unsafe { parse_rule_compare_op(api, clause.name)? },
                    right: terms[1].clone(),
                }),
                0 => return Err(-7),
                _ => return Err(-7),
            }
        }
        definitions.push(RuleDefinition { name, params, body });
    }
    Ok(definitions)
}

#[allow(clippy::too_many_arguments)]
unsafe fn project_rule_live(
    store: Oop,
    definitions: *const RuleDefinitionNode,
    definition_count: usize,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if store.is_null()
        || query_name.is_null()
        || written.is_null()
        || (capacity != 0 && output.is_null())
    {
        return -3;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let definitions = match unsafe { parse_rule_definitions(api, definitions, definition_count) } {
        Ok(definitions) => definitions,
        Err(status) => return status,
    };
    // SAFETY: query name and terms are borrowed for this synchronous call.
    let Some(query_name) = (unsafe { oop::classify(api, query_name) }) else {
        return -4;
    };
    let query_terms = match unsafe { parse_rule_terms(api, query_terms, query_term_count) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let values = query_rule(
        &facts,
        &definitions,
        &query_name,
        &query_terms,
        result_variable,
    );
    unsafe { write_projected_values(&values, output, capacity, written) }
}

#[allow(clippy::too_many_arguments)]
unsafe fn project_rule_tree_live(
    store: Oop,
    definitions: *const RuleTreeDefinitionNode,
    definition_count: usize,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if store.is_null()
        || query_name.is_null()
        || written.is_null()
        || (capacity != 0 && output.is_null())
    {
        return -3;
    }
    let (facts, api_address) = match unsafe { collect_project_facts(store) } {
        Ok(snapshot) => snapshot,
        Err(status) => return status,
    };
    // SAFETY: collect_project_facts returns the process-owned IdApi address.
    let api = unsafe { &*api_address };
    let definitions =
        match unsafe { parse_rule_tree_definitions(api, definitions, definition_count) } {
            Ok(definitions) => definitions,
            Err(status) => return status,
        };
    // SAFETY: query name and terms are borrowed for this synchronous call.
    let Some(query_name) = (unsafe { oop::classify(api, query_name) }) else {
        return -4;
    };
    let query_terms = match unsafe { parse_rule_terms(api, query_terms, query_term_count) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let values = query_rule(
        &facts,
        &definitions,
        &query_name,
        &query_terms,
        result_variable,
    );
    unsafe { write_projected_values(&values, output, capacity, written) }
}

#[allow(clippy::too_many_arguments)]
unsafe fn project_rule_store_live(
    store: Oop,
    query_name: Oop,
    query_terms: *const RuleTermNode,
    query_term_count: usize,
    result_variable: u32,
    output: *mut ClassifiedValue,
    capacity: usize,
    written: *mut usize,
) -> i32 {
    if store.is_null()
        || query_name.is_null()
        || written.is_null()
        || (capacity != 0 && output.is_null())
    {
        return -3;
    }
    let Some(api_address) = API.get() else {
        return -2;
    };
    // SAFETY: module_init stores only the process-owned IdApi address.
    let api = unsafe { &*(api_address.0 as *const IdApi) };
    let rule_name = unsafe { (api.intern)(c"__rule-name__".as_ptr()) };
    let (facts, pairs, api_address) =
        match unsafe { collect_project_facts_and_rules(store, rule_name) } {
            Ok(snapshot) => snapshot,
            Err(status) => return status,
        };
    debug_assert_eq!(
        API.get().expect("IdApi initialized").0 as *const IdApi,
        api_address
    );
    let definitions = match unsafe { parse_store_rule_definitions(api, &pairs) } {
        Ok(definitions) => definitions,
        Err(status) => return status,
    };
    // SAFETY: query name and terms are borrowed for this synchronous call.
    let Some(query_name) = (unsafe { oop::classify(api, query_name) }) else {
        return -4;
    };
    let query_terms = match unsafe { parse_rule_terms(api, query_terms, query_term_count) } {
        Ok(terms) => terms,
        Err(status) => return status,
    };
    let values = query_rule(
        &facts,
        &definitions,
        &query_name,
        &query_terms,
        result_variable,
    );
    unsafe { write_projected_values(&values, output, capacity, written) }
}


#[cfg(test)]
#[path = "native_output.rs"]
mod native_output;

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { API.get().expect("module not initialized").1 as *const birnam_abi::namespace_import::ApiV1 }
