//! Rust Store rung shell.
//!
//! This first slice establishes only the replaceable `module_init` boundary.
//! It deliberately publishes no Store objects until the storage core owns the
//! complete protocol; the Zig Store remains the sole production implementation
//! during N2.2 bring-up.

use birnam_abi::IdApi;
use core::ffi::{c_char, c_void};

const RUST_STORE_IDENTITY: &[u8] = b"rust-store-n2.2-shell\0";

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Store: {message}");
    std::process::abort();
}

/// Identifies the not-yet-selected Rust Store shell for build and audit tools.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_store_rust_identity() -> *const c_char {
    RUST_STORE_IDENTITY.as_ptr().cast()
}

/// ABI-safe module entrypoint. The shell is inert until its storage protocol is
/// installed; returning no opaque state preserves the current reload contract.
///
/// # Safety
///
/// `api` must be the live process-owned `IdApi` table supplied by the host, or
/// null to request the documented fatal boundary. `_state` is an opaque host
/// token and is not retained by this shell.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    // SAFETY: the host supplies a live IdApi pointer; the shell only checks its
    // address and does not retain a Rust reference across this call.
    let _ = unsafe { &*api };
    core::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_STORE_IDENTITY, birnam_store_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn shell_identity_is_a_terminated_static_string() {
        // SAFETY: the export points to the crate's immutable NUL-terminated
        // static for the process lifetime.
        let identity = unsafe { CStr::from_ptr(birnam_store_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_STORE_IDENTITY);
    }
}
