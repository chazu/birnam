//! Bounded inherited dictionary answers, without roots or executable leases.
mod guard_verdict;
use birnam_abi::id_protocol::NativeEntry;
use core::{cell::Cell, ffi::c_void, ptr};

const SLOTS: usize = 64;
#[derive(Clone, Copy)]
struct Entry {
    epoch: u64,
    selector: usize,
    shape: usize,
    method: NativeEntry,
}
const EMPTY: Entry = Entry { epoch: u64::MAX, selector: 0, shape: 0, method: NativeEntry { method: None, context: ptr::null_mut() } };
struct Cache([Cell<Entry>; SLOTS]);
fn slot(selector: usize) -> usize { ((selector >> 4) ^ (selector >> 10)) & (SLOTS - 1) }
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cache(core::array::from_fn(|_| Cell::new(EMPTY))))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cache>())) };
}
unsafe extern "C" fn lookup(context: *const c_void, epoch: u64, shape: usize, selector: usize) -> NativeEntry {
    let cache = unsafe { &*context.cast::<Cache>() };
    let entry = cache.0[slot(selector)].get();
    if entry.epoch == epoch && entry.shape == shape && entry.selector == selector { entry.method } else { EMPTY.method }
}
unsafe extern "C" fn insert(context: *mut c_void, epoch: u64, shape: usize, selector: usize, method: NativeEntry) {
    let cache = unsafe { &*context.cast::<Cache>() };
    cache.0[slot(selector)].set(Entry { epoch, shape, selector, method });
}
unsafe extern "C" fn clear(context: *mut c_void) {
    let cache = unsafe { &*context.cast::<Cache>() };
    for entry in &cache.0 { entry.set(EMPTY); }
}

#[cfg(test)]
mod tests;
