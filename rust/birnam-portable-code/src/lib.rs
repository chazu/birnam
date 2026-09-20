//! Reified PortableProgram ordinary-object rung.
//!
//! This module is loaded after Collection and Error. It owns the private
//! native payloads behind CodeObject, CodeProgram, CodeLexicalBinding,
//! CodeStatement, and CodeNode while exposing their behavior only through
//! ordinary sends and the frozen IdApi.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::IdApi;
use core::ffi::{c_char, c_void};
use core::ptr;

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

fn abort(message: &str) -> ! {
    eprintln!("birnam Portable Code: {message}");
    std::process::abort();
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_portable_code_rust_identity() -> *const c_char {
    IDENTITY.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1) -> *mut c_void {
    let _ = state;
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    for dependency in [c"Vector", c"VectorVt", c"Error", c"ErrorVt"] {
        if unsafe { birnam_abi::namespace_presence::contains(presence, dependency.as_ptr()) } == 0 {
            abort("Collection and Error must load before Portable Code");
        }
    }
    let runtime = runtime::install(api, imports, exports, vtables, roots);
    runtime.install_portable_code_objects();
    ptr::null_mut()
}

birnam_abi::integer_dependency!();

birnam_abi::string_dependency!();

birnam_abi::namespace_names_dependency!();
