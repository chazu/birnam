//! Native ownership of derived positions. Id retains every dictionary decision.
use birnam_abi::key_index::FactoryV1;
use core::{
    ffi::{c_int, c_void},
    hash::{BuildHasherDefault, Hasher},
};
use std::collections::HashMap;

// Exact existing Id address hasher; registry/Heap maps remain there.
#[derive(Default)]
pub(crate) struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Fallback for completeness; usize keys use write_usize below.
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }

    fn write_usize(&mut self, address: usize) {
        self.0 = ((address >> 3) as u64).wrapping_mul(0x9e3779b97f4a7c15);
    }
}

type Index = HashMap<usize, usize, BuildHasherDefault<AddressHasher>>;

unsafe extern "C" fn destroy(context: *mut c_void) {
    if !context.is_null() {
        unsafe { drop(Box::from_raw(context.cast::<Index>())) };
    }
}
unsafe extern "C" fn is_empty(context: *const c_void) -> c_int {
    c_int::from(context.is_null() || unsafe { (&*context.cast::<Index>()).is_empty() })
}
unsafe extern "C" fn get(context: *const c_void, key: usize, output: *mut usize) -> c_int {
    if context.is_null() {
        return 0;
    }
    match unsafe { (&*context.cast::<Index>()).get(&key) } {
        Some(position) => {
            unsafe { output.write(*position) };
            1
        }
        None => 0,
    }
}
unsafe extern "C" fn insert(slot: *mut *mut c_void, key: usize, position: usize) {
    // A synchronous mutable loan, never retained across record movement.
    let context = unsafe { &mut *slot };
    if context.is_null() {
        *context = Box::into_raw(Box::new(Index::default())).cast();
    }
    unsafe { (&mut *context.cast::<Index>()).insert(key, position) };
}
unsafe extern "C" fn clear(context: *mut c_void) {
    if !context.is_null() {
        unsafe { (&mut *context.cast::<Index>()).clear() };
    }
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1,
    byte_size: size_of::<FactoryV1>(),
    destroy: Some(destroy),
    is_empty: Some(is_empty),
    get: Some(get),
    insert: Some(insert),
    clear: Some(clear),
};
/// # Safety
/// Pin this descriptor and its code for every index lifetime. No semantic startup is needed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_key_index_factory_v1() -> *const FactoryV1 {
    &FACTORY
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lazy_all_bits_misses_overwrites_and_retained_capacity() {
        unsafe {
            let mut context = core::ptr::null_mut();
            let mut output = 91;
            assert_eq!(is_empty(context), 1);
            assert_eq!(get(context, 0, &mut output), 0);
            assert_eq!(output, 91);
            clear(context);
            destroy(context);
            assert!(context.is_null());
            let mut keys = vec![0, usize::MAX];
            keys.extend((0..usize::BITS).flat_map(|i| [1usize << i, !(1usize << i)]));
            for &key in &keys {
                insert(&mut context, key, !key);
            }
            assert!(!context.is_null());
            assert_eq!(is_empty(context), 0);
            for &key in &keys {
                assert_eq!(get(context, key, &mut output), 1);
                assert_eq!(output, !key);
            }
            insert(&mut context, 0, usize::MAX);
            assert_eq!(get(context, 0, &mut output), 1);
            assert_eq!(output, usize::MAX);
            output = 91;
            assert_eq!(get(context, 19, &mut output), 0);
            assert_eq!(output, 91);
            let retained = context;
            let capacity = (&*context.cast::<Index>()).capacity();
            clear(context);
            assert_eq!(context, retained);
            assert_eq!((&*context.cast::<Index>()).capacity(), capacity);
            assert_eq!(is_empty(context), 1);
            assert_eq!(get(context, 0, &mut output), 0);
            assert_eq!(output, 91);
            for key in 0..17 {
                insert(&mut context, key, key);
            }
            assert_eq!(context, retained);
            assert_eq!((&*context.cast::<Index>()).capacity(), capacity);
            destroy(context);
        }
    }
    #[test]
    fn moved_slots_and_independent_owners() {
        unsafe {
            let mut contexts = vec![core::ptr::null_mut(); 2];
            insert(&mut contexts[0], 7, usize::MAX);
            insert(&mut contexts[1], 7, 0);
            assert_ne!(contexts[0], contexts[1]);
            let mut moved = contexts.remove(0);
            insert(&mut moved, 19, 2);
            destroy(contexts[0]);
            let mut output = 0;
            assert_eq!(get(moved, 7, &mut output), 1);
            assert_eq!(output, usize::MAX);
            assert_eq!(get(moved, 19, &mut output), 1);
            assert_eq!(output, 2);
            destroy(moved);
        }
    }
    #[test]
    fn frozen_address_and_byte_hashes() {
        for (key, expected) in [
            (0, 0),
            (7, 0),
            (8, 0x9e3779b97f4a7c15),
            (usize::MAX, 0x01c8864680b583eb),
        ] {
            let mut hash = AddressHasher::default();
            hash.write_usize(key);
            assert_eq!(hash.finish(), expected);
        }
        let mut hash = AddressHasher::default();
        hash.write(b"abc");
        assert_eq!(hash.finish(), 0xe71fa2190541574b);
    }
}
