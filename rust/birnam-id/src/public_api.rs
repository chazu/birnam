//! Current IdApi table backed only by production implementations.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::abi::{Finalizer, IdApi, Method, MethodOverwriteHook, Oop, RootProvider, TraceHook};
use crate::boundary;
use crate::dispatch::{Bootstrap, build_bootstrap};
use crate::runtime::{Runtime, with_active_if_present};

static PUBLIC_API: AtomicPtr<IdApi> = AtomicPtr::new(core::ptr::null_mut());

static PUBLIC_RUNTIME: AtomicPtr<Runtime> = AtomicPtr::new(core::ptr::null_mut());

fn runtime() -> &'static Runtime {
    let pointer = PUBLIC_RUNTIME.load(Ordering::Acquire);
    boundary::require_invariant(!pointer.is_null(), "Rust IdApi used before id_init");
    // SAFETY: id_init leaks the owning Bootstrap before returning the API, so
    // this exact Runtime remains live for the process. Runtime enforces its
    // constructing-thread ownership on every operation.
    let runtime = unsafe { &*pointer };
    runtime.require_storage_access();
    runtime
}

/// Host observation rejects nested entry without going through ordinary access.
pub(crate) unsafe fn observe_storage(request: *const birnam_abi::storage::RequestV1) -> i32 {
    let pointer = PUBLIC_RUNTIME.load(Ordering::Acquire);
    if pointer.is_null() {
        return birnam_abi::storage::BUSY;
    }
    unsafe { (&*pointer).observe_storage(request) }
}

#[cfg(test)]
pub(crate) fn runtime_for_string_cleanup_test() -> &'static Runtime {
    runtime()
}

unsafe extern "C" fn api_dlopen(path: *const c_char, mode: c_int) -> *mut c_void {
    let _ = runtime();
    // SAFETY: this is a transparent pass-through of the platform loader ABI.
    unsafe { crate::loader::open(path, mode) }
}

unsafe extern "C" fn api_dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    let _ = runtime();
    // SAFETY: this is a transparent pass-through of the platform loader ABI.
    unsafe { crate::loader::symbol(handle, name) }
}

unsafe extern "C" fn api_intern(name: *const c_char) -> Oop {
    boundary::require_invariant(!name.is_null(), "IdApi.intern name is null");
    // SAFETY: the C ABI requires a live NUL-terminated name pointer, and the
    // bytes do not escape this synchronous call.
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    runtime()
        .intern(bytes)
        .unwrap_or_else(|_| boundary::fatal_invariant("IdApi.intern name contains embedded NUL"))
}

unsafe extern "C" fn api_import(name: *const c_char) -> Oop {
    boundary::require_invariant(!name.is_null(), "IdApi.import name is null");
    // SAFETY: the C ABI requires a live NUL-terminated name pointer, and the
    // bytes do not escape this synchronous call.
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    runtime()
        .import(bytes)
        .unwrap_or_else(|_| boundary::fatal_invariant("IdApi.import name contains embedded NUL"))
}

unsafe extern "C" fn api_export_object(name: *const c_char, object: Oop) -> Oop {
    boundary::require_invariant(!name.is_null(), "IdApi.exportObj name is null");
    // SAFETY: the C ABI requires a live NUL-terminated name pointer, and the
    // bytes do not escape this synchronous call.
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    runtime()
        .export(bytes, object)
        .unwrap_or_else(|_| boundary::fatal_invariant("IdApi.exportObj name contains embedded NUL"))
}

unsafe extern "C" fn api_proto(parent: Oop) -> Oop {
    boundary::require_invariant(!parent.is_null(), "IdApi.proto parent is null");
    runtime().create_vtable(parent)
}

unsafe extern "C" fn api_method(vtable: Oop, selector: Oop, method: Method) {
    let _ = runtime().add_method(vtable, selector, method);
}

unsafe extern "C" fn api_alloc(vtable: Oop, payload_size: usize) -> Oop {
    if let Some(object) = with_active_if_present(|active| {
        // SAFETY: with_active_if_present proves a surrounding owner-checked
        // Runtime::enter for this exact runtime.
        unsafe { active.allocate_instance_active(vtable, payload_size) }
    }) {
        return object;
    }
    runtime().allocate_instance(vtable, payload_size)
}

unsafe extern "C" fn api_send(receiver: Oop, selector: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = runtime();
    runtime.enter(|| {
        // SAFETY: the public wrapper forwards the IdApi argument-buffer contract.
        unsafe { runtime.send_abi(receiver, selector, args, count) }
    })
}

/// Test-only access to the exact public send wrapper while a foreign Method is
/// executing inside an active runtime.
///
/// # Safety
///
/// The ordinary `IdApi.send` receiver, selector, and argument-buffer contracts
/// apply.
#[cfg(feature = "ffi-test-hooks")]
pub(crate) unsafe fn test_active_send(
    receiver: Oop,
    selector: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    // SAFETY: forwarded caller contract.
    unsafe { api_send(receiver, selector, args, count) }
}

#[cfg(feature = "ffi-test-hooks")]
unsafe extern "C" fn external_null_answer(_: Oop, args: *const Oop, count: usize) -> Oop {
    if args.is_null() && count == 0 {
        4242usize as Oop
    } else {
        core::ptr::null_mut()
    }
}

/// Install and invoke one foreign reentrant Method through the actual
/// process-lifetime public runtime.
#[cfg(feature = "ffi-test-hooks")]
pub(crate) fn test_dispatch_reentry(method: Method) -> usize {
    let _ = initialize_once();
    let runtime = runtime();
    let globals = runtime.globals();
    let inner = runtime
        .intern(b"externalInner")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let outer = runtime
        .intern(b"externalOuter:")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let receiver = runtime.allocate_instance(globals.object_vt, 0);
    let _ = runtime.add_method(globals.object_vt, inner, external_null_answer);
    let _ = runtime.add_method(globals.object_vt, outer, method);
    runtime
        .enter(|| runtime.send_slice(receiver, outer, &[inner]))
        .addr()
}

/// This exact cleanup routes only to the process-published Runtime and removes
/// its private byte owner without reading Id state or invoking another callback.
/// The caller must additionally prove that this Runtime owns the object's bytes.
#[cfg(test)]
pub(crate) fn string_cleanup_matches_runtime(active: &Runtime, finalizer: Finalizer) -> bool {
    active.string_cleanup_matches_runtime(finalizer)
}
#[cfg(test)]
pub(crate) unsafe extern "C" fn string_finalizer(object: Oop) {
    runtime().release_string(object);
}
#[cfg(test)]
pub(crate) unsafe extern "C" fn test_string_cleanup_owner(finalizer: Finalizer) -> *mut c_void {
    let pointer = PUBLIC_RUNTIME.load(Ordering::Acquire);
    if finalizer as usize != string_finalizer as *const () as usize || pointer.is_null() { return core::ptr::null_mut(); }
    unsafe { (*pointer).string_context() }
}

unsafe extern "C" fn api_symbol_name(symbol: Oop) -> *const c_char {
    if let Some(name) = with_active_if_present(|active| {
        // SAFETY: with_active_if_present proves a surrounding owner-checked
        // Runtime::enter. The caller owns the obligation to keep `symbol`
        // rooted for the returned pointer's complete use.
        unsafe { active.symbol_name_active(symbol) }
    }) {
        return name;
    }
    runtime().symbol_name(symbol)
}

unsafe extern "C" fn api_vtable_of(object: Oop) -> Oop {
    if let Some(vtable) = with_active_if_present(|active| {
        // SAFETY: with_active_if_present proves a surrounding owner-checked
        // Runtime::enter; the lookup itself retains exact heap membership.
        unsafe { active.checked_vtable_of_active(object) }
    }) {
        return vtable;
    }
    runtime().checked_vtable_of(object)
}

unsafe extern "C" fn api_gc_set_stack_bottom(address: usize) {
    runtime().gc_set_stack_bottom(address);
}

unsafe extern "C" fn api_gc_register_root_provider(provider: RootProvider) {
    runtime().gc_register_root_provider(provider);
}

unsafe extern "C" fn api_gc_collect() -> usize {
    runtime().gc_collect()
}

unsafe extern "C" fn api_gc_live_count() -> usize {
    runtime().gc_live_count()
}

unsafe extern "C" fn api_set_trace_hook(vtable: Oop, hook: TraceHook) {
    runtime().set_trace_hook(vtable, hook);
}

unsafe extern "C" fn api_gc_set_auto(enabled: c_int) {
    runtime().gc_set_auto_requested(enabled != 0);
}

unsafe extern "C" fn api_set_finalizer(object: Oop, finalizer: Finalizer) {
    runtime().set_finalizer(object, finalizer);
}

unsafe extern "C" fn api_set_layout(vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
    runtime().set_layout(vtable, kinds, count, rest);
}

unsafe extern "C" fn api_gc_set_precise(enabled: c_int) {
    runtime().gc_set_precise(enabled != 0);
}

unsafe extern "C" fn api_object_payload_size(object: Oop) -> usize {
    runtime().payload_size(object).unwrap_or(0)
}

unsafe extern "C" fn api_slot_kind(vtable: Oop, word: usize) -> u8 {
    runtime().slot_kind(vtable, word)
}

unsafe extern "C" fn api_set_method_overwrite_hook(hook: Option<MethodOverwriteHook>) {
    runtime().set_method_overwrite_hook(hook);
}

unsafe extern "C" fn api_vtable_count() -> usize {
    runtime().vtable_count()
}

unsafe extern "C" fn api_vtable_at(index: usize) -> Oop {
    runtime().vtable_at(index)
}

unsafe extern "C" fn api_vtable_selector_at(vtable: Oop, index: usize) -> Oop {
    runtime().vtable_selector_at(vtable, index)
}

fn table_for(bootstrap: &Bootstrap) -> IdApi {
    let globals = bootstrap.runtime.globals();
    let canonical = bootstrap.runtime.canonical();
    IdApi {
        dlopen: api_dlopen,
        dlsym: api_dlsym,
        object_vt: globals.object_vt,
        vtable_vt: globals.vtable_vt,
        symbol_vt: globals.symbol_vt,
        intern: api_intern,
        proto: api_proto,
        method: api_method,
        alloc: canonical_numeric_functions().alloc,
        import: canonical_numeric_functions().import,
        exportObj: api_export_object,
        send: api_send,
        nilObj: canonical.nil,
        trueObj: canonical.true_object,
        falseObj: canonical.false_object,
        symbolName: api_symbol_name,
        vtableOf: canonical_numeric_functions().vtable_of,
        gcSetStackBottom: api_gc_set_stack_bottom,
        gcRegisterRootProvider: api_gc_register_root_provider,
        gcCollect: api_gc_collect,
        gcLiveCount: api_gc_live_count,
        setTraceHook: api_set_trace_hook,
        gcSetAuto: api_gc_set_auto,
        setFinalizer: api_set_finalizer,
        setLayout: api_set_layout,
        gcSetPrecise: api_gc_set_precise,
        objectPayloadSize: api_object_payload_size,
        slotKind: api_slot_kind,
        setMethodOverwriteHook: api_set_method_overwrite_hook,
        vtableCount: api_vtable_count,
        vtableAt: api_vtable_at,
        vtableSelectorAt: api_vtable_selector_at,
    }
}

/// Canonical numeric function identities are established once and shared by
/// publication and declaration checks. Re-taking a private function's address
/// in another codegen unit can select a different compiled copy in release.
struct NumericFunctions {
    alloc: unsafe extern "C" fn(Oop, usize) -> Oop,
    import: unsafe extern "C" fn(*const c_char) -> Oop,
    vtable_of: unsafe extern "C" fn(Oop) -> Oop,
}

fn canonical_numeric_functions() -> &'static NumericFunctions {
    static FUNCTIONS: std::sync::OnceLock<NumericFunctions> = std::sync::OnceLock::new();
    FUNCTIONS.get_or_init(|| NumericFunctions {
        alloc: api_alloc,
        import: api_import,
        vtable_of: api_vtable_of,
    })
}

/// Process-local comparison operands for an optional native declaration. This
/// does not publish another IdApi entry or call any of these functions.
pub(crate) fn numeric_functions(runtime: &Runtime) -> [usize; 11] {
    let functions = canonical_numeric_functions();
    let ops = runtime.integer_operations();
    let presence = runtime.canonical_namespace_presence();
    [ops.is_exact as usize, ops.read as usize, presence.contains.unwrap() as usize,
     functions.import as usize, functions.vtable_of as usize, ops.make as usize,
     ops.context as usize, ops.storage as usize, ops.prototype as usize,
     canonical_numeric_functions().alloc as usize, presence.context as usize]
}

/// Current floor operands for the optional warm Block constructor declaration.
pub(crate) fn block_construction_context(
    runtime: &Runtime,
    symbol_vtable: Oop,
    metavtable: Oop,
) -> [usize; 12] {
    let names = runtime.canonical_namespace_names();
    [
        canonical_numeric_functions().alloc as usize,
        canonical_numeric_functions().import as usize,
        api_object_payload_size as *const () as usize,
        canonical_numeric_functions().vtable_of as usize,
        api_send as *const () as usize,
        api_vtable_selector_at as *const () as usize,
        api_symbol_name as *const () as usize,
        api_intern as *const () as usize,
        symbol_vtable as usize,
        names.name_of.unwrap() as usize,
        names.context as usize,
        metavtable as usize,
    ]
}

pub(crate) fn closure_codegen_context_matches(api: &IdApi, nil: Oop, integer: &birnam_abi::integer::IntegerApiV1, expected: &birnam_abi::integer::IntegerApiV1) -> bool {
    if !integer.same_contract(expected) { return false; }
    api.send as usize == api_send as *const () as usize
        && api.intern as usize == api_intern as *const () as usize
        && api.nilObj == nil
}

fn install_bootstrap_metadata(bootstrap: &Bootstrap) {
    let canonical = bootstrap.runtime.canonical();
    for vtable in [
        canonical.nil_vt,
        canonical.bool_vt,
    ] {
        bootstrap
            .runtime
            .set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
    }
    bootstrap.runtime.publish_canonical_exports();
    bootstrap.runtime.install_execution_objects();
}

/// Perform the one supported process-lifetime initialization transition.
pub(crate) fn initialize_once() -> *mut IdApi {
    let bootstrap = Box::new(build_bootstrap());
    install_bootstrap_metadata(&bootstrap);
    let mut api = Box::new(table_for(&bootstrap));
    let api_pointer = core::ptr::from_mut(api.as_mut());
    let runtime_pointer = core::ptr::from_ref(&bootstrap.runtime).cast_mut();
    if PUBLIC_RUNTIME
        .compare_exchange(
            core::ptr::null_mut(),
            runtime_pointer,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        boundary::fatal_invariant("Rust id_init called more than once");
    }
    let _ = Box::into_raw(bootstrap);
    let _ = Box::into_raw(api);
    NATIVE_ALLOCATION.store(Box::into_raw(Box::new(runtime().native_allocation())), Ordering::Release);
    let enumeration = unsafe { &*runtime().namespace_enumeration() };
    unsafe {
        birnam_namespace_count_v1.publish(&enumeration.count);
        birnam_namespace_name_at_v1.publish(&enumeration.name_at);
    }
    PUBLIC_API.store(api_pointer, Ordering::Release);
    api_pointer
}

pub(crate) fn configure_symbol_compaction(
    enabled: bool,
    compatibility_count_limit: usize,
    compatibility_name_byte_limit: usize,
) -> bool {
    runtime().configure_symbol_compaction(
        enabled,
        compatibility_count_limit,
        compatibility_name_byte_limit,
    )
}

pub(crate) fn live_symbol_count() -> usize {
    runtime().symbol_count()
}

pub(crate) fn live_symbol_name_bytes() -> usize {
    runtime().symbol_owned_name_bytes()
}

pub(crate) fn live_heap_bytes() -> usize {
    runtime().gc_live_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_context_requires_active_nil_and_exact_codegen_operations() {
        // This guard must be usable with the owner-checked active runtime; it
        // must neither consult the singleton nor call an instrumented operation.
        let bootstrap = build_bootstrap();
        let other = build_bootstrap();
        let nil = bootstrap.runtime.canonical().nil;
        let mut table = table_for(&bootstrap);
        let expected = bootstrap.runtime.integer_operations();
        let mut integer_ops = expected;
        assert!(closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
        assert!(!closure_codegen_context_matches(
            &table,
            other.runtime.canonical().nil, &integer_ops, &expected
        ));
        unsafe extern "C" fn send(_: Oop, _: Oop, _: *const Oop, _: usize) -> Oop {
            panic!("guard must not send");
        }
        unsafe extern "C" fn intern(_: *const core::ffi::c_char) -> Oop {
            panic!("guard must not intern");
        }
        unsafe extern "C" fn integer(_: *mut c_void, _: i64) -> Oop {
            panic!("guard must not allocate");
        }
        table.send = send;
        assert!(!closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
        table.send = api_send;
        table.intern = intern;
        assert!(!closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
        table.intern = api_intern;
        integer_ops.make = integer;
        assert!(!closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
        integer_ops.make = expected.make;
        table.nilObj = other.runtime.canonical().nil;
        assert!(!closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
        table.nilObj = nil;
        assert!(closure_codegen_context_matches(&table, nil, &integer_ops, &expected));
    }

    #[test]
    fn complete_table_has_bootstrap_identities_and_no_null_function_slots() {
        let bootstrap = build_bootstrap();
        install_bootstrap_metadata(&bootstrap);
        let table = table_for(&bootstrap);
        let globals = bootstrap.runtime.globals();
        let canonical = bootstrap.runtime.canonical();
        assert_eq!(table.object_vt, globals.object_vt);
        assert_eq!(table.vtable_vt, globals.vtable_vt);
        assert_eq!(table.symbol_vt, globals.symbol_vt);
        assert_eq!(table.nilObj, canonical.nil);
        assert_eq!(table.trueObj, canonical.true_object);
        assert_eq!(table.falseObj, canonical.false_object);
        assert_eq!(
            bootstrap.runtime.has_finalizer(bootstrap.runtime.string_prototype()),
            Some(true)
        );
        assert_eq!(
            size_of::<IdApi>(),
            crate::abi::ABI_FIELD_COUNT * size_of::<usize>()
        );
    }
}

/// Explicit dependency on the loaded collection coordinator.
/// # Safety
/// The readable table supplies code pinned for the runtime's entire lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_collect_dependency_v1(
    api: *const birnam_abi::collect::ApiV1,
) -> c_int {
    c_int::from(unsafe { runtime().attach_collect(api) })
}

/// Explicit traversal dependency of the still-combined collector.
/// # Safety
/// The readable table supplies code pinned for the runtime's entire lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_dependency_v1(
    api: *const birnam_abi::trace_live::ApiV1,
) -> c_int {
    c_int::from(unsafe { runtime().attach_trace(api) })
}

/// Explicit dependency of the still-combined Machine/optimizer layer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_integer_dependency_v1(ops: *mut birnam_abi::integer::IntegerApiV1) -> c_int {
    let attached = runtime().integer_dependency_attached();
    if !unsafe { runtime().attach_integer(ops) } { return 0; }
    if attached { return 1; }
    #[cfg(feature = "ffi-test-hooks")]
    if std::env::var_os("BIRNAM_TEST_CLOSURE_CONTROL").is_some_and(|v| v == "1") {
        runtime().install_closure_control_probe(PUBLIC_API.load(Ordering::Acquire));
    }
    #[cfg(feature = "ffi-test-hooks")]
    if std::env::var_os("BIRNAM_TEST_UNBOXED_NUMERIC").is_some_and(|v| v == "1") {
        runtime().install_unboxed_numeric_probe();
    }
    1
}

#[cfg(test)]
pub(crate) fn test_value_api(bootstrap: &Bootstrap) -> Box<IdApi> {
    use crate::runtime::with_active;
    unsafe extern "C" fn proto(parent: Oop) -> Oop { with_active(|r| r.create_vtable(parent)) }
    unsafe extern "C" fn export(name: *const c_char, value: Oop) -> Oop {
        with_active(|r| r.export(unsafe { CStr::from_ptr(name) }.to_bytes(), value).unwrap())
    }
    unsafe extern "C" fn intern(name: *const c_char) -> Oop {
        with_active(|r| r.intern(unsafe { CStr::from_ptr(name) }.to_bytes()).unwrap())
    }
    unsafe extern "C" fn layout(vt: Oop, kinds: *const u8, count: usize, rest: u8) {
        with_active(|r| r.set_layout(vt, kinds, count, rest));
    }
    unsafe extern "C" fn send(receiver: Oop, selector: Oop, args: *const Oop, count: usize) -> Oop {
        with_active(|r| unsafe { r.send_abi(receiver, selector, args, count) })
    }
    let mut api = Box::new(table_for(bootstrap));
    unsafe extern "C" fn method(vt: Oop, selector: Oop, method: Method) {
        with_active(|r| { r.add_method(vt, selector, method); });
    }
    api.method = method;
    api.proto = proto; api.exportObj = export; api.intern = intern;
    api.setLayout = layout; api.send = send;
    // alloc and vtableOf already route through the active runtime. Every
    // callback used by Instance is local to this test's owner-thread entry.
    api
}

pub(crate) fn canonical_allocator() -> usize { canonical_numeric_functions().alloc as usize }

/// Explicit dependency of the still-combined Machine and recovery layer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_float_dependency_v1(ops: *mut birnam_abi::float::FloatApiV1) -> c_int {
    c_int::from(unsafe { runtime().attach_float(ops) })
}

/// Explicit dependency of the still-combined Machine layer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_word_dependency_v1(ops: *mut birnam_abi::word::WordApiV1) -> c_int {
    c_int::from(unsafe { runtime().attach_word(ops) })
}

/// Explicit dependency of internal String construction and retirement checks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_string_dependency_v1(ops: *mut birnam_abi::string::StringApiV1) -> c_int {
    c_int::from(unsafe { runtime().attach_string(ops) })
}
static NATIVE_ALLOCATION: AtomicPtr<birnam_abi::native_allocation::NativeAllocationV1> = AtomicPtr::new(core::ptr::null_mut());
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_allocation_provider_v1() -> *const birnam_abi::native_allocation::NativeAllocationV1 {
    let _ = runtime();
    NATIVE_ALLOCATION.load(Ordering::Acquire)
}

/// Primitive floor facts for the namespace's reverse-name operation. Constructing
/// this descriptor does not inspect a Runtime or call into it.
pub(crate) fn namespace_name_access() -> birnam_abi::namespace_names::FloorV1 {
    unsafe extern "C" fn preflight(_: *mut c_void) {
        #[cfg(not(test))]
        runtime().namespace_name_preflight();
        #[cfg(test)]
        crate::runtime::with_active(|r| r.namespace_name_preflight());
    }
    unsafe extern "C" fn symbol_name(_: *mut c_void, symbol: Oop) -> *const c_char {
        #[cfg(not(test))]
        {
            unsafe { runtime().namespace_symbol_bytes(symbol) }
        }
        #[cfg(test)]
        {
            crate::runtime::with_active(|r| unsafe { r.namespace_symbol_bytes(symbol) })
        }
    }
    birnam_abi::namespace_names::FloorV1 {
        version: 1,
        byte_size: size_of::<birnam_abi::namespace_names::FloorV1>(),
        context: core::ptr::null_mut(),
        preflight: Some(preflight),
        symbol_name: Some(symbol_name),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_namespace_names_provider_v1()
-> *mut birnam_abi::namespace_names::ApiV1 {
    runtime().namespace_names()
}


use birnam_abi::namespace_enumeration::{CountV1, NameAtV1, PublicationV1};
#[unsafe(no_mangle)]
pub static birnam_namespace_count_v1: PublicationV1<CountV1> = PublicationV1::new();
#[unsafe(no_mangle)]
pub static birnam_namespace_name_at_v1: PublicationV1<NameAtV1> = PublicationV1::new();

pub(crate) fn namespace_enumeration_access() -> birnam_abi::namespace_enumeration::FloorV1 {
    unsafe extern "C" fn preflight(_: *mut c_void) {
        #[cfg(not(test))]
        runtime().namespace_name_preflight();
        #[cfg(test)]
        crate::runtime::with_active(|r| r.namespace_name_preflight());
    }
    birnam_abi::namespace_enumeration::FloorV1 {
        version: 1, byte_size: size_of::<birnam_abi::namespace_enumeration::FloorV1>(),
        context: core::ptr::null_mut(), preflight: Some(preflight),
    }
}

pub(crate) fn namespace_presence_access() -> birnam_abi::namespace_presence::FloorV1 {
    unsafe extern "C" fn intern(_: *mut c_void, bytes: *const u8, length: usize) -> Oop {
        let bytes = unsafe { birnam_abi::borrowed_slice(bytes, length) }.expect("invalid Namespace interner bytes");
        #[cfg(not(test))]
        let value = runtime().intern(bytes);
        #[cfg(test)]
        let value = crate::runtime::with_active(|runtime| runtime.intern(bytes));
        value.unwrap_or(core::ptr::null_mut())
    }
    birnam_abi::namespace_presence::FloorV1 { version: 1,
        byte_size: size_of::<birnam_abi::namespace_presence::FloorV1>(),
        context: core::ptr::null_mut(), intern: Some(intern), }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_namespace_presence_provider_v1() -> *mut birnam_abi::namespace_presence::ApiV1 {
    runtime().namespace_presence()
}
