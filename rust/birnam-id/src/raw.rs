//! Private address indexes for Id-owned registries. Symbol names retain their
//! randomized byte hash; dictionary and Symbol enumeration keep ordered storage.
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
pub(crate) struct AddressHasher(u64);
impl Hasher for AddressHasher {
    fn finish(&self) -> u64 { self.0 }
    fn write_usize(&mut self, value: usize) {
        // Avalanche aligned pointer bits into both bucket and fingerprint bits.
        let mut x = value as u64;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.0 = x ^ (x >> 31);
    }
    fn write(&mut self, bytes: &[u8]) {
        // AddressMap keys use write_usize; retain a total byte implementation.
        for &byte in bytes { self.write_usize((self.0 ^ u64::from(byte)) as usize); }
    }
}
pub(crate) type AddressMap<V> =
    std::collections::HashMap<usize, V, BuildHasherDefault<AddressHasher>>;
