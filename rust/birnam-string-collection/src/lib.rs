//! Late String-to-Vector adapter, installed at Collection's original point.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::string_collection::ApiV1;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Collection supplies its original selection, even when registration reenters
// with another table before the first split installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("String collection methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}
unsafe fn string_copy(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
    if object.is_null()
        || unsafe { (api.vtableOf)(object) } != unsafe { birnam_abi::string::behavior() }
    {
        return None;
    }
    let mut length = 0;
    let bytes = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
    if length == 0 {
        return Some(Vec::new());
    }
    if bytes.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec())
}

unsafe extern "C" fn string_split(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(separator) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if unsafe { (api.vtableOf)(separator) } != unsafe { birnam_abi::string::behavior() } {
        let result = unsafe { birnam_abi::vector::empty() };
        return unsafe { birnam_abi::vector::push(result, self_object) };
    }
    let Some(bytes) = (unsafe { string_copy(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(separator) = (unsafe { string_copy(api, separator) }) else {
        return api.nilObj;
    };
    let mut result = unsafe { birnam_abi::vector::empty() };
    if separator.is_empty() {
        let item = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
        return unsafe { birnam_abi::vector::push(result, item) };
    }
    let mut start = 0usize;
    let mut cursor = 0usize;
    while cursor + separator.len() <= bytes.len() {
        if bytes[cursor..cursor + separator.len()] == separator[..] {
            let item = unsafe {
                (birnam_abi::string::make)(bytes[start..cursor].as_ptr(), cursor - start)
            };
            result = unsafe { birnam_abi::vector::push(result, item) };
            cursor += separator.len();
            start = cursor;
        } else {
            cursor += 1;
        }
    }
    let item = unsafe { (birnam_abi::string::make)(bytes[start..].as_ptr(), bytes.len() - start) };
    unsafe { birnam_abi::vector::push(result, item) }
}

unsafe extern "C" fn install(registration: *mut IdApi, execution: *const IdApi) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid String collection installation"
    );
    // Select execution before interning/registration callbacks can invoke split.
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(
            birnam_abi::string::behavior(),
            (api.intern)(c"split:".as_ptr()),
            string_split,
        );
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "String collection received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_string_collection_provider_v1() -> *const ApiV1 {
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
birnam_abi::vector_dependency!();
