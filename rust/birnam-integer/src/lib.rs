//! Integer representation and operations through Id messages.
mod native;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_integer_rust_identity() -> *const core::ffi::c_char {
    c"rust-integer-v1".as_ptr()
}
