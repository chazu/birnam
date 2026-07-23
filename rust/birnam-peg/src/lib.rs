//! Rust PEG rung shell.
//!
//! N3.2a establishes the dynamic library and identity boundary without
//! exporting a second `Peg` object. Zig PEG remains the canonical production
//! implementation until the semantic port and observation gate are complete.

use birnam_abi::IdApi;
use core::ffi::{c_char, c_void};
use core::ptr;

const RUST_PEG_IDENTITY: &[u8] = b"rust-peg-n3.2-shell\0";

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust PEG: {message}");
    std::process::abort();
}

/// Identifies this explicit Rust PEG oracle dylib.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_peg_rust_identity() -> *const c_char {
    RUST_PEG_IDENTITY.as_ptr().cast()
}

/// Validate the host boundary without exporting a competing `Peg` singleton.
///
/// # Safety
///
/// `api` is the process-owned IdApi pointer supplied by the host, or null to
/// request the documented fatal boundary. `_state` is not retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    if api.is_null() {
        abort("module_init received a null IdApi");
    }
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_PEG_IDENTITY, birnam_peg_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn shell_identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable crate storage for the process
        // lifetime.
        let identity = unsafe { CStr::from_ptr(birnam_peg_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_PEG_IDENTITY);
    }
}
