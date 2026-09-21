//! Id registry address index.
use core::hash::{BuildHasherDefault, Hasher};
use std::collections::HashMap;

/// Exact address hasher shared by heap and vtable membership registries.
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

pub(crate) type AddressMap<Value> = HashMap<usize, Value, BuildHasherDefault<AddressHasher>>;
