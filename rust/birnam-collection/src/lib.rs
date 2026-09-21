//! Collection parent and ordered native family construction.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, MarkFn, Oop};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Once, OnceLock};

const RUST_COLLECTION_IDENTITY: &[u8] = b"rust-collection-r1-vector\0";
const COLLECTION_NAME: &[u8] = b"Collection\0";
const COLLECTION_VT_NAME: &[u8] = b"CollectionVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    collection_vt: usize,
    value: usize,
    value1: usize,
    value2: usize,
    print_string: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.collection_vt,
            runtime.value,
            runtime.value1,
            runtime.value2,
            runtime.print_string,
        ] {
            // SAFETY: these are the collection module's process-owned cached
            // Oops and remain live while its methods can execute.
            unsafe { mark(value as Oop) };
        }
    }
}
fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Collection: {message}");
    std::process::abort();
}

unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    // SAFETY: selectors are static NUL-terminated byte strings.
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
}

unsafe fn import(imports: *const birnam_abi::namespace_import::ApiV1, name: &[u8]) -> Oop {
    // SAFETY: names are static NUL-terminated byte strings.
    unsafe { birnam_abi::namespace_import::import(imports, name.as_ptr().cast::<c_char>()) }
}

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, VECTOR_NAME.as_ptr().cast()) != 0 };
        let collection_vt = if reloading {
            unsafe { import(imports, COLLECTION_VT_NAME) }
        } else {
            unsafe { (api.proto)(api.object_vt) }
        };
        let vector = unsafe { birnam_abi::vector::prepare(api_pointer, imports, roots, collection_vt, reloading) };
        let range_vt = unsafe { birnam_abi::range::prepare(api_pointer, imports, roots, collection_vt, reloading) };
        let map = unsafe { birnam_abi::map::prepare(api_pointer, imports, roots, collection_vt, reloading) };
        if [collection_vt, vector.node, vector.vector, range_vt, map.map, map.node]
            .iter().any(|value| value.is_null())
        {
            abort("Rust Collection exports were incomplete");
        }
        unsafe { birnam_abi::vector::finish_prototype(api_pointer) };
        unsafe { birnam_abi::range::finish_prototype(api_pointer) };
        unsafe { birnam_abi::map::finish_prototype(api_pointer) };
        Runtime {
            api: api_pointer as usize,
            collection_vt: collection_vt as usize,
            value: unsafe { intern(api, b"value\0") } as usize,
            value1: unsafe { intern(api, b"value:\0") } as usize,
            value2: unsafe { intern(api, b"value:value:\0") } as usize,
            print_string: unsafe { intern(api, b"printString\0") } as usize,
        }
    });
    ROOTS_REGISTERED.call_once(|| unsafe {
        birnam_abi::root_registry::register(roots, mark_roots);
    });
    unsafe {
        birnam_abi::vector::set_layout(api_pointer, layouts);
        birnam_abi::range::set_layout(api_pointer, layouts);
        birnam_abi::map::set_layout(api_pointer, layouts);
        birnam_abi::set_vtable_parent(api, birnam_abi::string::behavior(), runtime.collection_vt as Oop);
        birnam_abi::string_collection::install(api_pointer, runtime.api as *const IdApi);
        let vector_comma = birnam_abi::vector::install(
            api_pointer,
            runtime.value1 as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
        );
        birnam_abi::range::install(
            api_pointer,
            runtime.value1 as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
            vector_comma,
        );
        birnam_abi::map::install(
            api_pointer,
            runtime.value as Oop,
            runtime.value2 as Oop,
            runtime.print_string as Oop,
        );
    }
    if unsafe { birnam_abi::namespace_presence::contains(presence, VECTOR_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            birnam_abi::vector::set_owner(api_pointer);
            let _ = birnam_abi::namespace_export::export(exports,
                COLLECTION_VT_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            let _ = birnam_abi::namespace_export::export(exports,
                COLLECTION_NAME.as_ptr().cast(),
                runtime.collection_vt as Oop,
            );
            birnam_abi::vector::publish(api_pointer, exports);
            birnam_abi::range::publish(api_pointer, exports);
            birnam_abi::map::publish(api_pointer, exports);
        }
    }
}

/// Identifies this explicit Rust Collection vector fixture.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_collection_rust_identity() -> *const c_char {
    RUST_COLLECTION_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust Collection vector protocol against the process floor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    let _ = state;
    unsafe { bring_up(api, presence, imports, exports, roots, layouts) };
    ptr::null_mut()
}

birnam_abi::map_dependency!();

birnam_abi::string_dependency!();

birnam_abi::range_dependency!();

birnam_abi::vector_dependency!();

birnam_abi::string_collection_dependency!();

fn runtime() -> &'static Runtime { RUNTIME.get().expect("Collection base not constructed") }

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, presence, imports, exports, roots, layouts]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let (Some(api), Some(presence), Some(imports), Some(exports), Some(roots), Some(layouts)) = (api.as_word(), presence.as_word(), imports.as_word(), exports.as_word(), roots.as_word(), layouts.as_word()) else { return invalid_native(); };
    let api = api as *mut IdApi;
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let layouts = layouts as *const birnam_abi::payload_layout::ApiV1;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || !unsafe { birnam_abi::namespace_presence::valid(presence) && birnam_abi::namespace_import::valid(imports)
            && birnam_abi::namespace_export::valid(exports) && birnam_abi::root_registry::valid_owned(roots)
            && birnam_abi::payload_layout::valid(layouts) } { return invalid_native(); }
    if RUNTIME.get().is_some_and(|selected| selected.api != api.addr()) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::object(runtime().collection_vt as Oop)), _ => return invalid_native(),
    }
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let api_ref = unsafe { &*api };
    let parent = unsafe { retained.keep((api_ref.proto)(api_ref.object_vt)) };
    let value = unsafe { retained.keep(intern(api_ref, b"value\0")) };
    let value1 = unsafe { retained.keep(intern(api_ref, b"value:\0")) };
    let value2 = unsafe { retained.keep(intern(api_ref, b"value:value:\0")) };
    let print_string = unsafe { retained.keep(intern(api_ref, b"printString\0")) };
    RUNTIME.set(Runtime { api: api.addr(), collection_vt: parent.addr(), value: value.addr(), value1: value1.addr(), value2: value2.addr(), print_string: print_string.addr() }).ok().expect("serialized Collection construction");
    ROOTS_REGISTERED.call_once(|| unsafe { birnam_abi::root_registry::register((*roots).api, mark_roots) });
    unsafe {
        birnam_abi::namespace_export::export(exports, COLLECTION_VT_NAME.as_ptr().cast(), parent);
        birnam_abi::namespace_export::export(exports, COLLECTION_NAME.as_ptr().cast(), parent);
    }
    NATIVE_PHASE.store(2, std::sync::atomic::Ordering::Release);
    Outcome::loaded(Operand::object(runtime().collection_vt as Oop))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
