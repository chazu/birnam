//! Dependency-free production Rust libid floor.
//!
//! The crate exports the complete frozen `IdApi` implementation installed as
//! `libbirnam_id.dylib`.

#[cfg(not(any(test, panic = "abort")))]
compile_error!("every runnable birnam_id cdylib profile must use panic=abort");

pub(crate) use birnam_abi as abi;
mod boundary;
mod dispatch;
mod env;
mod execution;
mod gc;
mod heap;
mod loader;
#[cfg(feature = "ffi-test-hooks")]
mod membership_bench;
mod public_api;
mod raw;
mod registration;
mod runtime;
mod symbol;
mod values;
mod vtable;

use core::ffi::c_char;

const RUST_CRATE_IDENTITY: &[u8] = b"rust-id-spike-r2.6\0";
const RUST_FLOOR_IDENTITY: &[u8] = b"rust-id-v1\0";

/// Returns the legacy R2.6 diagnostic identity retained for frozen probes.
///
/// This intentionally has a different name from the selected-floor identity
/// export and is not used for production floor selection.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_rust_crate_identity() -> *const c_char {
    boundary::require_invariant(
        RUST_CRATE_IDENTITY.last() == Some(&0),
        "diagnostic identity lost its C terminator",
    );
    RUST_CRATE_IDENTITY.as_ptr().cast()
}

/// Identifies this dylib as the production Rust floor.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_floor_identity() -> *const c_char {
    boundary::require_invariant(
        RUST_FLOOR_IDENTITY.last() == Some(&0),
        "selected-floor identity lost its C terminator",
    );
    RUST_FLOOR_IDENTITY.as_ptr().cast()
}

/// Returns the pointer-free build/test descriptor for the frozen v1 ABI.
///
/// The descriptor is diagnostic evidence only and is not part of `IdApi`.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_rust_abi_descriptor_v1() -> *const core::ffi::c_void {
    core::ptr::addr_of!(abi::ABI_DESCRIPTOR_V1).cast()
}

/// Initialize the Rust floor exactly once in this process.
#[unsafe(no_mangle)]
pub extern "C" fn id_init() -> *mut core::ffi::c_void {
    public_api::initialize_once().cast()
}

/// Private host/floor admission seam for Design 60J Symbol compaction.
///
/// This is deliberately outside frozen IdApi v1. `enabled != 0` admits weak
/// Symbol sweeping. Compatibility mode requires nonzero identity-count and
/// owned-name-byte ceilings and is rejected when the current interner already
/// exceeds either ceiling.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_symbol_compaction_configure_v1(
    enabled: core::ffi::c_int,
    compatibility_count_limit: usize,
    compatibility_name_byte_limit: usize,
) -> core::ffi::c_int {
    core::ffi::c_int::from(public_api::configure_symbol_compaction(
        enabled != 0,
        compatibility_count_limit,
        compatibility_name_byte_limit,
    ))
}

/// Private read-only diagnostics used by bounded-process acceptance. These
/// counters do not extend frozen `IdApi` and carry no mutation authority.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_live_symbol_count_v1() -> usize {
    public_api::live_symbol_count()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_live_symbol_name_bytes_v1() -> usize {
    public_api::live_symbol_name_bytes()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_live_heap_bytes_v1() -> usize {
    public_api::live_heap_bytes()
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

/// Test-only proof that the process runtime rejects a different calling thread.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_test_non_owner_abort() {
    let api_address = public_api::initialize_once() as usize;
    std::thread::spawn(move || {
        // SAFETY: initialize_once leaks the complete IdApi table for the process.
        let api = unsafe { &*(api_address as *const abi::IdApi) };
        // SAFETY: gcLiveCount takes no foreign pointers; this deliberately
        // violates only the owner-thread precondition under test.
        unsafe { (api.gcLiveCount)() };
    })
    .join()
    .expect("non-owner Rust floor call must terminate the process");
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

/// Test-only optimized membership benchmark entrypoint.
///
/// # Safety
///
/// `output` must be non-null, aligned, and writable for one
/// `membership_bench::BenchReport`.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_test_membership_bench(
    output: *mut core::ffi::c_void,
) -> core::ffi::c_int {
    let Some(output) = core::ptr::NonNull::new(output.cast::<membership_bench::BenchReport>())
    else {
        return 0;
    };
    let report = membership_bench::run();
    // SAFETY: the caller supplies exclusive writable storage for one report.
    unsafe { output.as_ptr().write(report) };
    1
}

/// Test-only harness that installs and invokes one foreign reentrant Method.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_test_dispatch_reentry(method: abi::Method) -> usize {
    public_api::test_dispatch_reentry(method)
}

/// Test-only send seam available only during the synchronous reentry harness.
///
/// # Safety
///
/// The ordinary `IdApi.send` receiver, selector, and argument-buffer contracts
/// apply.
#[cfg(feature = "ffi-test-hooks")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_test_active_send(
    receiver: abi::Oop,
    selector: abi::Oop,
    args: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    // SAFETY: forwarded caller contract. This deliberately goes through the
    // exact public wrapper so the already-active fast path is exercised.
    unsafe { public_api::test_active_send(receiver, selector, args, count) }
}

#[cfg(test)]
mod tests {
    use super::{
        RUST_CRATE_IDENTITY, RUST_FLOOR_IDENTITY, birnam_id_floor_identity,
        birnam_id_rust_crate_identity,
    };
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

        // SAFETY: same static NUL-terminated contract for selected-floor tests.
        let floor_identity = unsafe { CStr::from_ptr(birnam_id_floor_identity()) };
        assert_eq!(floor_identity.to_bytes_with_nul(), RUST_FLOOR_IDENTITY);
    }
}
