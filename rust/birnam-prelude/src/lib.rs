//! Prelude installs System operations after their inputs are available.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

const RUST_PRELUDE_IDENTITY: &[u8] = b"rust-prelude-n7.1-core\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    finalizers: usize,
    settings: usize,
    layouts: usize,
    hooks: usize,
    vtables: usize,
    observation: usize,
    roots: usize,
    presence: usize,
    imports: usize,
    exports: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Prelude: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn install_system_protocol(api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1) {
    let (system_vt, system_object) = if unsafe { birnam_abi::namespace_presence::contains(presence, c"System".as_ptr()) } != 0 {
        // SAFETY: an exported System family is process-owned and remains live
        // across reloads; extend its vtable instead of minting a duplicate.
        unsafe {
            (
                birnam_abi::namespace_import::import(imports, c"SystemVt".as_ptr()),
                birnam_abi::namespace_import::import(imports, c"System".as_ptr()),
            )
        }
    } else {
        let system_vt = unsafe { (api.proto)(api.object_vt) };
        let system_object = unsafe { (api.alloc)(system_vt, 0) };
        unsafe { birnam_abi::set_vtable_owner(api, system_vt, system_object) };
        unsafe { birnam_abi::namespace_export::export(exports, c"SystemVt".as_ptr(), system_vt) };
        unsafe { birnam_abi::namespace_export::export(exports, c"System".as_ptr(), system_object) };
        (system_vt, system_object)
    };
    let print = unsafe { intern(api, b"print:\0") };
    let println = unsafe { intern(api, b"println:\0") };
    let read_file = unsafe { intern(api, b"readFile:\0") };
    let write_file = unsafe { intern(api, b"writeFile:contents:\0") };
    let get_env = unsafe { intern(api, b"getEnv:\0") };
    let load = unsafe { intern(api, b"load:\0") };
    let load_module = unsafe { intern(api, b"loadModule:\0") };
    let read_line = unsafe { intern(api, b"readLine\0") };
    let gc = unsafe { intern(api, b"gc\0") };
    let live_objects = unsafe { intern(api, b"liveObjects\0") };
    let auto_gc = unsafe { intern(api, b"autoGc:\0") };
    let precise_gc = unsafe { intern(api, b"preciseGc:\0") };
    // SAFETY: the System vtable and callback addresses are process-lifetime
    // values. The singleton is retained by the exported family.
    unsafe {
        birnam_abi::system_output::install(
            api, runtime().api as *const IdApi, system_vt, print, println,
        );
        birnam_abi::file_io::install(
            api, runtime().api as *const IdApi, runtime().presence as *const birnam_abi::namespace_presence::ApiV1, runtime().imports as *const birnam_abi::namespace_import::ApiV1, system_vt, read_file, write_file,
        );
        birnam_abi::environment::install(
            api, runtime().api as *const IdApi, runtime().presence as *const birnam_abi::namespace_presence::ApiV1, runtime().imports as *const birnam_abi::namespace_import::ApiV1, system_vt, get_env,
        );
        birnam_abi::source_loader::install(
            api, runtime().api as *const IdApi, runtime().presence as *const birnam_abi::namespace_presence::ApiV1, runtime().imports as *const birnam_abi::namespace_import::ApiV1, system_vt, load,
        );
        birnam_abi::native_loader::install(
            api, runtime().api as *const IdApi, runtime().presence as *const birnam_abi::namespace_presence::ApiV1, runtime().imports as *const birnam_abi::namespace_import::ApiV1, runtime().exports as *const birnam_abi::namespace_export::ApiV1, runtime().vtables as *const birnam_abi::vtable_enumeration::ApiV2, runtime().observation as *const birnam_abi::method_observation::ApiV1, runtime().roots as *const birnam_abi::root_registry::ApiV1, runtime().settings as *const birnam_abi::gc_settings::ApiV1, runtime().layouts as *const birnam_abi::payload_layout::ApiV1, runtime().hooks as *const birnam_abi::trace_hooks::ApiV1, runtime().finalizers as *const birnam_abi::finalizers::ApiV1, system_vt, load_module,
        );
        birnam_abi::system_input::install(
            api, runtime().api as *const IdApi, runtime().presence as *const birnam_abi::namespace_presence::ApiV1, runtime().imports as *const birnam_abi::namespace_import::ApiV1, system_vt, read_line,
        );
        birnam_abi::system_gc::install(
            api, runtime().api as *const IdApi, runtime().settings as *const birnam_abi::gc_settings::ApiV1, system_vt,
            gc, live_objects, auto_gc, precise_gc,
        );
    }
    let _ = system_object;
}

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1, hooks: *const birnam_abi::trace_hooks::ApiV1, finalizers: *const birnam_abi::finalizers::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = RUNTIME.get_or_init(|| Runtime {
        api: api_pointer as usize, finalizers: finalizers as usize,
        settings: settings as usize,
        layouts: layouts as usize,
            hooks: hooks as usize,
            vtables: vtables as usize,
            observation: observation as usize,
            roots: roots as usize,
        presence: presence as usize,
        imports: imports as usize,
        exports: exports as usize,
    });
    unsafe { install_system_protocol(api, presence, imports, exports) };
}

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

/// Initialize the Rust prelude against the selected floor's IdApi.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1, hooks: *const birnam_abi::trace_hooks::ApiV1, finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    // SAFETY: bring_up enforces the null boundary and only retains the
    // process-owned IdApi address.
    unsafe { bring_up(api, presence, imports, exports, vtables, observation, roots, settings, layouts, hooks, finalizers) };
    ptr::null_mut()
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

birnam_abi::system_gc_dependency!();

birnam_abi::system_output_dependency!();

birnam_abi::system_input_dependency!();

birnam_abi::environment_dependency!();

birnam_abi::file_io_dependency!();

birnam_abi::native_loader_dependency!();

birnam_abi::source_loader_dependency!();
