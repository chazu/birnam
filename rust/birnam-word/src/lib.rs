//! Word storage and operations through Id messages.
mod native;
mod expression;
mod operations;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
