//! Shared lazy Error factory dispatch and exact recognition (81AM).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::ApiV3;
use birnam_abi::namespace_import::{self, ApiV1 as ImportV1};
use birnam_abi::namespace_presence::{self, ApiV1 as PresenceV1};
use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_int};
use std::sync::atomic::{AtomicBool, Ordering};
static READY: AtomicBool = AtomicBool::new(false);

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn is_error(
    api: &IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    value: Oop,
) -> bool {
    if value.is_null()
        || unsafe { namespace_presence::contains(presence, c"ErrorVt".as_ptr()) } == 0
    {
        return false;
    }
    let error_vt = unsafe { namespace_import::import(imports, c"ErrorVt".as_ptr()) };
    !error_vt.is_null() && unsafe { (api.vtableOf)(value) } == error_vt
}

unsafe fn make_error(
    api: &IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    kind: Oop,
    message: Oop,
) -> Oop {
    // Error owns its representation and Object protocol in the standalone
    // Error rung. Prelude faults are intentionally lazy: before that rung is
    // loaded, the floor has no catchable Error value and answers nil, matching
    // the native prelude's `Error` import behavior.
    if unsafe { namespace_presence::contains(presence, c"Error".as_ptr()) } == 0 {
        return api.nilObj;
    }
    let factory = unsafe { namespace_import::import(imports, c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let args = [kind, message];
    // SAFETY: the standalone Error factory owns this selector and the stack
    // argument array remains live for the duration of the synchronous send.
    unsafe { (api.send)(factory, selector, args.as_ptr(), args.len()) }
}

unsafe fn numeric_error(
    api: &IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    kind: &[u8],
    message: &[u8],
) -> Oop {
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    unsafe { make_error(api, presence, imports, kind, message) }
}

unsafe extern "C" fn exact(
    api: *const IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    value: Oop,
) -> c_int {
    assert!(!api.is_null(), "Error value received a null IdApi");
    c_int::from(unsafe { is_error(&*api, presence, imports, value) })
}
unsafe extern "C" fn from_bytes(
    api: *const IdApi,
    presence: *const PresenceV1,
    imports: *const ImportV1,
    kind: *const u8,
    kind_length: usize,
    message: *const u8,
    message_length: usize,
) -> Oop {
    assert!(!api.is_null(), "Error value received a null IdApi");
    // The caller supplies the same valid borrowed slices used by the original
    // helper, including a terminated kind. Reconstruct without scanning/copying.
    unsafe {
        numeric_error(
            &*api,
            presence,
            imports,
            core::slice::from_raw_parts(kind, kind_length),
            core::slice::from_raw_parts(message, message_length),
        )
    }
}
static API: ApiV3 = ApiV3 {
    version: 3,
    byte_size: size_of::<ApiV3>(),
    is_error: Some(exact),
    from_bytes: Some(from_bytes),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

unsafe extern "C" fn initialize_native(id: *const birnam_abi::id_protocol::Protocol, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
    use birnam_abi::id_protocol::{self as native, Operand, Outcome};
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([string]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(string) = string.as_word() else { return invalid(); };
    if unsafe { birnam_abi::string::attach(string as _) } != 1 { return invalid(); }
    READY.store(true, Ordering::Release);
    Outcome::loaded(Operand::word((&API as *const ApiV3).addr()))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: birnam_abi::id_protocol::Module = birnam_abi::id_protocol::Module {
    version: 1, byte_size: size_of::<birnam_abi::id_protocol::Module>() as u32,
    initialize: Some(initialize_native),
};
