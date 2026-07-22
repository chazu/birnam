//! Dependency-free shell for the prospective Rust libid implementation.
//!
//! This crate is deliberately not a usable Birnam floor yet: it does not
//! export `id_init`, and normal Zig builds neither select nor install it.

mod abi;
mod dispatch;
mod env;
mod gc;
mod heap;
mod loader;
mod raw;
mod runtime;
mod symbol;
mod values;
mod vtable;

use core::ffi::c_char;

const RUST_CRATE_IDENTITY: &[u8] = b"rust-id-shell-v1\0";

/// Identifies the diagnostic-only R1 Rust artifact.
///
/// This intentionally has a different name from the selected-floor identity
/// export. Its presence does not imply that the dylib implements `IdApi`.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_rust_crate_identity() -> *const c_char {
    RUST_CRATE_IDENTITY.as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::{RUST_CRATE_IDENTITY, birnam_id_rust_crate_identity};
    use std::ffi::CStr;

    #[test]
    fn target_uses_64_bit_pointers() {
        assert_eq!(core::mem::size_of::<usize>(), 8);
    }

    #[test]
    fn diagnostic_identity_is_a_static_c_string() {
        // SAFETY: the exported pointer addresses the static, NUL-terminated
        // RUST_CRATE_IDENTITY byte string for the life of the process.
        let identity = unsafe { CStr::from_ptr(birnam_id_rust_crate_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_CRATE_IDENTITY);
    }
}
