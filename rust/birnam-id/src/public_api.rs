//! Complete-shaped R2.6 API table with explicit fatal gaps.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::abi::{Finalizer, IdApi, Method, MethodOverwriteHook, Oop, RootProvider, TraceHook};
use crate::boundary;
use crate::dispatch::{Bootstrap, build_bootstrap};
use crate::runtime::Runtime;

static PUBLIC_RUNTIME: AtomicPtr<Runtime> = AtomicPtr::new(core::ptr::null_mut());

fn runtime() -> &'static Runtime {
    let pointer = PUBLIC_RUNTIME.load(Ordering::Acquire);
    boundary::require_invariant(!pointer.is_null(), "Rust IdApi used before id_init");
    // SAFETY: id_init leaks the owning Bootstrap before returning the API, so
    // this exact Runtime remains live for the process. Runtime enforces its
    // constructing-thread ownership on every operation.
    unsafe { &*pointer }
}

unsafe extern "C" fn api_intern(name: *const c_char) -> Oop {
    boundary::require_invariant(!name.is_null(), "IdApi.intern name is null");
    // SAFETY: the C ABI requires a live NUL-terminated name pointer.
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
    runtime().allocate_instance(vtable, payload_size)
}

unsafe extern "C" fn api_send(receiver: Oop, selector: Oop, args: *const Oop, count: usize) -> Oop {
    runtime().enter(|| {
        // SAFETY: the public wrapper forwards the IdApi argument-buffer contract.
        unsafe { runtime().send_abi(receiver, selector, args, count) }
    })
}

unsafe extern "C" fn api_symbol_name(symbol: Oop) -> *const c_char {
    runtime().symbol_name(symbol)
}

unsafe extern "C" fn api_vtable_of(object: Oop) -> Oop {
    runtime().checked_vtable_of(object)
}

unsafe extern "C" fn api_gc_set_stack_bottom(address: usize) {
    runtime().gc_set_stack_bottom(address);
}

unsafe extern "C" fn api_gc_register_root_provider(provider: RootProvider) {
    runtime().gc_register_root_provider(provider);
}

unsafe extern "C" fn api_gc_collect() -> usize {
    runtime().gc_collect_disabled()
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

unsafe extern "C" fn api_set_owner(vtable: Oop, owner: Oop) {
    runtime().set_vtable_owner(vtable, owner);
}

unsafe extern "C" fn api_set_parent(vtable: Oop, parent: Oop) {
    runtime().set_vtable_parent(vtable, parent);
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

unsafe extern "C" fn api_vtable_parent(vtable: Oop) -> Oop {
    runtime().vtable_parent(vtable)
}

unsafe extern "C" fn api_vtable_owner(vtable: Oop) -> Oop {
    runtime().vtable_owner(vtable)
}

macro_rules! fatal_stub {
    ($name:ident($($argument:ident: $argument_type:ty),* $(,)?) -> $result:ty, $field:literal) => {
        unsafe extern "C" fn $name($($argument: $argument_type),*) -> $result {
            $(let _ = $argument;)*
            boundary::fatal_invariant(concat!(
                "Rust R2.6 IdApi field is unsupported: ",
                $field,
            ))
        }
    };
}

fatal_stub!(unsupported_dlopen(path: *const c_char, mode: c_int) -> *mut c_void, "dlopen");
fatal_stub!(unsupported_dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void, "dlsym");
fatal_stub!(unsupported_import(name: *const c_char) -> Oop, "import");
fatal_stub!(unsupported_export_object(name: *const c_char, object: Oop) -> Oop, "exportObj");
fatal_stub!(unsupported_is_exported(name: *const c_char) -> c_int, "isExported");
fatal_stub!(unsupported_make_smallint(value: i64) -> Oop, "mkSmallInt");
fatal_stub!(unsupported_smallint_value(object: Oop) -> i64, "smallIntValue");
fatal_stub!(unsupported_is_smallint(object: Oop) -> c_int, "isSmallInt");
fatal_stub!(unsupported_make_string(bytes: *const u8, length: usize) -> Oop, "mkString");
fatal_stub!(unsupported_string_bytes(object: Oop, length: *mut usize) -> *const u8, "stringBytes");
fatal_stub!(unsupported_make_float(value: f64) -> Oop, "mkFloat");
fatal_stub!(unsupported_float_value(object: Oop) -> f64, "floatValue");
fatal_stub!(unsupported_is_float(object: Oop) -> c_int, "isFloat");
fatal_stub!(unsupported_exported_name_of(object: Oop) -> *const c_char, "exportedNameOf");
fatal_stub!(unsupported_ensure_private_vtable(object: Oop) -> Oop, "ensurePrivateVtable");
fatal_stub!(unsupported_set_method_overwrite_hook(hook: Option<MethodOverwriteHook>) -> (), "setMethodOverwriteHook");
fatal_stub!(unsupported_make_word(value: u64) -> Oop, "mkWord");
fatal_stub!(unsupported_word_value(object: Oop) -> u64, "wordValue");
fatal_stub!(unsupported_is_word(object: Oop) -> c_int, "isWord");

fn table_for(bootstrap: &Bootstrap) -> IdApi {
    let globals = bootstrap.runtime.globals();
    let canonical = bootstrap.runtime.canonical();
    IdApi {
        dlopen: unsupported_dlopen,
        dlsym: unsupported_dlsym,
        object_vt: globals.object_vt,
        vtable_vt: globals.vtable_vt,
        symbol_vt: globals.symbol_vt,
        intern: api_intern,
        proto: api_proto,
        method: api_method,
        alloc: api_alloc,
        import: unsupported_import,
        exportObj: unsupported_export_object,
        isExported: unsupported_is_exported,
        send: api_send,
        smallint_vt: canonical.smallint_vt,
        nilObj: canonical.nil,
        trueObj: canonical.true_object,
        falseObj: canonical.false_object,
        mkSmallInt: unsupported_make_smallint,
        smallIntValue: unsupported_smallint_value,
        isSmallInt: unsupported_is_smallint,
        string_vt: canonical.string_vt,
        mkString: unsupported_make_string,
        stringBytes: unsupported_string_bytes,
        symbolName: api_symbol_name,
        vtableOf: api_vtable_of,
        gcSetStackBottom: api_gc_set_stack_bottom,
        gcRegisterRootProvider: api_gc_register_root_provider,
        gcCollect: api_gc_collect,
        gcLiveCount: api_gc_live_count,
        setTraceHook: api_set_trace_hook,
        gcSetAuto: api_gc_set_auto,
        float_vt: canonical.float_vt,
        mkFloat: unsupported_make_float,
        floatValue: unsupported_float_value,
        isFloat: unsupported_is_float,
        setFinalizer: api_set_finalizer,
        setLayout: api_set_layout,
        gcSetPrecise: api_gc_set_precise,
        objectPayloadSize: api_object_payload_size,
        exportedNameOf: unsupported_exported_name_of,
        slotKind: api_slot_kind,
        ensurePrivateVtable: unsupported_ensure_private_vtable,
        setOwner: api_set_owner,
        setParent: api_set_parent,
        setMethodOverwriteHook: unsupported_set_method_overwrite_hook,
        vtableCount: api_vtable_count,
        vtableAt: api_vtable_at,
        vtableSelectorAt: api_vtable_selector_at,
        vtableParent: api_vtable_parent,
        mkWord: unsupported_make_word,
        wordValue: unsupported_word_value,
        isWord: unsupported_is_word,
        word_vt: canonical.word_vt,
        vtableOwner: api_vtable_owner,
    }
}

/// Perform the one supported process-lifetime initialization transition.
pub(crate) fn initialize_once() -> *mut IdApi {
    let bootstrap = Box::new(build_bootstrap());
    let canonical = bootstrap.runtime.canonical();
    for vtable in [
        canonical.string_vt,
        canonical.smallint_vt,
        canonical.float_vt,
        canonical.word_vt,
        canonical.nil_vt,
        canonical.bool_vt,
    ] {
        bootstrap
            .runtime
            .set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
    }
    let api = Box::new(table_for(&bootstrap));
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
    Box::into_raw(api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_table_has_bootstrap_identities_and_no_null_function_slots() {
        let bootstrap = build_bootstrap();
        let table = table_for(&bootstrap);
        let globals = bootstrap.runtime.globals();
        let canonical = bootstrap.runtime.canonical();
        assert_eq!(table.object_vt, globals.object_vt);
        assert_eq!(table.vtable_vt, globals.vtable_vt);
        assert_eq!(table.symbol_vt, globals.symbol_vt);
        assert_eq!(table.smallint_vt, canonical.smallint_vt);
        assert_eq!(table.nilObj, canonical.nil);
        assert_eq!(table.trueObj, canonical.true_object);
        assert_eq!(table.falseObj, canonical.false_object);
        assert_eq!(table.string_vt, canonical.string_vt);
        assert_eq!(table.float_vt, canonical.float_vt);
        assert_eq!(table.word_vt, canonical.word_vt);
        assert_eq!(size_of::<IdApi>(), 54 * size_of::<usize>());
    }
}
