//! Minimal production Id. All language policy and compiler code lives above it.
#[cfg(not(any(test, panic = "abort")))]
compile_error!("runnable Id requires panic=abort");
pub(crate) use birnam_abi as abi;
mod boundary;
mod owned;
mod raw;
mod id;

#[unsafe(no_mangle)]
pub static birnam_native_module_v1: abi::id_protocol::Module = id::DESCRIPTOR;

#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_floor_identity() -> *const core::ffi::c_char {
    c"rust-id-v1".as_ptr()
}
