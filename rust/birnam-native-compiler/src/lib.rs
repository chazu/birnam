//! Internal native compiler implementation shared by runtime components.
//!
//! This crate has no IdApi state, language methods, module initializer or
//! exported Birnam symbols. Callers own executable lifetime and serialize
//! compiler operations. The native function declarations preserve libtcc's
//! existing interface; they are not a new Birnam native contract.

use core::ffi::{c_char, c_int, c_void};

mod unit;
pub use unit::Unit;

#[repr(C)]
pub struct TccState {
    _private: [u8; 0],
}

pub type TccErrorFunc = unsafe extern "C" fn(*mut c_void, *const c_char);

unsafe extern "C" {
    pub fn tcc_new() -> *mut TccState;
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
