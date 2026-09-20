//! Existing Word methods, loaded independently of Prelude (Design 81S).
//! Owns canonical objects and representation through an explicit dependency (81T).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use birnam_abi::error_value::numeric_error;
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;

static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
// 0 unprepared, 1 preparing, 2 prepared, 3 activating, 4 active.
static PHASE: AtomicU8 = AtomicU8::new(0);
fn abort(message: &str) -> ! {
    eprintln!("birnam Word: {message}");
    std::process::abort();
}
unsafe fn api_ref() -> &'static IdApi {
    let instance = INSTANCE.load(Ordering::Acquire);
    if instance.is_null() {
        abort("Word was not prepared");
    }
    unsafe { &*(*instance).api() }
}
unsafe extern "C" fn word_roots(mark: birnam_abi::MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() {
        unsafe { (*instance).trace(mark) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_word_provider_v1() -> *mut birnam_abi::word::WordApiV1 {
    if PHASE.load(Ordering::Acquire) < 2 {
        return ptr::null_mut();
    }
    unsafe { (*INSTANCE.load(Ordering::Acquire)).operations() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_word_prepare_v6(
    api: *mut IdApi,
    presence: *const birnam_abi::namespace_presence::ApiV1,
    imports: *const birnam_abi::namespace_import::ApiV1,
    exports: *const birnam_abi::namespace_export::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) -> *mut birnam_abi::word::WordApiV1 {
    if api.is_null() {
        abort("preparation received a null IdApi");
    }
    if PHASE.load(Ordering::Acquire) != 0 {
        let instance = INSTANCE.load(Ordering::Acquire);
        if instance.is_null() || unsafe { (*instance).api() } != api {
            abort("another IdApi is already mounted");
        }
        if PHASE.load(Ordering::Acquire) == 1 {
            abort("recursive Word preparation");
        }
        return unsafe { birnam_word_provider_v1() };
    }
    PHASE.store(1, Ordering::Release);
    let instance = Box::into_raw(unsafe { storage::Instance::empty(api, presence, imports, exports, layouts) });
    INSTANCE.store(instance, Ordering::Release);
    unsafe {
        birnam_abi::root_registry::register(roots, word_roots);
        (*instance).prepare();
    }
    let operations = unsafe { (*instance).operations() };
    if unsafe { birnam_abi::word::attach(operations) } != 1 {
        abort("invalid Word table");
    }
    PHASE.store(2, Ordering::Release);
    operations
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn word_value(_api: &IdApi, value: Oop) -> Option<u64> {
    if value.is_null() || unsafe { (birnam_abi::word::is_word)(value) } == 0 {
        return None;
    }
    Some(unsafe { (birnam_abi::word::value)(value) })
}

unsafe extern "C" fn word_print_string(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    let text = format!("0x{value:x}");
    unsafe { (birnam_abi::string::make)(text.as_ptr(), text.len()) }
}

unsafe extern "C" fn word_eq(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(left) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    let Some(right_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(right) = (unsafe { word_value(api, right_object) }) else {
        return api.falseObj;
    };
    if left == right {
        api.trueObj
    } else {
        api.falseObj
    }
}

unsafe extern "C" fn word_as_integer(self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { word_value(api, self_object) }) else {
        return api.nilObj;
    };
    if value > i64::MAX as u64 {
        return unsafe { numeric_error(api, execution_presence(), execution_imports(), b"overflow\0", b"word out of Integer range") };
    }
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

/// Install the existing Word protocol on this module's canonical behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api_pointer: *mut IdApi, _: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, layouts: *const birnam_abi::payload_layout::ApiV1) -> *mut c_void {
    let operations = unsafe { birnam_word_prepare_v6(api_pointer, presence, imports, exports, roots, layouts) };
    let previous = PHASE.swap(3, Ordering::AcqRel);
    if previous != 2 && previous != 4 {
        abort("recursive Word activation");
    }
    // Verify Integer before the first replaceable publication callback.
    let _ = unsafe { birnam_abi::integer::operations() };
    let api = unsafe { &*api_pointer };
    if previous == 2 {
        unsafe { (*INSTANCE.load(Ordering::Acquire)).publish() };
    }
    // Reinitialization retains canonical objects but reinstalls the existing
    // methods, exactly as the method-only module did before extraction.
    let behavior = unsafe { (*operations).behavior };
    let word_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"printString\0", word_print_string),
        (b"=\0", word_eq),
        (b"asInteger\0", word_as_integer),
    ];
    for (selector, method) in word_methods {
        let symbol = unsafe { intern(api, selector) };
        // SAFETY: word_vt and each callback are process-lifetime values.
        unsafe { (api.method)(behavior, symbol, *method) };
    }
    PHASE.store(4, Ordering::Release);
    ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

birnam_abi::integer_dependency!();

birnam_abi::string_dependency!();

birnam_abi::error_value_dependency!();

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { unsafe { (*INSTANCE.load(Ordering::Acquire)).presence() } }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { unsafe { (*INSTANCE.load(Ordering::Acquire)).imports() } }
