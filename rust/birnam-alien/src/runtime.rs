//! Live Alien owner seam over the frozen IdApi.
//!
//! This first integrated slice owns the `AlienVt`/`Alien` family and the
//! closeable dynamic-library protocol. Signature parsing, generated C, and
//! loader mechanics remain separately testable; wrapper installation and
//! callbacks are deliberately added only after this object/loader boundary is
//! proven.

#![allow(clippy::missing_safety_doc)]

use crate::loader::{LibraryRegistry, LoaderError};
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Mutex, OnceLock};

const RUST_ALIEN_IDENTITY: &[u8] = b"rust-alien-r5-loader\0";
const ALIEN_VT_NAME: &[u8] = b"AlienVt\0";
const ALIEN_NAME: &[u8] = b"Alien\0";

static API: OnceLock<usize> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<LibraryRegistry>> = OnceLock::new();

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    alien_vt: usize,
    alien_obj: usize,
}

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Alien: {message}");
    std::process::abort();
}

unsafe fn api_ref() -> &'static IdApi {
    let Some(address) = API.get() else {
        abort("module_init was not called");
    };
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains that table for the lifetime of the loaded floor.
    unsafe { &*(*address as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all current callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the synchronous method call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<&'static [u8]> {
    if value.is_null() {
        return None;
    }
    let mut length = 0usize;
    // SAFETY: `value` is expected to be a String; the IdApi returns a borrowed
    // byte span valid until the current send returns.
    let pointer = unsafe { (api.stringBytes)(value, &mut length) };
    if length != 0 && pointer.is_null() {
        return None;
    }
    // SAFETY: the host's stringBytes contract supplies the borrowed extent.
    Some(unsafe { core::slice::from_raw_parts(pointer, length) })
}

unsafe fn make_error(api: &IdApi, kind: &[u8], message: &[u8]) -> Oop {
    let factory = unsafe { (api.import)(c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (api.mkString)(message.as_ptr(), message.len()) };
    if kind.is_null() || message.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let arguments = [kind, message];
    // SAFETY: all arguments are process-owned Oops retained for this call.
    unsafe { (api.send)(factory, selector, arguments.as_ptr(), arguments.len()) }
}

fn loader_error(api: &IdApi, error: LoaderError, operation: &[u8]) -> Oop {
    let kind = match error {
        LoaderError::NotOpen => b"notFound\0".as_slice(),
        LoaderError::EmptyPath
        | LoaderError::EmbeddedNul
        | LoaderError::EmptySymbol
        | LoaderError::SymbolEmbeddedNul => b"typeMismatch\0".as_slice(),
        LoaderError::OpenFailed | LoaderError::SymbolNotFound => b"loadFailed\0".as_slice(),
    };
    let mut message = Vec::with_capacity(operation.len() + 32);
    message.extend_from_slice(operation);
    message.extend_from_slice(b": ");
    message.extend_from_slice(error.to_string().as_bytes());
    // `api_ref` is process-owned and this method holds no Rust borrow across send.
    unsafe { make_error(api, kind, &message) }
}

fn registry() -> &'static Mutex<LibraryRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(LibraryRegistry::default()))
}

unsafe extern "C" fn alien_library_dir(
    self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path) = (unsafe { string_bytes(api, value) }) else {
        return api.nilObj;
    };
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|_| abort("Alien library registry was poisoned"));
    if registry.set_library_dir(path).is_err() {
        return api.nilObj;
    }
    self_object
}

unsafe extern "C" fn alien_dlopen(
    _self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path) = (unsafe { string_bytes(api, value) }) else {
        return api.nilObj;
    };
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|_| abort("Alien library registry was poisoned"));
    match registry.open(path) {
        Ok(()) => api.trueObj,
        Err(_) => api.nilObj,
    }
}

unsafe extern "C" fn alien_dlclose(
    _self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path) = (unsafe { string_bytes(api, value) }) else {
        return api.nilObj;
    };
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|_| abort("Alien library registry was poisoned"));
    match registry.close(path) {
        Ok(()) => api.trueObj,
        Err(error) => loader_error(api, error, b"dlclose"),
    }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let address = api_pointer as usize;
    match API.get() {
        Some(existing) if *existing != address => abort("IdApi was replaced"),
        Some(_) => {}
        None => {
            let _ = API.set(address);
        }
    }
    let api = unsafe { &*api_pointer };
    let reloading = unsafe { (api.isExported)(ALIEN_NAME.as_ptr().cast()) } != 0;
    let (alien_vt, alien_obj) = if reloading {
        let alien_vt = unsafe { (api.import)(ALIEN_VT_NAME.as_ptr().cast()) };
        let alien_obj = unsafe { (api.import)(ALIEN_NAME.as_ptr().cast()) };
        (alien_vt, alien_obj)
    } else {
        let alien_vt = unsafe { (api.proto)(api.object_vt) };
        let alien_obj = unsafe { (api.alloc)(alien_vt, 0) };
        (alien_vt, alien_obj)
    };
    if alien_vt.is_null() || alien_obj.is_null() {
        abort("Alien family allocation/import failed");
    }
    let runtime = RUNTIME.get_or_init(|| Runtime {
        api: address,
        alien_vt: alien_vt as usize,
        alien_obj: alien_obj as usize,
    });
    if runtime.api != address {
        abort("Alien runtime was initialized with a different IdApi");
    }
    unsafe {
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"libraryDir:\0"),
            alien_library_dir,
        );
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"dlopen:\0"),
            alien_dlopen,
        );
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"dlclose:\0"),
            alien_dlclose,
        );
    }
    if !reloading {
        unsafe {
            let _ = (api.exportObj)(ALIEN_VT_NAME.as_ptr().cast(), runtime.alien_vt as Oop);
            let _ = (api.exportObj)(ALIEN_NAME.as_ptr().cast(), runtime.alien_obj as Oop);
        }
    }
}

/// Identity of this Rust Alien owner. The value is NUL-terminated for the
/// existing dynamic-rung diagnostic convention.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_alien_rust_identity() -> *const c_char {
    RUST_ALIEN_IDENTITY.as_ptr().cast()
}

/// Initialize the Rust Alien object family and its dynamic-library protocol.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void) -> *mut c_void {
    let _ = state;
    // SAFETY: the host supplies the process-owned IdApi table and retains it
    // for the module lifetime.
    unsafe { bring_up(api) };
    ptr::null_mut()
}
