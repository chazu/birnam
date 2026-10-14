//! Private indexes of process-generated root tokens; never language Map keys.
//! Full usize equality remains HashMap's authority. Enumeration lives elsewhere.
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
pub(super) struct WordHasher(u64);
impl Hasher for WordHasher {
    fn finish(&self) -> u64 { self.0 }
    fn write_usize(&mut self, value: usize) {
        // Same avalanche as Id's private address index: distribute both aligned
        // low bits and high fingerprint bits. This is not a byte-string hasher.
        let mut x = value as u64;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.0 = x ^ (x >> 31);
    }
    fn write(&mut self, bytes: &[u8]) {
        // usize hashing calls write_usize. Keep the trait's byte entry total.
        for &byte in bytes { self.write_usize((self.0 ^ u64::from(byte)) as usize); }
    }
}
pub(super) type WordIndex<V> =
    std::collections::HashMap<usize, V, BuildHasherDefault<WordHasher>>;
