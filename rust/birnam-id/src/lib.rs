//! Dependency-free shell for the prospective Rust libid implementation.
//!
//! This crate is deliberately not a usable Birnam floor yet: it does not
//! export `id_init`, and normal Zig builds neither select nor install it.

#[cfg(not(any(test, panic = "abort")))]
compile_error!("every runnable birnam_id cdylib profile must use panic=abort");

mod abi;
mod boundary;
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
    boundary::require_invariant(
        RUST_CRATE_IDENTITY.last() == Some(&0),
        "diagnostic identity lost its C terminator",
    );
    RUST_CRATE_IDENTITY.as_ptr().cast()
}

/// Returns the pointer-free build/test descriptor for the frozen v1 ABI.
///
/// The descriptor is diagnostic evidence only. It is not part of `IdApi` and
/// does not make this shell a usable floor.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_rust_abi_descriptor_v1() -> *const core::ffi::c_void {
    core::ptr::addr_of!(abi::ABI_DESCRIPTOR_V1).cast()
}

/// Test-only proof that a Rust panic cannot unwind across the C ABI.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_test_panic_across_ffi() {
    panic!("intentional Rust FFI panic");
}

/// Test-only proof of the explicit fatal-invariant boundary helper.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_test_invariant_abort() {
    boundary::require_invariant(false, "intentional Rust FFI invariant failure");
}

/// Test-only bridge through which an external C-layout fixture inspects one
/// isolated Rust heap block.
///
/// # Safety
///
/// The callback must obey `heap::run_layout_probe`'s pointer lifetime and
/// bounds contract.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_test_heap_layout(
    requested_size: usize,
    vtable: abi::Oop,
    probe: heap::LayoutProbe,
) -> core::ffi::c_int {
    // SAFETY: the exported function forwards the documented caller contract.
    unsafe { heap::run_layout_probe(requested_size, vtable, probe) }
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
