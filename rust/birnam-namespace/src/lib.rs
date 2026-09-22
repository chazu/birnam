//! Ordered Symbol bindings owned by Namespace, accessed through Id messages.
mod index;
mod native;
mod state;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
