//! Native Block loops installed after Runtime's Block representation.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::block_loop::ApiV3;
use birnam_abi::error_value::{is_error, numeric_error};
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_char;
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Installation retains its original selection, even when registration reenters
// with another table before the first block loop installation. The host pins it.
static EXECUTION_API: OnceLock<(usize, usize, usize)> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("Block Loop methods were not installed");
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

unsafe fn block_loop(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    continue_on_true: bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(body) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    loop {
        let condition = unsafe { send0(api, self_object, b"value\0") };
        if unsafe { is_error(api, execution_presence(), execution_imports(), condition) } {
            return condition;
        }
        let is_true = condition == api.trueObj;
        let is_false = condition == api.falseObj;
        if !is_true && !is_false {
            return unsafe {
                numeric_error(api, execution_presence(), execution_imports(),
                    b"mustBeBoolean\0",
                    b"while loop condition must answer true or false",
                )
            };
        }
        if is_true != continue_on_true {
            return api.nilObj;
        }
        let result = unsafe { send0(api, body, b"value\0") };
        if unsafe { is_error(api, execution_presence(), execution_imports(), result) } {
            return result;
        }
    }
}

unsafe extern "C" fn block_while_true(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { block_loop(self_object, args, count, true) }
}

unsafe extern "C" fn block_while_false(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { block_loop(self_object, args, count, false) }
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    block_vt: Oop,
    while_true: Oop,
    while_false: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid Block Loop installation"
    );
    EXECUTION_API.get_or_init(|| (execution as usize, presence as usize, imports as usize));
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(block_vt, while_true, block_while_true);
        (api.method)(block_vt, while_false, block_while_false);
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { EXECUTION_API.get().expect("methods not installed").1 as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { EXECUTION_API.get().expect("module not installed").2 as *const birnam_abi::namespace_import::ApiV1 }

unsafe fn mount(api: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1) {
    assert!(!api.is_null(), "Block Loop received a null IdApi");
    // Select execution before namespace/intern callbacks can reenter native construction.
    EXECUTION_API.get_or_init(|| (api as usize, presence as usize, imports as usize));
    let block_vt = unsafe { birnam_abi::namespace_import::import(imports, c"BlockVt".as_ptr()) };
    assert!(!block_vt.is_null(), "Block Loop requires Runtime's Block representation");
    READY.store(true, Ordering::Release);
    unsafe {
        let while_true = ((*api).intern)(c"whileTrue:".as_ptr());
        let while_false = ((*api).intern)(c"whileFalse:".as_ptr());
        install(api, api, presence, imports, block_vt, while_true, while_false);
    }
}
static NATIVE_BUSY: AtomicBool = AtomicBool::new(false);
static NATIVE_COMPLETE: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn initialize_native(id: *const birnam_abi::id_protocol::Protocol, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([api, presence, imports, errors]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(api), Some(presence), Some(imports), Some(errors)) = (api.as_word(), presence.as_word(), imports.as_word(), errors.as_word()) else { return invalid(); };
    let api = api as *mut IdApi;
    if api.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || EXECUTION_API.get().is_some_and(|selected| *selected != (api.addr(), presence, imports))
        || !unsafe { birnam_abi::namespace_presence::valid(presence as _) && birnam_abi::namespace_import::valid(imports as _) }
        || unsafe { birnam_abi::error_value::attach(errors as _) } != 1 { return invalid(); }
    if NATIVE_COMPLETE.load(Ordering::Acquire) { return Outcome::loaded(Operand::word((&API as *const ApiV3).addr())); }
    if NATIVE_BUSY.swap(true, Ordering::AcqRel) { return invalid(); }
    unsafe { mount(api, presence as _, imports as _) };
    NATIVE_COMPLETE.store(true, Ordering::Release);
    NATIVE_BUSY.store(false, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const ApiV3).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32,
    initialize: Some(initialize_native),
};
