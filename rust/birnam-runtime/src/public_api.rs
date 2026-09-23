//! Runtime entry operations retained while object-based integration is rebuilt.
//! The Runtime table, aggregate initializer and service factories are deleted.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::abi::{IdApi, Method, Oop};
use crate::boundary;
use crate::dispatch::Bootstrap;
#[cfg(test)]
use crate::dispatch::build_bootstrap;
use crate::runtime::{Runtime, with_active_if_present};

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

pub(crate) fn queue_native_retirement(
    context: *mut c_void,
    release: birnam_abi::private::NativeRetirementFn,
) {
    runtime().queue_native_retirement(context, release);
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
    unsafe { birnam_runtime_send_v1(receiver, selector, args, count) }
}

/// Checked language send; native entry results become owned Method objects.
/// # Safety
/// The receiver, selector and argument slice are live language Objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_runtime_send_v1(receiver: Oop, selector: Oop, args: *const Oop, count: usize) -> Oop {
    let runtime = runtime();
    runtime.enter_application(|| unsafe { runtime.send_abi(receiver, selector, args, count) })
}

/// # Safety
/// The native function belongs to the live owner and uses the Method ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_runtime_register_method_owner_v1(function: *const c_void, owner: Oop) -> i32 {
    i32::from(runtime().register_method_owner(function, owner))
}
/// # Safety
/// The owner is being finalized or released with no surviving callable users.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_runtime_forget_method_owner_v1(owner: Oop) {
    runtime().forget_method_owner(owner);
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

unsafe extern "C" fn api_gc_collect() -> usize {
    runtime().gc_collect()
}

unsafe extern "C" fn api_gc_live_count() -> usize {
    runtime().gc_live_count()
}

unsafe extern "C" fn api_object_payload_size(object: Oop) -> usize {
    runtime().payload_size(object).unwrap_or(0)
}

/// Canonical numeric function identities are established once and shared by
/// publication and declaration checks. Re-taking a private function's address
/// in another codegen unit can select a different compiled copy in release.
struct NumericFunctions {
    alloc: unsafe extern "C" fn(Oop, usize) -> Oop,
    vtable_of: unsafe extern "C" fn(Oop) -> Oop,
}

fn canonical_numeric_functions() -> &'static NumericFunctions {
    static FUNCTIONS: std::sync::OnceLock<NumericFunctions> = std::sync::OnceLock::new();
    FUNCTIONS.get_or_init(|| NumericFunctions {
        alloc: api_alloc,
        vtable_of: api_vtable_of,
    })
}

/// Process-local comparison operands for an optional native declaration. This
/// does not publish another IdApi entry or call any of these functions.
pub(crate) fn numeric_functions(runtime: &Runtime) -> [usize; 12] {
    let functions = canonical_numeric_functions();
    let ops = runtime.integer_operations();
    let presence = runtime.canonical_namespace_presence();
    let imports = runtime.canonical_namespace_import();
    [ops.is_exact as usize, ops.read as usize, presence.contains.unwrap() as usize,
     imports.import.unwrap() as usize, functions.vtable_of as usize, ops.make as usize,
     ops.context as usize, ops.storage as usize, ops.prototype as usize,
     canonical_numeric_functions().alloc as usize, presence.context as usize, imports.context as usize]
}

/// Current floor operands for the optional warm Block constructor declaration.
pub(crate) fn block_construction_context(
    runtime: &Runtime,
    symbol_vtable: Oop,
    metavtable: Oop,
) -> [usize; 14] {
    let imports = runtime.canonical_namespace_import();
    let names = runtime.canonical_namespace_names();
    let enumeration = runtime.canonical_vtable_enumeration();
    [
        canonical_numeric_functions().alloc as usize,
        imports.import.unwrap() as usize,
        api_object_payload_size as *const () as usize,
        canonical_numeric_functions().vtable_of as usize,
        api_send as *const () as usize,
        enumeration.selector_at.unwrap() as usize,
        api_symbol_name as *const () as usize,
        api_intern as *const () as usize,
        symbol_vtable as usize,
        names.name_of.unwrap() as usize,
        names.context as usize,
        metavtable as usize,
        imports.context as usize,
        enumeration.context as usize,
    ]
}

pub(crate) fn closure_codegen_context_matches(api: &IdApi, nil: Oop, integer: &birnam_abi::integer::IntegerApiV1, expected: &birnam_abi::integer::IntegerApiV1) -> bool {
    if !integer.same_contract(expected) { return false; }
    api.send as usize == api_send as *const () as usize
        && api.intern as usize == api_intern as *const () as usize
        && api.nilObj == nil
}

fn install_bootstrap_metadata(bootstrap: &Bootstrap, base_values_exist: bool, namespace_exists: bool) {
    let canonical = bootstrap.runtime.canonical();
    for vtable in [
        canonical.nil_vt,
        canonical.bool_vt,
    ] {
        if base_values_exist { continue; }
        bootstrap
            .runtime
            .set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
    }
    if !namespace_exists { bootstrap.runtime.publish_canonical_exports(); }
    bootstrap.runtime.install_execution_objects();
    bootstrap.runtime.install_block_objects();
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

}

pub(crate) fn canonical_allocator() -> usize { canonical_numeric_functions().alloc as usize }
