//! Byte storage and bounded generic binary operations. Text policy belongs to String.
mod native;
mod binary_hash;
mod binary_rows;
mod numeric;
mod tagged_view;
mod tagged_rows;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 { 1 }
