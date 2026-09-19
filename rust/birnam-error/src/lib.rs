//! Standalone Rust Error rung.
//!
//! The Error family is deliberately separate from the Rust Prelude. It owns
//! the two-OOP payload, the propagating `doesNotUnderstand:` method, the
//! factory selectors, and the Object DNU swap. Prelude faults construct values
//! through the exported `Error` factory instead of minting a private family.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::ffi::CStr;
use std::sync::OnceLock;

const RUST_ERROR_IDENTITY: &[u8] = b"rust-error-r2-core\0";
const ERROR_VT_NAME: &[u8] = b"ErrorVt\0";
const FACTORY_VT_NAME: &[u8] = b"ErrorFactoryVt\0";
const ERROR_NAME: &[u8] = b"Error\0";

#[repr(C)]
struct ErrorPayload {
    kind: Oop,
    message: Oop,
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    error_vt: usize,
    factory_vt: usize,
    factory_obj: usize,
    wild_kind: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();

unsafe extern "C" fn mark_runtime_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.error_vt,
            runtime.factory_vt,
            runtime.factory_obj,
            runtime.wild_kind,
        ] {
            unsafe { mark(value as Oop) };
        }
    }
}

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Error: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api_ref() -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains the selected floor for the process lifetime.
    unsafe { &*(runtime().api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn error_payload(object: Oop) -> Option<*mut ErrorPayload> {
    if object.is_null() {
        None
    } else {
        Some(object.cast::<ErrorPayload>())
    }
}

unsafe fn make_error(api: &IdApi, kind: Oop, message: Oop) -> Oop {
    let error_vt = runtime().error_vt as Oop;
    let object = unsafe { (api.alloc)(error_vt, size_of::<ErrorPayload>()) };
    if object.is_null() {
        return api.nilObj;
    }
    // SAFETY: `object` was allocated with the exact Error payload size and is
    // owned by the Error vtable installed by this module.
    unsafe {
        error_payload(object)
            .unwrap_unchecked()
            .write(ErrorPayload { kind, message });
    }
    object
}

unsafe extern "C" fn error_dnu(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    self_object
}

unsafe extern "C" fn error_is_error(_self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    unsafe { api_ref() }.trueObj
}

unsafe extern "C" fn error_kind(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(payload) = (unsafe { error_payload(self_object) }) else {
        return api.nilObj;
    };
    // SAFETY: this method is installed only on ErrorVt instances.
    unsafe { (*payload).kind }
}

unsafe extern "C" fn error_message(self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    let api = unsafe { api_ref() };
    let Some(payload) = (unsafe { error_payload(self_object) }) else {
        return api.nilObj;
    };
    // SAFETY: this method is installed only on ErrorVt instances.
    unsafe { (*payload).message }
}

unsafe extern "C" fn factory_kind_message(
    _self_object: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    unsafe { make_error(api, arguments[0], arguments[1]) }
}

unsafe extern "C" fn factory_kind(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(kind) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    unsafe { make_error(api, kind, api.nilObj) }
}

unsafe extern "C" fn factory_message(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(message) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    unsafe { make_error(api, runtime().wild_kind as Oop, message) }
}

unsafe extern "C" fn object_is_error(_self_object: Oop, _args: *const Oop, _count: usize) -> Oop {
    unsafe { api_ref() }.falseObj
}

unsafe extern "C" fn object_dnu(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    // Dispatch forwards an unbound send as [selector, arg0, arg1, …], so count
    // is 1 + the original argument count — 1 only for a unary send. Guarding on
    // `count != 1` here made every keyword and binary send answer nil instead of
    // erroring. The selector is args[0] whatever the arity.
    if count == 0 {
        return api.nilObj;
    }
    let Some(selector) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let symbol_name = unsafe { (api.symbolName)(selector) };
    if symbol_name.is_null() {
        return api.nilObj;
    }
    // SAFETY: selector remains rooted by this active send while its borrowed
    // NUL-terminated name is copied into `message` below.
    let name = unsafe { CStr::from_ptr(symbol_name) }.to_bytes();
    let mut message = Vec::with_capacity(b"does not understand #".len() + name.len());
    message.extend_from_slice(b"does not understand #");
    message.extend_from_slice(name);
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    if message.is_null() {
        return api.nilObj;
    }
    let kind = unsafe { intern(api, b"doesNotUnderstand\0") };
    unsafe { make_error(api, kind, message) }
}

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let wild_kind = unsafe { intern(api, b"error\0") };
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, ERROR_NAME.as_ptr().cast()) } != 0;
        let (error_vt, factory_vt, factory_obj) = if reloading {
            (
                unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) },
                unsafe { (api.import)(FACTORY_VT_NAME.as_ptr().cast()) },
                unsafe { (api.import)(ERROR_NAME.as_ptr().cast()) },
            )
        } else {
            let error_vt = unsafe { (api.proto)(api.object_vt) };
            let factory_vt = unsafe { (api.proto)(api.object_vt) };
            let factory_obj = unsafe { (api.alloc)(factory_vt, 0) };
            (error_vt, factory_vt, factory_obj)
        };
        if error_vt.is_null() || factory_vt.is_null() || factory_obj.is_null() {
            abort("Rust Error exports were incomplete");
        }
        Runtime {
            api: api_pointer as usize,
            error_vt: error_vt as usize,
            factory_vt: factory_vt as usize,
            factory_obj: factory_obj as usize,
            wild_kind: wild_kind as usize,
        }
    });
    unsafe {
        (api.setLayout)(
            runtime.error_vt as Oop,
            ptr::null(),
            0,
            birnam_abi::SLOT_OOP,
        );
        (api.method)(
            runtime.error_vt as Oop,
            intern(api, b"doesNotUnderstand:\0"),
            error_dnu,
        );
        (api.method)(
            runtime.error_vt as Oop,
            intern(api, b"isError\0"),
            error_is_error,
        );
        (api.method)(runtime.error_vt as Oop, intern(api, b"kind\0"), error_kind);
        (api.method)(
            runtime.error_vt as Oop,
            intern(api, b"message\0"),
            error_message,
        );
        (api.method)(
            runtime.factory_vt as Oop,
            intern(api, b"kind:message:\0"),
            factory_kind_message,
        );
        (api.method)(
            runtime.factory_vt as Oop,
            intern(api, b"kind:\0"),
            factory_kind,
        );
        (api.method)(
            runtime.factory_vt as Oop,
            intern(api, b"message:\0"),
            factory_message,
        );
        (api.method)(api.object_vt, intern(api, b"isError\0"), object_is_error);
        (api.method)(
            api.object_vt,
            intern(api, b"doesNotUnderstand:\0"),
            object_dnu,
        );
    }
    ROOT_REGISTERED.get_or_init(|| unsafe {
        (api.gcRegisterRootProvider)(mark_runtime_roots);
    });
    if unsafe { birnam_abi::namespace_presence::contains(presence, ERROR_NAME.as_ptr().cast()) } == 0 {
        unsafe {
            let _ = (api.exportObj)(ERROR_VT_NAME.as_ptr().cast(), runtime.error_vt as Oop);
            let _ = (api.exportObj)(FACTORY_VT_NAME.as_ptr().cast(), runtime.factory_vt as Oop);
            let _ = (api.exportObj)(ERROR_NAME.as_ptr().cast(), runtime.factory_obj as Oop);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_error_rust_identity() -> *const c_char {
    RUST_ERROR_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1) -> *mut c_void {
    let _ = state;
    unsafe { bring_up(api, presence) };
    ptr::null_mut()
}

birnam_abi::string_dependency!();
