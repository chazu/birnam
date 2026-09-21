//! Existing Word methods, loaded independently of Prelude (Design 81S).
//! Owns canonical objects and representation through an explicit dependency (81T).
#![allow(clippy::missing_safety_doc)]
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
pub mod storage;
mod native;

static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
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
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
