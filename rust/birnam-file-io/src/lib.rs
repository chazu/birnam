//! System's existing file read/write methods, constructed through Bootstrap (81AS).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::numeric_error;
use birnam_abi::file_io::ApiV3;
use birnam_abi::text_view::string_bytes;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Construction supplies the original selection, even when registration reenters
// with another table before the first file IO installation. The host pins it.
static EXECUTION_API: OnceLock<(usize, usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("File IO methods were not installed");
    unsafe { &*(pointer.0 as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}

unsafe extern "C" fn system_read_file(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const MAX_FILE_BYTES: u64 = 1 << 30;
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
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read file") };
    };
    let Ok(file) = File::open(path) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read file") };
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > MAX_FILE_BYTES as usize
    {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not read file") };
    }
    unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn system_write_file(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(values) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        return api.nilObj;
    };
    let Some(contents) = (unsafe { string_bytes(api, values[1]) }) else {
        return api.nilObj;
    };
    let Some(path) = path_from_bytes(path_bytes) else {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not write file") };
    };
    if fs::write(path, contents).is_err() {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"ioError\0", b"could not write file") };
    }
    self_object
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    system_vt: Oop,
    read_file: Oop,
    write_file: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid File IO installation"
    );
    EXECUTION_API.get_or_init(|| (execution as usize, presence as usize, imports as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, read_file, system_read_file);
        (api.method)(system_vt, write_file, system_write_file);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "File IO received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_file_io_provider_v3() -> *const ApiV3 {
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
birnam_abi::text_view_dependency!();
birnam_abi::string_dependency!();
birnam_abi::error_value_dependency!();

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { EXECUTION_API.get().expect("methods not installed").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { EXECUTION_API.get().expect("module not installed").2 as *const birnam_abi::namespace_import::ApiV1 }

// The native constructor returns only after the System methods are installed.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static NATIVE_SELECTION: OnceLock<(usize, usize)> = OnceLock::new();
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, roots, system, presence, imports, string, errors, text]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let Some(api) = api.as_word() else { return invalid_native(); };
    let Some(roots) = roots.as_word() else { return invalid_native(); };
    let Some(presence) = presence.as_word() else { return invalid_native(); };
    let Some(imports) = imports.as_word() else { return invalid_native(); };
    let Some(string) = string.as_word() else { return invalid_native(); };
    let Some(errors) = errors.as_word() else { return invalid_native(); };
    let Some(text) = text.as_word() else { return invalid_native(); };
    let api = api as *const IdApi;
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let Some(system) = system.as_object().filter(|p| !p.is_null()) else { return invalid_native(); };
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver || !birnam_abi::root_registry::valid_owned(roots) } { return invalid_native(); }
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    if !unsafe { birnam_abi::namespace_presence::valid(presence) } { return invalid_native(); }
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    if !unsafe { birnam_abi::namespace_import::valid(imports) } { return invalid_native(); }
    if unsafe { birnam_abi::string::attach(string as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::error_value::attach(errors as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::text_view::attach(text as _) } != 1 { return invalid_native(); }
    if NATIVE_SELECTION.get().is_some_and(|selected| *selected != (api.addr(), system.addr())) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::word((&API as *const _ as *const c_void).addr())), _ => return invalid_native(),
    }
    assert!(NATIVE_SELECTION.set((api.addr(), system.addr())).is_ok());
    READY.store(true, Ordering::Release);
    let mut retained = unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots) };
    let system_vt = unsafe { ((*api).vtableOf)(system) };
    let s0 = unsafe { retained.keep(((*api).intern)(c"readFile:".as_ptr())) };
    let s1 = unsafe { retained.keep(((*api).intern)(c"writeFile:contents:".as_ptr())) };
    unsafe { install(api, api, presence, imports, system_vt, s0, s1) };
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const _ as *const c_void).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
