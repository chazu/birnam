//! Owned text and text construction, reached exclusively through Id messages.
mod native;
mod text;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
