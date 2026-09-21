//! Reified PortableProgram ordinary-object rung.
//!
//! This module is loaded after Collection and Error. It owns the private
//! native payloads behind CodeObject, CodeProgram, CodeLexicalBinding,
//! CodeStatement, and CodeNode while exposing their behavior only through
//! ordinary sends and the frozen IdApi.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::IdApi;
use core::ffi::c_char;

mod abi {
    pub use birnam_abi::{MarkFn, Method, Oop, SLOT_RAW};
}

mod boundary {
    pub fn fatal_invariant(message: &str) -> ! {
        eprintln!("birnam Portable Code invariant: {message}");
        std::process::abort();
    }
}

mod heap {
    #[derive(Clone, Copy)]
    pub enum Permanence {
        Permanent,
    }
}

mod public_api {
    use birnam_abi::Oop;

    // Runtime::make_string_with_finalizer_* deliberately ignores this token:
    // IdApi.mkString installs the floor-owned String finalizer.
    pub unsafe extern "C" fn string_finalizer(_object: Oop) {}
}

mod runtime;

mod portable_code;

const IDENTITY: &[u8] = b"rust-portable-code-k1\0";

#[unsafe(no_mangle)]
pub extern "C" fn birnam_portable_code_rust_identity() -> *const c_char {
    IDENTITY.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    use std::sync::atomic::Ordering;
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, presence, imports, exports, roots, layouts, vtables, hooks, finalizers, integer, string, names, vector, error]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let (Some(api), Some(presence), Some(imports), Some(exports), Some(roots), Some(layouts), Some(vtables), Some(hooks), Some(finalizers), Some(integer), Some(string), Some(names)) = (api.as_word(), presence.as_word(), imports.as_word(), exports.as_word(), roots.as_word(), layouts.as_word(), vtables.as_word(), hooks.as_word(), finalizers.as_word(), integer.as_word(), string.as_word(), names.as_word()) else { return invalid_native(); };
    let api = api as *mut IdApi;
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let layouts = layouts as *const birnam_abi::payload_layout::ApiV1;
    let vtables = vtables as *const birnam_abi::vtable_enumeration::ApiV2;
    let hooks = hooks as *const birnam_abi::trace_hooks::ApiV1;
    let finalizers = finalizers as *const birnam_abi::finalizers::ApiV1;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || !unsafe { birnam_abi::namespace_presence::valid(presence) && birnam_abi::namespace_import::valid(imports) && birnam_abi::namespace_export::valid(exports) && birnam_abi::root_registry::valid_owned(roots) && birnam_abi::payload_layout::valid(layouts) && birnam_abi::vtable_enumeration::valid(vtables) && birnam_abi::trace_hooks::valid(hooks) && birnam_abi::finalizers::valid(finalizers) } { return invalid_native(); }
    let Some(_vector) = vector.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    let Some(_error) = error.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if unsafe { birnam_abi::integer::attach(integer as _) } != 1 || unsafe { birnam_abi::string::attach(string as _) } != 1 || unsafe { birnam_abi::namespace_names::attach(names as _) } != 1 { return invalid_native(); }
    if runtime::with_active_if_present(|selected| !core::ptr::eq(selected.api(), api)).unwrap_or(false) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::unit()), _ => return invalid_native(),
    }
    let runtime = runtime::install(api, imports, exports, vtables, unsafe { (*roots).api }, layouts, hooks, finalizers);
    runtime.install_portable_code_objects();
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::unit())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: core::mem::size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
