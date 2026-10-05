//! Canonical Boolean/nil identities and native behavior required for cold bootstrap.
//! Production loads the authored booleanValues artifact over these same identities.
mod base;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
