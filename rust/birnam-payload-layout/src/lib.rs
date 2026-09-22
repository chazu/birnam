//! Owned exact-vtable payload descriptions shared by GC, Image and recovery.
use birnam_abi::payload_layout::{OwnedV1, ViewV1};
use birnam_abi::{Oop, SLOT_OOP, SLOT_UNKNOWN};
use core::ffi::{c_int, c_void};
use core::ptr;

unsafe extern "C" fn copy(kinds: *const u8, count: usize, rest: u8) -> OwnedV1 {
    if count != 0 && kinds.is_null() {
        // Preserve the existing public diagnostic and its pre-membership order.
        eprintln!("birnam-id invariant failed: setLayout kinds is null with nonzero count");
        std::process::abort();
    }
    let kinds = if count == 0 {
        ptr::null()
    } else {
        let bytes = unsafe { core::slice::from_raw_parts(kinds, count) }.to_vec().into_boxed_slice();
        Box::into_raw(bytes).cast::<u8>().cast_const()
    };
    OwnedV1 { view: ViewV1 { kinds, count, rest, is_set: 1 } }
}
unsafe extern "C" fn release(owned: OwnedV1) {
    if owned.view.count != 0 {
        let slice = ptr::slice_from_raw_parts_mut(owned.view.kinds.cast_mut(), owned.view.count);
        unsafe { drop(Box::from_raw(slice)) };
    }
}
unsafe extern "C" fn slot_kind(view: ViewV1, word: usize) -> u8 {
    if view.is_set == 0 { return SLOT_UNKNOWN; }
    if word >= view.count { return view.rest; }
    unsafe { *view.kinds.add(word) }
}
unsafe extern "C" fn has_layout(view: ViewV1) -> c_int { c_int::from(view.is_set != 0) }
unsafe extern "C" fn all_oop(view: ViewV1, words: usize) -> c_int {
    if view.is_set == 0 { return 0; }
    for index in 0..words.min(view.count) {
        if unsafe { *view.kinds.add(index) } != SLOT_OOP { return 0; }
    }
    c_int::from(words <= view.count || view.rest == SLOT_OOP)
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
