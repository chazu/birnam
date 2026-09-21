//! Existing Object/nil protocol, independently loaded from Collection (81V).
//! Native predicates precede the later String printing extension.
#![allow(clippy::missing_safety_doc)]
mod base;

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
