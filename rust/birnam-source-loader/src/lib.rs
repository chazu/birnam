//! System's existing source loader, installed by Prelude (81AV).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::{is_error, numeric_error};
use birnam_abi::source_loader::ApiV3;
use birnam_abi::text_view::{printable_string, string_bytes};
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first source loader installation. The host pins it.
static EXECUTION_API: OnceLock<(usize, usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("Source Loader methods were not installed");
    unsafe { &*(pointer.0 as *const IdApi) }
}
unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
}

unsafe fn send_args(api: &IdApi, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    let arguments = if args.is_empty() {
        ptr::null()
    } else {
        args.as_ptr()
    };
    // SAFETY: callers keep the synchronous stack-backed argument slice alive
    // until this send returns; IdApi does not retain the pointer.
    unsafe { (api.send)(receiver, selector, arguments, args.len()) }
}

fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}

unsafe extern "C" fn system_load(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const MAX_SOURCE_BYTES: u64 = 1 << 30;
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(path_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, path_object) }) else {
        return api.nilObj;
    };
    let Some(path) = path_from_bytes(path_bytes) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read source file") };
    };
    let Ok(file) = File::open(path) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read source file") };
    };
    let mut source = Vec::new();
    if file
        .take(MAX_SOURCE_BYTES.saturating_add(1))
        .read_to_end(&mut source)
        .is_err()
        || source.len() > MAX_SOURCE_BYTES as usize
    {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read source file") };
    }
    if unsafe { birnam_abi::namespace_presence::contains(execution_presence(), c"Codegen".as_ptr()) } == 0 {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"noCodegen\0", b"Codegen module is not loaded") };
    }
    let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Codegen".as_ptr()) };
    if codegen.is_null() {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"noCodegen\0", b"Codegen module is not loaded") };
    }
    let source_object = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };

    // Preserve the module-source staging protocol used by the native prelude:
    // inspect a declaration first, record it by declared identity, then run
    // the source-file-aware persisting evaluator.
    let revision = unsafe { send_args(api, codegen, b"sourceRevisionOf:\0", &[source_object]) };
    let revision_is_error = unsafe { is_error(api, execution_presence(), execution_imports(), revision) };
    let mut declared_name = api.nilObj;
    if !revision_is_error && revision != api.nilObj {
        let name = unsafe { send_args(api, revision, b"at:\0", &[(birnam_abi::integer::make)(1)]) };
        declared_name = name;
        let recorded = unsafe {
            send_args(
                api,
                codegen,
                b"recordModuleSource:named:\0",
                &[source_object, name],
            )
        };
        if unsafe { is_error(api, execution_presence(), execution_imports(), recorded) } {
            return recorded;
        }
    }

    let result = unsafe {
        send_args(
            api,
            codegen,
            b"evalPersisting:from:\0",
            &[source_object, path_object],
        )
    };
    if result == api.nilObj {
        let last_error = unsafe { send0(api, codegen, b"lastError\0") };
        if last_error != api.nilObj {
            if let Some(bytes) = unsafe { string_bytes(api, last_error) } {
                eprintln!(
                    "birnam: load {} failed:\n{}",
                    path.display(),
                    String::from_utf8_lossy(bytes)
                );
            }
            return unsafe {
                numeric_error(api, execution_presence(), execution_imports(), b"syntaxError\0", b"source file failed to compile")
            };
        }
        return result;
    }
    if unsafe { is_error(api, execution_presence(), execution_imports(), result) } {
        if let Some(rendered) = unsafe { printable_string(api, result) } {
            eprintln!(
                "birnam: load {}: {}",
                path.display(),
                String::from_utf8_lossy(rendered)
            );
        }
    } else if declared_name != api.nilObj && unsafe { birnam_abi::namespace_presence::contains(execution_presence(), c"Module".as_ptr()) } != 0 {
        let module = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Module".as_ptr()) };
        if !module.is_null() {
            let _ = unsafe { send_args(api, module, b"markLoaded:\0", &[declared_name]) };
        }
    }
    result
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    system_vt: Oop,
    load: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Source Loader installation"
    );
    EXECUTION_API.get_or_init(|| (execution as usize, presence as usize, imports as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, load, system_load);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Source Loader received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_source_loader_provider_v3() -> *const ApiV3 {
    if READY.load(Ordering::Acquire) {
        &API
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
birnam_abi::string_dependency!();
birnam_abi::error_value_dependency!();
birnam_abi::text_view_dependency!();
birnam_abi::integer_dependency!();

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { EXECUTION_API.get().expect("methods not installed").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { EXECUTION_API.get().expect("module not installed").2 as *const birnam_abi::namespace_import::ApiV1 }
