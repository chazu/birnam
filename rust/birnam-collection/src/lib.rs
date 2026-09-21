//! Collection parent and ordered native family construction.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, MarkFn, Oop};
use core::ffi::c_char;
use std::sync::{Once, OnceLock};

const RUST_COLLECTION_IDENTITY: &[u8] = b"rust-collection-r1-vector\0";
const COLLECTION_NAME: &[u8] = b"Collection\0";
const COLLECTION_VT_NAME: &[u8] = b"CollectionVt\0";
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

unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
    // SAFETY: selectors are static NUL-terminated byte strings.
    unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
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
