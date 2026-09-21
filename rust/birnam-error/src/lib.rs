//! Standalone Rust Error rung.
//!
//! The Error family is deliberately separate from the Rust Prelude. It owns
//! the two-OOP payload, the propagating `doesNotUnderstand:` method, the
//! factory selectors, and the Object DNU swap. Prelude faults construct values
//! through the exported `Error` factory instead of minting a private family.

#![allow(clippy::missing_safety_doc)]

mod native;

use birnam_abi::Oop;
use core::ffi::c_char;

const RUST_ERROR_IDENTITY: &[u8] = b"rust-error-r2-core\0";

#[repr(C)]
struct ErrorPayload {
    kind: Oop,
    message: Oop,
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
