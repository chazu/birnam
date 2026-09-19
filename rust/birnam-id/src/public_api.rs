//! Current IdApi table backed only by production implementations.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::abi::{Finalizer, IdApi, Method, MethodOverwriteHook, Oop, RootProvider, TraceHook};
use crate::boundary;
use crate::dispatch::{Bootstrap, build_bootstrap};
use crate::runtime::{Runtime, with_active_if_present};
use crate::values::{float_value_unchecked, word_value_unchecked};

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

unsafe extern "C" fn api_is_exported(name: *const c_char) -> c_int {
    i32::from(!unsafe { api_import(name) }.is_null())
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

unsafe extern "C" fn api_make_float(value: f64) -> Oop {
    runtime().make_float(value)
}

unsafe extern "C" fn api_float_value(object: Oop) -> f64 {
    // SAFETY: the frozen IdApi contract requires a live Float object. The
    // payload read needs no mutable runtime state; callers remain subject to
    // the floor's owner-thread rule.
    unsafe { float_value_unchecked(object) }
}

unsafe extern "C" fn api_is_float(object: Oop) -> c_int {
    i32::from(runtime().is_float(object))
}

unsafe extern "C" fn api_make_word(value: u64) -> Oop {
    runtime().make_word(value)
}

unsafe extern "C" fn api_word_value(object: Oop) -> u64 {
    // SAFETY: the frozen IdApi contract requires a live Word object. The
    // payload read needs no mutable runtime state; callers remain subject to
    // the floor's owner-thread rule.
    unsafe { word_value_unchecked(object) }
}

unsafe extern "C" fn api_is_word(object: Oop) -> c_int {
    i32::from(runtime().is_word(object))
}

/// This exact cleanup routes only to the process-published Runtime and removes
/// its private byte owner without reading Id state or invoking another callback.
/// The caller must additionally prove that this Runtime owns the object's bytes.
pub(crate) fn string_cleanup_matches_runtime(active: &Runtime, finalizer: Finalizer) -> bool {
    finalizer as usize == string_finalizer as *const () as usize
        && core::ptr::eq(PUBLIC_RUNTIME.load(Ordering::Acquire).cast_const(), active)
}

pub(crate) unsafe extern "C" fn string_finalizer(object: Oop) {
    runtime().release_string(object);
}

unsafe extern "C" fn api_make_string(bytes: *const u8, length: usize) -> Oop {
    boundary::require_invariant(
        length == 0 || !bytes.is_null(),
        "IdApi.mkString bytes is null with nonzero length",
    );
    let bytes = if length == 0 {
        &[][..]
    } else {
        // SAFETY: the IdApi caller supplies length readable bytes, which are
        // copied synchronously and never retained through this borrowed slice.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    if let Some(object) = with_active_if_present(|active| {
        // SAFETY: with_active_if_present proves a surrounding owner-checked
        // Runtime::enter for this exact runtime.
        unsafe { active.make_string_with_finalizer_active(bytes, string_finalizer) }
    }) {
        return object;
    }
    runtime().make_string_with_finalizer(bytes, string_finalizer)
}

unsafe extern "C" fn api_string_bytes(object: Oop, out_length: *mut usize) -> *const u8 {
    boundary::require_invariant(
        !out_length.is_null(),
        "IdApi.stringBytes length output is null",
    );
    let bytes = runtime()
        .string_bytes(object)
        .unwrap_or_else(|| boundary::fatal_invariant("IdApi.stringBytes object is not a String"));
    // SAFETY: caller supplies one writable usize and the stable boxed byte owner
    // remains in the runtime until the object's future finalization.
    unsafe { out_length.write(bytes.len()) };
    bytes.as_ptr()
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

unsafe extern "C" fn api_exported_name_of(object: Oop) -> *const c_char {
    runtime().exported_name(object)
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
        isExported: canonical_numeric_functions().is_exported,
        send: api_send,
        nilObj: canonical.nil,
        trueObj: canonical.true_object,
        falseObj: canonical.false_object,
        string_vt: canonical.string_vt,
        mkString: api_make_string,
        stringBytes: api_string_bytes,
        symbolName: api_symbol_name,
        vtableOf: canonical_numeric_functions().vtable_of,
        gcSetStackBottom: api_gc_set_stack_bottom,
        gcRegisterRootProvider: api_gc_register_root_provider,
        gcCollect: api_gc_collect,
        gcLiveCount: api_gc_live_count,
        setTraceHook: api_set_trace_hook,
        gcSetAuto: api_gc_set_auto,
        float_vt: canonical.float_vt,
        mkFloat: api_make_float,
        floatValue: api_float_value,
        isFloat: api_is_float,
        setFinalizer: api_set_finalizer,
        setLayout: api_set_layout,
        gcSetPrecise: api_gc_set_precise,
        objectPayloadSize: api_object_payload_size,
        exportedNameOf: api_exported_name_of,
        slotKind: api_slot_kind,
        setMethodOverwriteHook: api_set_method_overwrite_hook,
        vtableCount: api_vtable_count,
        vtableAt: api_vtable_at,
        vtableSelectorAt: api_vtable_selector_at,
        mkWord: api_make_word,
        wordValue: api_word_value,
        isWord: api_is_word,
        word_vt: canonical.word_vt,
    }
}

/// Canonical numeric function identities are established once and shared by
/// publication and declaration checks. Re-taking a private function's address
/// in another codegen unit can select a different compiled copy in release.
struct NumericFunctions {
    alloc: unsafe extern "C" fn(Oop, usize) -> Oop,
    is_exported: unsafe extern "C" fn(*const c_char) -> c_int,
    import: unsafe extern "C" fn(*const c_char) -> Oop,
    vtable_of: unsafe extern "C" fn(Oop) -> Oop,
}

fn canonical_numeric_functions() -> &'static NumericFunctions {
    static FUNCTIONS: std::sync::OnceLock<NumericFunctions> = std::sync::OnceLock::new();
    FUNCTIONS.get_or_init(|| NumericFunctions {
        alloc: api_alloc,
        is_exported: api_is_exported,
        import: api_import,
        vtable_of: api_vtable_of,
    })
}

/// Process-local comparison operands for an optional native declaration. This
/// does not publish another IdApi entry or call any of these functions.
pub(crate) fn numeric_functions(runtime: &Runtime) -> [usize; 10] {
    let functions = canonical_numeric_functions();
    let ops = runtime.integer_operations();
    [ops.is_exact as usize, ops.read as usize, functions.is_exported as usize,
     functions.import as usize, functions.vtable_of as usize, ops.make as usize,
     ops.context as usize, ops.storage as usize, ops.prototype as usize,
     canonical_numeric_functions().alloc as usize]
}

/// Current floor operands for the optional warm Block constructor declaration.
pub(crate) fn block_construction_context(symbol_vtable: Oop, metavtable: Oop) -> [usize; 11] {
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
        api_exported_name_of as *const () as usize,
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
        canonical.string_vt,
        canonical.float_vt,
        canonical.word_vt,
        canonical.nil_vt,
        canonical.bool_vt,
    ] {
        bootstrap
            .runtime
            .set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
    }
    bootstrap.runtime.publish_canonical_exports();
    bootstrap.runtime.install_execution_objects();
    bootstrap
        .runtime
        .set_finalizer(canonical.string_prototype, string_finalizer);
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

pub(crate) fn export_count() -> usize {
    runtime().export_count()
}

pub(crate) fn export_name_at(index: usize) -> Oop {
    runtime().export_name_at(index)
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
        assert_eq!(table.string_vt, canonical.string_vt);
        assert_eq!(table.float_vt, canonical.float_vt);
        assert_eq!(table.word_vt, canonical.word_vt);
        assert_eq!(
            bootstrap.runtime.has_finalizer(canonical.string_prototype),
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
pub(crate) fn test_integer_api(bootstrap: &Bootstrap) -> Box<IdApi> {
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
    api.proto = proto; api.exportObj = export; api.intern = intern;
    api.setLayout = layout; api.send = send;
    // alloc and vtableOf already route through the active runtime. Every
    // callback used by Instance is local to this test's owner-thread entry.
    api
}

pub(crate) fn canonical_allocator() -> usize { canonical_numeric_functions().alloc as usize }
