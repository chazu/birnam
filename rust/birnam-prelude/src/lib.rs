//! Prelude installs System operations after their inputs are available.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop};
use core::ffi::c_char;
use std::sync::OnceLock;

const RUST_PRELUDE_IDENTITY: &[u8] = b"rust-prelude-n7.1-core\0";

/// Identifies the original Rust Prelude rung; identity remains stable.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_prelude_rust_identity() -> *const c_char {
    RUST_PRELUDE_IDENTITY.as_ptr().cast()
}

/// This module copies or synchronously consumes every borrowed Symbol name.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

#[cfg(test)]
mod tests {
    use super::{RUST_PRELUDE_IDENTITY, birnam_prelude_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable process-lifetime storage.
        let identity = unsafe { CStr::from_ptr(birnam_prelude_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_PRELUDE_IDENTITY);
    }
}

// System begins as a rooted object. Completed modules add their own operations.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_SYSTEM: OnceLock<(usize, usize)> = OnceLock::new();
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    use std::sync::atomic::Ordering;
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([api, presence, imports, exports, roots]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(api), Some(presence), Some(imports), Some(exports), Some(roots)) = (api.as_word(), presence.as_word(), imports.as_word(), exports.as_word(), roots.as_word()) else { return invalid(); };
    let api = api as *const IdApi;
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || !unsafe { birnam_abi::namespace_presence::valid(presence) && birnam_abi::namespace_import::valid(imports) && birnam_abi::namespace_export::valid(exports) && birnam_abi::root_registry::valid_owned(roots) } { return invalid(); }
    if NATIVE_SYSTEM.get().is_some_and(|selected| selected.0 != api.addr()) { return invalid(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::object(NATIVE_SYSTEM.get().unwrap().1 as Oop)), _ => return invalid(),
    }
    let api_ref = unsafe { &*api };
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let system = if unsafe { birnam_abi::namespace_presence::contains(presence, c"System".as_ptr()) } != 0 {
        unsafe { birnam_abi::namespace_import::import(imports, c"System".as_ptr()) }
    } else {
        let vt = unsafe { retained.keep((api_ref.proto)(api_ref.object_vt)) };
        let system = unsafe { retained.keep((api_ref.alloc)(vt, 0)) };
        unsafe {
            birnam_abi::set_vtable_owner(api_ref, vt, system);
            birnam_abi::namespace_export::export(exports, c"SystemVt".as_ptr(), vt);
            birnam_abi::namespace_export::export(exports, c"System".as_ptr(), system);
        }
        system
    };
    assert!(NATIVE_SYSTEM.set((api.addr(), system.addr())).is_ok());
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::object(system))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
