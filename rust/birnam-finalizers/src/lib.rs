//! Native finalizer eligibility and inline callback attachments.
use birnam_abi::finalizers::{SlotV1};
use birnam_abi::{Finalizer, Oop};
use core::ffi::{c_int, c_void};
use core::ptr;

unsafe extern "C" fn slot(permanent: c_int, finalizer: Finalizer) -> SlotV1 {
    if permanent != 0 { SlotV1::EMPTY } else { SlotV1 { callback: Some(finalizer) } }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
