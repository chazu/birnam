//! Live Alien owner seam over the frozen IdApi.
//!
//! The integrated Rust owner keeps the `AlienVt`/`Alien` family, dynamic-library
//! protocol, generated wrappers, callbacks, and raw-buffer boundary over the
//! frozen IdApi. TCC remains the compiler/relocation authority and the host
//! remains the object, collection, and GC authority.

#![allow(clippy::missing_safety_doc)]

use crate::codegen::{emit_callback, emit_shim};
use crate::loader::{LibraryRegistry, LoaderError};
use crate::parse_signature;
use birnam_abi::{
    IdApi, MarkFn, Oop, borrowed_oop_args,
    foreign_address::{self, KIND_ALIEN_MALLOC, KIND_BORROWED},
};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once, OnceLock};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(pointer: *mut c_void);
}

const RUST_ALIEN_IDENTITY: &[u8] = b"rust-alien-r5-loader\0";
const ALIEN_VT_NAME: &[u8] = b"AlienVt\0";
const ALIEN_NAME: &[u8] = b"Alien\0";

static API: OnceLock<(usize, usize, usize, usize, usize, usize, usize)> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<LibraryRegistry>> = OnceLock::new();
static CALLBACKS: OnceLock<Mutex<Vec<CallbackEntry>>> = OnceLock::new();
static CALLBACK_ROOTS: Once = Once::new();
static LIVE_WRAPPERS: AtomicUsize = AtomicUsize::new(0);

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

/// Test-only proof that an Alien panic cannot unwind across its C ABI.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_alien_test_panic_across_ffi() {
    panic!("intentional Rust Alien FFI panic");
}

/// Test-only proof of Alien's fatal invariant boundary.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_alien_test_invariant_abort() {
    abort("intentional Rust Alien FFI invariant failure");
}

unsafe fn api_ref() -> &'static IdApi {
    let Some(address) = API.get() else {
        abort("module_init was not called");
    };
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains that table for the lifetime of the loaded floor.
    unsafe { &*(address.0 as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all current callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the synchronous method call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn string_bytes(_api: &IdApi, value: Oop) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let mut length = 0usize;
    // SAFETY: `value` is expected to be a String; the IdApi returns a borrowed
    // byte span valid until the current send returns.
    let pointer = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    if length != 0 && pointer.is_null() {
        return None;
    }
    // SAFETY: the host's stringBytes contract supplies the borrowed extent.
    Some(unsafe { core::slice::from_raw_parts(pointer, length) }.to_vec())
}

unsafe fn symbol_name(api: &IdApi, value: Oop) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let pointer = unsafe { (api.symbolName)(value) };
    if pointer.is_null() {
        return None;
    }
    Some(
        unsafe { std::ffi::CStr::from_ptr(pointer) }
            .to_bytes()
            .to_vec(),
    )
}

unsafe fn store_entity(api: &IdApi, name: &[u8]) -> Option<(Oop, Oop)> {
    let store = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Store".as_ptr()) };
    if store.is_null() || store == api.nilObj {
        return None;
    }
    Some((store, unsafe { intern(api, name) }))
}

unsafe fn find_alien_source(api: &IdApi, store: Oop, entity: Oop, source: &[u8]) -> bool {
    let count = unsafe { (api.send)(store, intern(api, b"attrCount:\0"), [entity].as_ptr(), 1) };
    if unsafe { (birnam_abi::integer::is_integer)(count) } == 0 {
        return false;
    }
    let count = unsafe { (birnam_abi::integer::value)(count) };
    for index in 0..count {
        let attr = unsafe {
            (api.send)(
                store,
                intern(api, b"attrAt:index:\0"),
                [entity, (birnam_abi::integer::make)(index)].as_ptr(),
                2,
            )
        };
        let value = unsafe {
            (api.send)(
                store,
                intern(api, b"currentValue:attr:\0"),
                [entity, attr].as_ptr(),
                2,
            )
        };
        if unsafe { string_bytes(api, value) }.as_deref() == Some(source) {
            return true;
        }
    }
    false
}

unsafe fn record_alien_source(api: &IdApi, source: &[u8]) {
    let Some((store, entity)) = (unsafe { store_entity(api, b"__aliens__\0") }) else {
        return;
    };
    if unsafe { find_alien_source(api, store, entity, source) } {
        return;
    }
    let count = unsafe { (api.send)(store, intern(api, b"attrCount:\0"), [entity].as_ptr(), 1) };
    if unsafe { (birnam_abi::integer::is_integer)(count) } == 0 {
        return;
    }
    let count = unsafe { (birnam_abi::integer::value)(count) };
    let mut next = 0i64;
    for index in 0..count {
        let attr = unsafe {
            (api.send)(
                store,
                intern(api, b"attrAt:index:\0"),
                [entity, (birnam_abi::integer::make)(index)].as_ptr(),
                2,
            )
        };
        let Some(name) = (unsafe { symbol_name(api, attr) }) else {
            continue;
        };
        let Some(suffix) = name.strip_prefix(b"__alien_") else {
            continue;
        };
        if let Some(index) = core::str::from_utf8(suffix)
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
        {
            next = next.max(index + 1);
        }
    }
    let key = format!("__alien_{next}");
    let key = unsafe { intern(api, [key.as_bytes(), b"\0"].concat().as_slice()) };
    let source_object = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
    unsafe {
        (api.send)(
            store,
            intern(api, b"assert:attr:value:\0"),
            [entity, key, source_object].as_ptr(),
            3,
        );
    }
}

unsafe fn global_name(api: &IdApi, target: Oop) -> Option<Vec<u8>> {
    let exported = unsafe { (birnam_abi::namespace_names::name_of)(target) };
    if !exported.is_null() {
        return Some(
            unsafe { std::ffi::CStr::from_ptr(exported) }
                .to_bytes()
                .to_vec(),
        );
    }
    let (store, globals) = unsafe { store_entity(api, b"__globals__\0") }?;
    let count = unsafe { (api.send)(store, intern(api, b"attrCount:\0"), [globals].as_ptr(), 1) };
    if unsafe { (birnam_abi::integer::is_integer)(count) } == 0 {
        return None;
    }
    let count = unsafe { (birnam_abi::integer::value)(count) };
    for index in 0..count {
        let attr = unsafe {
            (api.send)(
                store,
                intern(api, b"attrAt:index:\0"),
                [globals, (birnam_abi::integer::make)(index)].as_ptr(),
                2,
            )
        };
        let value = unsafe {
            (api.send)(
                store,
                intern(api, b"currentValue:attr:\0"),
                [globals, attr].as_ptr(),
                2,
            )
        };
        if value == target {
            return unsafe { symbol_name(api, attr) };
        }
    }
    None
}

unsafe fn record_alien_wrap(api: &IdApi, arguments: &[Oop]) {
    if arguments.len() != 4 {
        return;
    }
    let Some(target) = (unsafe { global_name(api, arguments[3]) }) else {
        return;
    };
    let Some(function) = (unsafe { string_bytes(api, arguments[0]) }) else {
        return;
    };
    let Some(signature) = (unsafe { string_bytes(api, arguments[1]) }) else {
        return;
    };
    let Some(selector) = (unsafe { symbol_name(api, arguments[2]) }) else {
        return;
    };
    if function.contains(&b'"') || signature.contains(&b'"') {
        return;
    }
    let mut source =
        Vec::with_capacity(function.len() + signature.len() + target.len() + selector.len() + 48);
    source.extend_from_slice(b"[Alien wrap: \"");
    source.extend_from_slice(&function);
    source.extend_from_slice(b"\" signature: \"");
    source.extend_from_slice(&signature);
    source.extend_from_slice(b"\" as: #");
    source.extend_from_slice(&selector);
    source.extend_from_slice(b" on: ");
    source.extend_from_slice(&target);
    source.push(b']');
    unsafe { record_alien_source(api, &source) };
}

unsafe fn is_error_object(api: &IdApi, value: Oop) -> bool {
    let error_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"ErrorVt".as_ptr()) };
    !error_vt.is_null() && unsafe { (api.vtableOf)(value) } == error_vt
}

unsafe fn make_error(api: &IdApi, kind: &[u8], message: &[u8]) -> Oop {
    let factory = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
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
        LoaderError::OpenFailed => b"loadFailed\0".as_slice(),
        LoaderError::SymbolNotFound => b"symbolNotFound\0".as_slice(),
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
    if let Some(runtime) = RUNTIME.get() {
        // SAFETY: the Alien family and singleton are cached by this loaded
        // module and are semantic roots for its dispatch surface.
        unsafe {
            mark(runtime.alien_vt as Oop);
            mark(runtime.alien_obj as Oop);
        }
    }
    let live = callbacks()
        .lock()
        .unwrap_or_else(|_| abort("Alien callback registry was poisoned"));
    // `MarkFn` only records a root in the host collector worklist; it cannot
    // re-enter an Alien method or the Rust registry. Keep the lock while
    // marking so GC never allocates a temporary Rust snapshot.
    for entry in live.iter() {
        // SAFETY: every entry was copied from a live Oop supplied by the host;
        // the root provider is called synchronously by that same collector.
        unsafe {
            mark(entry.block as Oop);
            mark(entry.unit as Oop);
        }
    }
}

unsafe fn ensure_callback_roots(roots: *const birnam_abi::root_registry::ApiV1) {
    CALLBACK_ROOTS.call_once(|| {
        // SAFETY: the host IdApi retains this process-owned root provider for
        // the lifetime of the floor. Registration occurs once per process.
        unsafe { birnam_abi::root_registry::register(roots, callback_roots) };
    });
}

unsafe fn address_value(api: &IdApi, value: Oop) -> Option<usize> {
    unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), value) }.map(|(address, _kind)| address)
}

unsafe fn resolve_buffer_address(api: &IdApi, receiver: Oop, offset: Oop) -> Result<usize, Oop> {
    let Some((base, _kind)) = (unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), receiver) }) else {
        return Err(unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"memory access needs a ForeignAddress receiver",
            )
        });
    };
    if base == 0 {
        return Err(unsafe {
            make_error(
                api,
                b"nullPointer\0",
                b"cannot peek/poke through a null pointer",
            )
        });
    }
    if unsafe { (birnam_abi::integer::is_integer)(offset) } == 0 {
        return Err(unsafe {
            make_error(api, b"typeMismatch\0", b"the offset must be an Integer")
        });
    }
    let signed = unsafe { (birnam_abi::integer::value)(offset) } as u64;
    Ok(base.wrapping_add(signed as usize))
}

unsafe extern "C" fn alien_alloc(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (birnam_abi::integer::is_integer)(value) } == 0 {
        return unsafe {
            make_error(
                api,
                b"typeMismatch\0",
                b"alloc: needs an Integer byte count",
            )
        };
    }
    let size = unsafe { (birnam_abi::integer::value)(value) };
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
    let Some(address) =
        (unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), pointer as usize, KIND_ALIEN_MALLOC) })
    else {
        unsafe { free(pointer) };
        return unsafe {
            make_error(
                api,
                b"outOfMemory\0",
                b"alloc: could not allocate the ForeignAddress owner",
            )
        };
    };
    unsafe { birnam_abi::finalizers::set(API.get().unwrap().6 as *const birnam_abi::finalizers::ApiV1, address, alien_buffer_finalize) };
    address
}

unsafe extern "C" fn alien_free(receiver: Oop, _args: *const Oop, _count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some((address, kind)) = (unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), receiver) }) else {
        return unsafe { make_error(api, b"typeMismatch\0", b"free needs a ForeignAddress") };
    };
    if kind != KIND_ALIEN_MALLOC {
        return unsafe {
            make_error(
                api,
                b"notOwned\0",
                b"free only accepts an address returned by Alien alloc:",
            )
        };
    }
    if address == 0 {
        return unsafe { make_error(api, b"alreadyFreed\0", b"the address was already freed") };
    }
    unsafe { foreign_address::clear(api, execution_imports(), execution_vtables(), receiver) };
    unsafe { free(address as *mut c_void) };
    api.nilObj
}

unsafe extern "C" fn foreign_address_is_null(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    match unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), receiver) } {
        Some((0, _kind)) => api.trueObj,
        Some((_address, _kind)) => api.falseObj,
        None => api.nilObj,
    }
}

unsafe extern "C" fn foreign_address_equal(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let (Some((left, _left_kind)), Some(other)) =
        (unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), receiver) }, unsafe {
            argument(args, count)
        })
    else {
        return api.nilObj;
    };
    match unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), other) } {
        Some((right, _right_kind)) if left == right => api.trueObj,
        _ => api.falseObj,
    }
}

unsafe extern "C" fn foreign_address_print_string(
    receiver: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some((address, _kind)) = (unsafe { foreign_address::value(api, execution_imports(), execution_vtables(), receiver) }) else {
        return api.nilObj;
    };
    let text = format!("ForeignAddress(0x{address:x})");
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

unsafe extern "C" fn alien_buffer_finalize(object: Oop) {
    // Only Alien-owned buffers receive this finalizer, so reading the fixed
    // two-word payload does not need an IdApi query during collection.
    let address = unsafe { object.cast::<usize>().read() };
    let kind = unsafe { object.cast::<usize>().add(1).read() };
    if address != 0 && kind == KIND_ALIEN_MALLOC {
        unsafe { object.cast::<usize>().write(0) };
        unsafe { free(address as *mut c_void) };
    }
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
    // contract; unaligned reads match the native `align(1)` implementation.
    let value = unsafe { ptr::read_unaligned(address as *const i64) };
    unsafe { (birnam_abi::integer::make)(value) }
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
    unsafe { (birnam_abi::integer::make)(i64::from(value)) }
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
    unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), value, KIND_BORROWED) }.unwrap_or(api.nilObj)
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
    unsafe { (birnam_abi::float::make)(value) }
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
    unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn alien_poke_int(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 || unsafe { (birnam_abi::integer::is_integer)(arguments[0]) } == 0 {
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
    let value = unsafe { (birnam_abi::integer::value)(arguments[0]) };
    unsafe { ptr::write_unaligned(address as *mut i64, value) };
    receiver
}

unsafe extern "C" fn alien_poke_byte(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 2 || unsafe { (birnam_abi::integer::is_integer)(arguments[0]) } == 0 {
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
    let value = unsafe { (birnam_abi::integer::value)(arguments[0]) } as u8;
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
                b"pokePtr: needs a ForeignAddress value",
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
    if arguments.len() != 2 || unsafe { (birnam_abi::float::is_float)(arguments[0]) } == 0 {
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
    let value = unsafe { (birnam_abi::float::value)(arguments[0]) };
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
    if LIVE_WRAPPERS.load(Ordering::Relaxed) != 0 && registry.open_count(&path) != 0 {
        drop(registry);
        return unsafe {
            make_error(
                api,
                b"inUse\0",
                b"dlclose: a live wrapper may still call this library",
            )
        };
    }
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
    let record_arguments = [function_object, signature_object, selector, target];
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
    if !signature.aggregates.is_empty() && unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) }.is_null() {
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
    let tcc = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Tcc".as_ptr()) };
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
    let source_object = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
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
        unsafe { (birnam_abi::string::make)(function_name.as_ptr(), function_name.len()) };
    let Some(address_object) = (unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), address, KIND_BORROWED) })
    else {
        return unsafe { make_error(api, b"outOfMemory\0", b"could not box the symbol address") };
    };
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
        let vector = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) };
        let vector_name = unsafe { (birnam_abi::string::make)(c"__vecFactory".as_ptr().cast(), 12) };
        let Some(vector_address) =
            (unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), vector as usize, KIND_BORROWED) })
        else {
            return unsafe {
                make_error(api, b"outOfMemory\0", b"could not box the Vector address")
            };
        };
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
    let shim_name = unsafe { (birnam_abi::string::make)(c"__shim".as_ptr().cast(), 6) };
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
    if installed != api.nilObj {
        LIVE_WRAPPERS.fetch_add(1, Ordering::Relaxed);
        if !unsafe { is_error_object(api, installed) } {
            unsafe { record_alien_wrap(api, &record_arguments) };
        }
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
    if block.is_null() || block == api.nilObj {
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
    if !signature.aggregates.is_empty() && unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) }.is_null() {
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
    let tcc = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Tcc".as_ptr()) };
    if tcc.is_null() {
        return unsafe { make_error(api, b"moduleNotLoaded\0", b"Tcc is not loaded") };
    }
    // SAFETY: this is the same synchronous TCC send sequence used by wrappers;
    // no Rust borrow survives any send.
    let unit = unsafe { (api.send)(tcc, intern(api, b"new\0"), ptr::null(), 0) };
    if unit.is_null() || unit == api.nilObj {
        return unsafe { make_error(api, b"compileFailed\0", b"could not create a callback unit") };
    }
    let source_object = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
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
    let block_name = unsafe { (birnam_abi::string::make)(c"__cbBlock".as_ptr().cast(), 9) };
    let Some(block_address) =
        (unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), block as usize, KIND_BORROWED) })
    else {
        return unsafe { make_error(api, b"outOfMemory\0", b"could not box the callback block") };
    };
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
        let vector = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Vector".as_ptr()) };
        let vector_name = unsafe { (birnam_abi::string::make)(c"__vecFactory".as_ptr().cast(), 12) };
        let Some(vector_address) =
            (unsafe { foreign_address::allocate(api, execution_layouts(), execution_presence(), execution_imports(), execution_exports(), vector as usize, KIND_BORROWED) })
        else {
            return unsafe {
                make_error(api, b"outOfMemory\0", b"could not box the Vector address")
            };
        };
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
    let symbol_name = unsafe { (birnam_abi::string::make)(c"__cb".as_ptr().cast(), 4) };
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
    unsafe { foreign_address::clear(api, execution_imports(), execution_vtables(), value) };
    api.trueObj
}

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, roots: *const birnam_abi::root_registry::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1, finalizers: *const birnam_abi::finalizers::ApiV1, roots_owner: *const birnam_abi::root_registry::OwnedV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let address = api_pointer as usize;
    match API.get() {
        Some(existing) if existing.0 != address => abort("IdApi was replaced"),
        Some(_) => {}
        None => {
            let _ = API.set((address, presence as usize, imports as usize, exports as usize, vtables as usize, layouts as usize, finalizers as usize));
        }
    }
    let mut retained = if roots_owner.is_null() { None } else { Some(unsafe { birnam_abi::root_registry::ConstructionRoots::new(roots_owner) }) };
    let api = unsafe { &*api_pointer };
    let Some((foreign_address_vt, _foreign_address)) =
        (unsafe { foreign_address::ensure_family_with_roots(api, layouts, presence, imports, exports, retained.as_mut()) })
    else {
        abort("ForeignAddress exports were incomplete");
    };
    unsafe { ensure_callback_roots(roots) };
    let mut retain = |object| { if let Some(roots) = &mut retained { unsafe { roots.keep(object) } } else { object } };
    let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, ALIEN_NAME.as_ptr().cast()) } != 0;
    let (alien_vt, alien_obj) = if reloading {
        let alien_vt = unsafe { birnam_abi::namespace_import::import(imports, ALIEN_VT_NAME.as_ptr().cast()) };
        let alien_obj = unsafe { birnam_abi::namespace_import::import(imports, ALIEN_NAME.as_ptr().cast()) };
        (alien_vt, alien_obj)
    } else {
        let alien_vt = retain(unsafe { (api.proto)(api.object_vt) });
        let alien_obj = retain(unsafe { (api.alloc)(alien_vt, 0) });
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
            foreign_address_vt,
            intern(api, b"printString\0"),
            foreign_address_print_string,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"=\0"),
            foreign_address_equal,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"isNull\0"),
            foreign_address_is_null,
        );
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
        (api.method)(foreign_address_vt, intern(api, b"free\0"), alien_free);
        (api.method)(
            foreign_address_vt,
            intern(api, b"peekIntAt:\0"),
            alien_peek_int,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"peekByteAt:\0"),
            alien_peek_byte,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"peekPtrAt:\0"),
            alien_peek_pointer,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"peekDoubleAt:\0"),
            alien_peek_double,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"peekStringAt:\0"),
            alien_peek_string,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"pokeInt:at:\0"),
            alien_poke_int,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"pokeByte:at:\0"),
            alien_poke_byte,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"pokePtr:at:\0"),
            alien_poke_pointer,
        );
        (api.method)(
            foreign_address_vt,
            intern(api, b"pokeDouble:at:\0"),
            alien_poke_double,
        );
    }
    if !reloading {
        unsafe {
            let _ = birnam_abi::namespace_export::export(exports, ALIEN_VT_NAME.as_ptr().cast(), runtime.alien_vt as Oop);
            let _ = birnam_abi::namespace_export::export(exports, ALIEN_NAME.as_ptr().cast(), runtime.alien_obj as Oop);
        }
    }
}

/// Identity of this Rust Alien owner. The value is NUL-terminated for the
/// existing dynamic-rung diagnostic convention.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_alien_rust_identity() -> *const c_char {
    RUST_ALIEN_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust Alien object family and its dynamic-library protocol.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    let _ = state;
    // SAFETY: the host supplies the process-owned IdApi table and retains it
    // for the module lifetime.
    unsafe { bring_up(api, presence, imports, exports, vtables, roots, layouts, finalizers, ptr::null()) };
    ptr::null_mut()
}

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { API.get().expect("module not initialized").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { API.get().expect("module not initialized").2 as *const birnam_abi::namespace_import::ApiV1 }

fn execution_exports() -> *const birnam_abi::namespace_export::ApiV1 { API.get().expect("module not initialized").3 as *const birnam_abi::namespace_export::ApiV1 }
fn execution_layouts() -> *const birnam_abi::payload_layout::ApiV1 { API.get().expect("module not initialized").5 as *const birnam_abi::payload_layout::ApiV1 }

fn execution_vtables() -> *const birnam_abi::vtable_enumeration::ApiV2 { API.get().expect("module not initialized").4 as *const _ }

use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
static NATIVE_PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static NATIVE_RESULT: OnceLock<usize> = OnceLock::new();
fn invalid_native() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe extern "C" fn initialize_native(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    use std::sync::atomic::Ordering;
    if !unsafe { native::valid_protocol(id) } { return invalid_native(); }
    let Some([api, presence, imports, exports, roots, layouts, vtables, finalizers, integer, float, string, names, tcc, store, vector, error]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid_native(); };
    let Some(api) = api.as_word() else { return invalid_native(); };
    let Some(presence) = presence.as_word() else { return invalid_native(); };
    let Some(imports) = imports.as_word() else { return invalid_native(); };
    let Some(exports) = exports.as_word() else { return invalid_native(); };
    let Some(roots) = roots.as_word() else { return invalid_native(); };
    let Some(layouts) = layouts.as_word() else { return invalid_native(); };
    let Some(vtables) = vtables.as_word() else { return invalid_native(); };
    let Some(finalizers) = finalizers.as_word() else { return invalid_native(); };
    let Some(integer) = integer.as_word() else { return invalid_native(); };
    let Some(float) = float.as_word() else { return invalid_native(); };
    let Some(string) = string.as_word() else { return invalid_native(); };
    let Some(names) = names.as_word() else { return invalid_native(); };
    let api = api as *mut IdApi;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver } { return invalid_native(); }
    let presence = presence as *const birnam_abi::namespace_presence::ApiV1;
    if !unsafe { birnam_abi::namespace_presence::valid(presence) } { return invalid_native(); }
    let imports = imports as *const birnam_abi::namespace_import::ApiV1;
    if !unsafe { birnam_abi::namespace_import::valid(imports) } { return invalid_native(); }
    let exports = exports as *const birnam_abi::namespace_export::ApiV1;
    if !unsafe { birnam_abi::namespace_export::valid(exports) } { return invalid_native(); }
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    if !unsafe { birnam_abi::root_registry::valid_owned(roots) } { return invalid_native(); }
    let layouts = layouts as *const birnam_abi::payload_layout::ApiV1;
    if !unsafe { birnam_abi::payload_layout::valid(layouts) } { return invalid_native(); }
    let vtables = vtables as *const birnam_abi::vtable_enumeration::ApiV2;
    if !unsafe { birnam_abi::vtable_enumeration::valid(vtables) } { return invalid_native(); }
    let finalizers = finalizers as *const birnam_abi::finalizers::ApiV1;
    if !unsafe { birnam_abi::finalizers::valid(finalizers) } { return invalid_native(); }
    if unsafe { birnam_abi::integer::attach(integer as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::float::attach(float as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::string::attach(string as _) } != 1 { return invalid_native(); }
    if unsafe { birnam_abi::namespace_names::attach(names as _) } != 1 { return invalid_native(); }
    if tcc.as_object().filter(|p| !p.is_null()).is_none() { return invalid_native(); }
    if store.as_object().filter(|p| !p.is_null()).is_none() { return invalid_native(); }
    if vector.as_object().filter(|p| !p.is_null()).is_none() { return invalid_native(); }
    if error.as_object().filter(|p| !p.is_null()).is_none() { return invalid_native(); }
    if API.get().is_some_and(|selected| selected.0 != api.addr()) { return invalid_native(); }
    match NATIVE_PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::object(*NATIVE_RESULT.get().unwrap() as Oop)), _ => return invalid_native(),
    }
    unsafe { bring_up(api, presence, imports, exports, vtables, (*roots).api, layouts, finalizers, roots); }
    let result = RUNTIME.get().unwrap().alien_obj as Oop;
    assert!(NATIVE_RESULT.set(result.addr()).is_ok());
    NATIVE_PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::object(result))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: core::mem::size_of::<native::Module>() as u32, initialize: Some(initialize_native) };
