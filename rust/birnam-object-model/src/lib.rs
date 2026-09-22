//! Ordinary objects and private behaviors above Id, Roots and GC.
//! All dependencies are opaque objects reached by fresh messages.
mod layout;
mod native;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
