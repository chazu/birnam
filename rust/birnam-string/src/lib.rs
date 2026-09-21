//! Existing String methods, independently loaded from Collection (81W).
//! Owns canonical representation and external byte buffers (81X).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::{MarkFn, Oop};
use core::ffi::c_void;
use core::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
pub mod storage;
mod native;
mod text;
static INSTANCE: AtomicPtr<storage::Instance> = AtomicPtr::new(ptr::null_mut());
static PHASE: AtomicU8 = AtomicU8::new(0);

fn abort(message: &str) -> ! {
    eprintln!("birnam String: {message}");
    std::process::abort();
}

unsafe extern "C" fn storage_roots(mark: MarkFn) {
    let instance = INSTANCE.load(Ordering::Acquire);
    if !instance.is_null() { unsafe { (*instance).trace(mark) }; }
}
unsafe extern "C" fn cleanup(object: Oop) {
    let ops = unsafe { string_provider().read() };
    assert_eq!(unsafe { ((*ops.storage).release)(ops.context, object) }, 1, "String finalizer lost byte ownership");
}
unsafe extern "C" fn cleanup_owner(finalizer: birnam_abi::Finalizer) -> *mut c_void {
    if finalizer as usize == cleanup as *const () as usize {
        INSTANCE.load(Ordering::Acquire).cast()
    } else { ptr::null_mut() }
}

unsafe fn string_provider() -> *mut birnam_abi::string::StringApiV1 {
    if PHASE.load(Ordering::Acquire) < 2 { return ptr::null_mut(); }
    unsafe { (*INSTANCE.load(Ordering::Acquire)).operations() }
}
fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
    let start = bytes
        .iter()
        .position(|byte| !is_space(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_space(*byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
