//! Internal native compiler implementation shared by runtime components.
//!
//! This crate has no IdApi state, language methods, module initializer or
//! exported Birnam symbols. Callers own executable lifetime and serialize
//! compiler operations. The native function declarations preserve libtcc's
//! existing interface; they are not a new Birnam native contract.

use core::ffi::{c_char, c_int, c_void};

mod unit;
pub use unit::Unit;

#[cfg(feature = "allocation-metrics")]
mod allocation_metrics;
#[cfg(feature = "allocation-metrics")]
pub use allocation_metrics::{HeapAllocationTotals, heap_allocation_totals};

#[repr(C)]
pub struct TccState {
    _private: [u8; 0],
}

pub type TccErrorFunc = unsafe extern "C" fn(*mut c_void, *const c_char);

unsafe extern "C" {
    #[cfg(not(feature = "allocation-metrics"))]
    pub fn tcc_new() -> *mut TccState;
    #[cfg(feature = "allocation-metrics")]
    #[link_name = "tcc_new"]
    fn raw_tcc_new() -> *mut TccState;
    pub fn tcc_delete(state: *mut TccState);
    pub fn tcc_set_error_func(
        state: *mut TccState,
        opaque: *mut c_void,
        func: Option<TccErrorFunc>,
    );
    pub fn tcc_set_output_type(state: *mut TccState, output_type: c_int) -> c_int;
    pub fn tcc_compile_string(state: *mut TccState, source: *const c_char) -> c_int;
    pub fn tcc_add_library(state: *mut TccState, name: *const c_char) -> c_int;
    pub fn tcc_add_library_path(state: *mut TccState, path: *const c_char) -> c_int;
    pub fn tcc_add_file(state: *mut TccState, path: *const c_char) -> c_int;
    pub fn tcc_add_symbol(state: *mut TccState, name: *const c_char, value: *const c_void)
    -> c_int;
    pub fn tcc_output_file(state: *mut TccState, path: *const c_char) -> c_int;
    pub fn tcc_relocate(state: *mut TccState) -> c_int;
    pub fn tcc_get_symbol(state: *mut TccState, name: *const c_char) -> *mut c_void;
}

/// Create a state in this compiler instance.
///
/// # Safety
/// Callers must serialize compiler operations and release the state exactly once.
#[cfg(feature = "allocation-metrics")]
pub unsafe fn tcc_new() -> *mut TccState {
    allocation_metrics::install();
    unsafe { raw_tcc_new() }
}

/// Relocated allocation owned by one raw state in this compiler instance.
/// Includes code, data and alignment storage, excluding compiler metadata.
///
/// # Safety
/// The state must be live, created by this compiler instance, and exclusively
/// observed with respect to compiler mutation or deletion for this call.
pub unsafe fn relocated_allocation_bytes(state: *const TccState) -> usize {
    unsafe extern "C" {
        fn birnam_tcc_relocated_allocation_bytes(state: *const TccState) -> usize;
    }
    unsafe { birnam_tcc_relocated_allocation_bytes(state) }
}
