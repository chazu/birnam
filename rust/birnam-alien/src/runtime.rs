//! Live Alien owner seam over the frozen IdApi.
//!
//! This first integrated slice owns the `AlienVt`/`Alien` family and the
//! closeable dynamic-library protocol. Signature parsing, generated C, and
//! loader mechanics remain separately testable; wrapper installation and
//! callbacks are deliberately added only after this object/loader boundary is
//! proven.

#![allow(clippy::missing_safety_doc)]

use crate::codegen::{emit_callback, emit_shim};
use crate::loader::{LibraryRegistry, LoaderError};
use crate::parse_signature;
use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::{Mutex, Once, OnceLock};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(pointer: *mut c_void);
}

const RUST_ALIEN_IDENTITY: &[u8] = b"rust-alien-r5-loader\0";
const ALIEN_VT_NAME: &[u8] = b"AlienVt\0";
const ALIEN_NAME: &[u8] = b"Alien\0";

static API: OnceLock<usize> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<LibraryRegistry>> = OnceLock::new();
static CALLBACKS: OnceLock<Mutex<Vec<CallbackEntry>>> = OnceLock::new();
static CALLBACK_ROOTS: Once = Once::new();

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    alien_vt: usize,
    alien_obj: usize,
}

/// A live callback keeps both the block it dispatches to and the TCC unit that
/// owns its executable code alive.  Raw Oops are stored as integer addresses so
/// this registry can be protected by a process-wide mutex without inventing a
/// Rust ownership authority for heap objects.
#[derive(Clone, Copy)]
struct CallbackEntry {
    block: usize,
    unit: usize,
    address: usize,
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

unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<Vec<u8>> {
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
    Some(unsafe { core::slice::from_raw_parts(pointer, length) }.to_vec())
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

fn callbacks() -> &'static Mutex<Vec<CallbackEntry>> {
    CALLBACKS.get_or_init(|| Mutex::new(Vec::new()))
}

unsafe extern "C" fn callback_roots(mark: MarkFn) {
    let roots = callbacks()
        .lock()
        .unwrap_or_else(|_| abort("Alien callback registry was poisoned"))
        .iter()
        .flat_map(|entry| [entry.block, entry.unit])
        .collect::<Vec<_>>();
    // Do not hold the registry lock across the host mark callback.  The mark
    // function currently only queues addresses, but this keeps the boundary
    // valid if the collector grows re-entrant bookkeeping later.
    for root in roots {
        // SAFETY: every entry was copied from a live Oop supplied by the host;
        // the root provider is called synchronously by that same collector.
        unsafe { mark(root as Oop) };
    }
}

unsafe fn ensure_callback_roots(api: &IdApi) {
    CALLBACK_ROOTS.call_once(|| {
        // SAFETY: the host IdApi retains this process-owned root provider for
        // the lifetime of the floor. Registration occurs once per process.
        unsafe { (api.gcRegisterRootProvider)(callback_roots) };
    });
}

unsafe fn address_value(api: &IdApi, value: Oop) -> Option<usize> {
    if unsafe { (api.isWord)(value) } != 0 {
        return Some(unsafe { (api.wordValue)(value) } as usize);
    }
    if unsafe { (api.isSmallInt)(value) } != 0 {
        return Some(unsafe { (api.smallIntValue)(value) } as usize);
    }
    None
}

unsafe fn resolve_buffer_address(api: &IdApi, receiver: Oop, offset: Oop) -> Result<usize, Oop> {
    let base = unsafe { (api.wordValue)(receiver) } as usize;
    if base == 0 {
        return Err(unsafe {
            make_error(
                api,
                b"nullPointer\0",
                b"cannot peek/poke through a null pointer",
            )
        });
    }
    if unsafe { (api.isSmallInt)(offset) } == 0 {
        return Err(unsafe {
            make_error(api, b"typeMismatch\0", b"the offset must be an Integer")
        });
    }
    let signed = unsafe { (api.smallIntValue)(offset) } as u64;
    Ok(base.wrapping_add(signed as usize))
}

unsafe extern "C" fn alien_alloc(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.isSmallInt)(value) } == 0 {
        return unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"alloc: needs an Integer byte count",
            )
        };
    }
    let size = unsafe { (api.smallIntValue)(value) };
    if size <= 0 {
        return unsafe { make_error(api, b"badArgument\0", b"alloc: needs a positive byte count") };
    }
    let pointer = unsafe { malloc(size as usize) };
    if pointer.is_null() {
        return unsafe { make_error(api, b"outOfMemory\0", b"alloc: malloc failed") };
    }
    // SAFETY: malloc returned `size` writable bytes and the slice does not
    // escape; this is the frozen zero-initialized buffer contract.
    unsafe { ptr::write_bytes(pointer.cast::<u8>(), 0, size as usize) };
    unsafe { (api.mkWord)(pointer as usize as u64) }
}

unsafe extern "C" fn alien_free(receiver: Oop, _args: *const Oop, _count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let address = unsafe { (api.wordValue)(receiver) };
    if address != 0 {
        // SAFETY: the caller owns the raw address and promises it came from a
        // compatible allocator, matching the existing unsafe Word protocol.
        unsafe { free(address as *mut c_void) };
    }
    api.nilObj
}

unsafe extern "C" fn alien_peek_int(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(offset) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, offset) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    // SAFETY: raw buffer operations intentionally follow the caller's C-memory
    // contract; unaligned reads match the Zig `align(1)` implementation.
    let value = unsafe { ptr::read_unaligned(address as *const i64) };
    unsafe { (api.mkSmallInt)(value) }
}

unsafe extern "C" fn alien_peek_byte(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(offset) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, offset) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { ptr::read(address as *const u8) };
    unsafe { (api.mkSmallInt)(i64::from(value)) }
}

unsafe extern "C" fn alien_peek_pointer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(offset) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, offset) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { ptr::read_unaligned(address as *const usize) };
    unsafe { (api.mkWord)(value as u64) }
}

unsafe extern "C" fn alien_peek_double(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(offset) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, offset) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { ptr::read_unaligned(address as *const f64) };
    unsafe { (api.mkFloat)(value) }
}

unsafe extern "C" fn alien_peek_string(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(offset) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, offset) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    // SAFETY: this operation deliberately adopts the C NUL-terminated string
    // contract; the result is copied into a managed Birnam String immediately.
    let bytes = unsafe { std::ffi::CStr::from_ptr(address as *const c_char) }.to_bytes();
    unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn alien_poke_int(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 || unsafe { (api.isSmallInt)(arguments[0]) } == 0 {
        return if arguments.len() == 2 {
            unsafe { make_error(api, b"typeMismatch\0", b"pokeInt: needs an Integer value") }
        } else {
            api.nilObj
        };
    }
    let address = match unsafe { resolve_buffer_address(api, receiver, arguments[1]) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { (api.smallIntValue)(arguments[0]) };
    unsafe { ptr::write_unaligned(address as *mut i64, value) };
    receiver
}

unsafe extern "C" fn alien_poke_byte(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 || unsafe { (api.isSmallInt)(arguments[0]) } == 0 {
        return if arguments.len() == 2 {
            unsafe { make_error(api, b"typeMismatch\0", b"pokeByte: needs an Integer value") }
        } else {
            api.nilObj
        };
    }
    let address = match unsafe { resolve_buffer_address(api, receiver, arguments[1]) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { (api.smallIntValue)(arguments[0]) } as u8;
    unsafe { ptr::write(address as *mut u8, value) };
    receiver
}

unsafe extern "C" fn alien_poke_pointer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { address_value(api, arguments[0]) }) else {
        return unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"pokePtr: needs a Word or Integer address",
            )
        };
    };
    let address = match unsafe { resolve_buffer_address(api, receiver, arguments[1]) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    unsafe { ptr::write_unaligned(address as *mut usize, value) };
    receiver
}

unsafe extern "C" fn alien_poke_double(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 || unsafe { (api.isFloat)(arguments[0]) } == 0 {
        return if arguments.len() == 2 {
            unsafe { make_error(api, b"typeMismatch\0", b"pokeDouble: needs a Float value") }
        } else {
            api.nilObj
        };
    }
    let address = match unsafe { resolve_buffer_address(api, receiver, arguments[1]) } {
        Ok(address) => address,
        Err(error) => return error,
    };
    let value = unsafe { (api.floatValue)(arguments[0]) };
    unsafe { ptr::write_unaligned(address as *mut f64, value) };
    receiver
}

unsafe extern "C" fn alien_library_dir(self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    if registry.set_library_dir(&path).is_err() {
        return api.nilObj;
    }
    self_object
}

unsafe extern "C" fn alien_dlopen(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    match registry.open(&path) {
        Ok(()) => api.trueObj,
        Err(_) => api.nilObj,
    }
}

unsafe extern "C" fn alien_dlclose(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
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
    let result = registry.close(&path);
    drop(registry);
    match result {
        Ok(()) => api.trueObj,
        Err(error) => loader_error(api, error, b"dlclose"),
    }
}

unsafe extern "C" fn alien_wrap(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let (function_object, signature_object, selector, target) = {
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if arguments.len() != 4 {
            return api.nilObj;
        }
        (arguments[0], arguments[1], arguments[2], arguments[3])
    };
    let Some(function_name) = (unsafe { string_bytes(api, function_object) }) else {
        return api.nilObj;
    };
    let Some(signature_bytes) = (unsafe { string_bytes(api, signature_object) }) else {
        return api.nilObj;
    };
    let signature = match parse_signature(&signature_bytes) {
        Ok(signature) => signature,
        Err(error) => unsafe {
            return make_error(api, b"badSignature\0", error.to_string().as_bytes());
        },
    };
    if !signature.aggregates.is_empty() && unsafe { (api.import)(c"Vector".as_ptr()) }.is_null() {
        return unsafe {
            make_error(
                api,
                b"moduleNotLoaded\0",
                b"a struct signature needs the collection module",
            )
        };
    }
    let address_result = {
        let registry = registry()
            .lock()
            .unwrap_or_else(|_| abort("Alien library registry was poisoned"));
        registry.symbol_default(&function_name)
    };
    let address = match address_result {
        Ok(address) => address.as_ptr() as usize,
        Err(error) => return loader_error(api, error, b"wrap"),
    };
    let source = match emit_shim(&function_name, &signature) {
        Ok(source) => source,
        Err(error) => unsafe {
            return make_error(api, b"badSignature\0", error.to_string().as_bytes());
        },
    };
    let tcc = unsafe { (api.import)(c"Tcc".as_ptr()) };
    if tcc.is_null() {
        return unsafe { make_error(api, b"moduleNotLoaded\0", b"Tcc is not loaded") };
    }
    let new_selector = unsafe { intern(api, b"new\0") };
    // SAFETY: all sends below use process-owned selectors and live Oops; no Rust
    // borrow survives a send or crosses into generated C.
    let unit = unsafe { (api.send)(tcc, new_selector, ptr::null(), 0) };
    if unit.is_null() || unit == api.nilObj {
        return unsafe { make_error(api, b"compileFailed\0", b"could not create a compile unit") };
    }
    let compile_selector = unsafe { intern(api, b"compile:\0") };
    let source_object = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    if source_object.is_null()
        || unsafe { (api.send)(unit, compile_selector, &source_object, 1) } == api.nilObj
    {
        return unsafe {
            make_error(
                api,
                b"compileFailed\0",
                b"the generated shim C did not compile",
            )
        };
    }
    let bind_selector = unsafe { intern(api, b"bindRuntime\0") };
    if unsafe { (api.send)(unit, bind_selector, ptr::null(), 0) } == api.nilObj {
        return unsafe { make_error(api, b"compileFailed\0", b"TCC runtime binding failed") };
    }
    let add_symbol_selector = unsafe { intern(api, b"addSymbol:value:\0") };
    let function_name_object =
        unsafe { (api.mkString)(function_name.as_ptr(), function_name.len()) };
    let address_object = unsafe { (api.mkSmallInt)(address as i64) };
    let symbol_arguments = [function_name_object, address_object];
    if unsafe {
        (api.send)(
            unit,
            add_symbol_selector,
            symbol_arguments.as_ptr(),
            symbol_arguments.len(),
        )
    } == api.nilObj
    {
        return unsafe {
            make_error(
                api,
                b"compileFailed\0",
                b"the wrapped symbol could not be bound",
            )
        };
    }
    if !signature.aggregates.is_empty() {
        let vector = unsafe { (api.import)(c"Vector".as_ptr()) };
        let vector_name = unsafe { (api.mkString)(c"__vecFactory".as_ptr().cast(), 12) };
        let vector_address = unsafe { (api.mkSmallInt)(vector as usize as i64) };
        let vector_arguments = [vector_name, vector_address];
        if unsafe {
            (api.send)(
                unit,
                add_symbol_selector,
                vector_arguments.as_ptr(),
                vector_arguments.len(),
            )
        } == api.nilObj
        {
            return unsafe {
                make_error(api, b"compileFailed\0", b"Vector factory binding failed")
            };
        }
    }
    let relocate_selector = unsafe { intern(api, b"relocate\0") };
    if unsafe { (api.send)(unit, relocate_selector, ptr::null(), 0) } == api.nilObj {
        return unsafe { make_error(api, b"relocateFailed\0", b"could not relocate the shim") };
    }
    let install_selector = unsafe { intern(api, b"install:as:on:\0") };
    let shim_name = unsafe { (api.mkString)(c"__shim".as_ptr().cast(), 6) };
    let install_arguments = [shim_name, selector, target];
    let installed = unsafe {
        (api.send)(
            unit,
            install_selector,
            install_arguments.as_ptr(),
            install_arguments.len(),
        )
    };
    if installed == api.nilObj {
        return unsafe {
            make_error(
                api,
                b"compileFailed\0",
                b"the shim method could not be installed",
            )
        };
    }
    installed
}

unsafe extern "C" fn alien_callback(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let (block, signature_object) = {
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if arguments.len() != 2 {
            return api.nilObj;
        }
        (arguments[0], arguments[1])
    };
    if block.is_null() {
        return unsafe { make_error(api, b"typeMismatch\0", b"callback needs a live block") };
    }
    let Some(signature_bytes) = (unsafe { string_bytes(api, signature_object) }) else {
        return unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"callback signature must be a String",
            )
        };
    };
    let signature = match parse_signature(&signature_bytes) {
        Ok(signature) => signature,
        Err(error) => unsafe {
            return make_error(api, b"badSignature\0", error.to_string().as_bytes());
        },
    };
    if matches!(
        signature.return_type,
        crate::Type::Scalar(crate::Scalar::StringPointer)
    ) {
        return unsafe { make_error(api, b"badSignature\0", b"a callback cannot return a string") };
    }
    if !signature.aggregates.is_empty() && unsafe { (api.import)(c"Vector".as_ptr()) }.is_null() {
        return unsafe {
            make_error(
                api,
                b"moduleNotLoaded\0",
                b"a struct signature needs the collection module",
            )
        };
    }
    let source = match emit_callback(&signature) {
        Ok(source) => source,
        Err(error) => unsafe {
            return make_error(api, b"badSignature\0", error.to_string().as_bytes());
        },
    };
    let tcc = unsafe { (api.import)(c"Tcc".as_ptr()) };
    if tcc.is_null() {
        return unsafe { make_error(api, b"moduleNotLoaded\0", b"Tcc is not loaded") };
    }
    // SAFETY: this is the same synchronous TCC send sequence used by wrappers;
    // no Rust borrow survives any send.
    let unit = unsafe { (api.send)(tcc, intern(api, b"new\0"), ptr::null(), 0) };
    if unit.is_null() || unit == api.nilObj {
        return unsafe { make_error(api, b"compileFailed\0", b"could not create a callback unit") };
    }
    let source_object = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    if source_object.is_null()
        || unsafe { (api.send)(unit, intern(api, b"compile:\0"), &source_object, 1) } == api.nilObj
    {
        return unsafe {
            make_error(
                api,
                b"compileFailed\0",
                b"the generated callback C did not compile",
            )
        };
    }
    if unsafe { (api.send)(unit, intern(api, b"bindRuntime\0"), ptr::null(), 0) } == api.nilObj {
        return unsafe { make_error(api, b"compileFailed\0", b"TCC runtime binding failed") };
    }
    let add_symbol_selector = unsafe { intern(api, b"addSymbol:value:\0") };
    let block_name = unsafe { (api.mkString)(c"__cbBlock".as_ptr().cast(), 9) };
    let block_address = unsafe { (api.mkSmallInt)(block as usize as i64) };
    let block_arguments = [block_name, block_address];
    if unsafe {
        (api.send)(
            unit,
            add_symbol_selector,
            block_arguments.as_ptr(),
            block_arguments.len(),
        )
    } == api.nilObj
    {
        return unsafe { make_error(api, b"compileFailed\0", b"callback block binding failed") };
    }
    if !signature.aggregates.is_empty() {
        let vector = unsafe { (api.import)(c"Vector".as_ptr()) };
        let vector_name = unsafe { (api.mkString)(c"__vecFactory".as_ptr().cast(), 12) };
        let vector_address = unsafe { (api.mkSmallInt)(vector as usize as i64) };
        let vector_arguments = [vector_name, vector_address];
        if unsafe {
            (api.send)(
                unit,
                add_symbol_selector,
                vector_arguments.as_ptr(),
                vector_arguments.len(),
            )
        } == api.nilObj
        {
            return unsafe {
                make_error(api, b"compileFailed\0", b"Vector factory binding failed")
            };
        }
    }
    if unsafe { (api.send)(unit, intern(api, b"relocate\0"), ptr::null(), 0) } == api.nilObj {
        return unsafe { make_error(api, b"relocateFailed\0", b"could not relocate the callback") };
    }
    let symbol_name = unsafe { (api.mkString)(c"__cb".as_ptr().cast(), 4) };
    let callback_address = unsafe { (api.send)(unit, intern(api, b"symbol:\0"), &symbol_name, 1) };
    let Some(address) = (unsafe { address_value(api, callback_address) }) else {
        return unsafe {
            make_error(
                api,
                b"symbolNotFound\0",
                b"the callback entry symbol was not found",
            )
        };
    };
    callbacks()
        .lock()
        .unwrap_or_else(|_| abort("Alien callback registry was poisoned"))
        .push(CallbackEntry {
            block: block as usize,
            unit: unit as usize,
            address,
        });
    callback_address
}

unsafe extern "C" fn alien_release_callback(
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
    let Some(address) = (unsafe { address_value(api, value) }) else {
        return unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"releaseCallback: needs a callback address",
            )
        };
    };
    let entry = {
        let mut live = callbacks()
            .lock()
            .unwrap_or_else(|_| abort("Alien callback registry was poisoned"));
        let Some(index) = live.iter().position(|entry| entry.address == address) else {
            return unsafe {
                make_error(
                    api,
                    b"notFound\0",
                    b"releaseCallback: no live callback at that address",
                )
            };
        };
        live.swap_remove(index)
    };
    // The caller's explicit release is an unsafe contract: it promises that C
    // will never invoke the address again. Marking the unit disposable lets the
    // host reclaim its executable pages once the unit is otherwise unreachable.
    let _ = unsafe {
        (api.send)(
            entry.unit as Oop,
            intern(api, b"disposable\0"),
            ptr::null(),
            0,
        )
    };
    api.trueObj
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
    unsafe { ensure_callback_roots(api) };
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
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"wrap:signature:as:on:\0"),
            alien_wrap,
        );
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"callback:signature:\0"),
            alien_callback,
        );
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"releaseCallback:\0"),
            alien_release_callback,
        );
        (api.method)(
            runtime.alien_vt as Oop,
            intern(api, b"alloc:\0"),
            alien_alloc,
        );
        let word_vt = api.word_vt;
        (api.method)(word_vt, intern(api, b"free\0"), alien_free);
        (api.method)(word_vt, intern(api, b"peekIntAt:\0"), alien_peek_int);
        (api.method)(word_vt, intern(api, b"peekByteAt:\0"), alien_peek_byte);
        (api.method)(word_vt, intern(api, b"peekPtrAt:\0"), alien_peek_pointer);
        (api.method)(word_vt, intern(api, b"peekDoubleAt:\0"), alien_peek_double);
        (api.method)(word_vt, intern(api, b"peekStringAt:\0"), alien_peek_string);
        (api.method)(word_vt, intern(api, b"pokeInt:at:\0"), alien_poke_int);
        (api.method)(word_vt, intern(api, b"pokeByte:at:\0"), alien_poke_byte);
        (api.method)(word_vt, intern(api, b"pokePtr:at:\0"), alien_poke_pointer);
        (api.method)(word_vt, intern(api, b"pokeDouble:at:\0"), alien_poke_double);
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
