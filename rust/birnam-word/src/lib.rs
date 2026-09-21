//! Existing Word methods, loaded independently of Prelude (Design 81S).
//! Owns canonical objects and representation through an explicit dependency (81T).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::IdApi;
use core::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;
mod native;

static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
// 0 unprepared, 1 preparing, 2 prepared, 3 activating, 4 active.
static PHASE: AtomicU8 = AtomicU8::new(0);
fn abort(message: &str) -> ! {
    eprintln!("birnam Word: {message}");
    std::process::abort();
}
unsafe extern "C" fn word_roots(mark: birnam_abi::MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() {
        unsafe { (*instance).trace(mark) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_word_provider_v1() -> *mut birnam_abi::word::WordApiV1 {
    if !native::ready() && PHASE.load(Ordering::Acquire) < 2 {
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
    if native::ready() {
        let instance = INSTANCE.load(Ordering::Acquire);
        assert_eq!(unsafe { (*instance).native_access().unwrap().id.receiver }, unsafe { (*api).vtable_vt }, "Word prepared on another Id");
        return unsafe { (*instance).operations() };
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

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
