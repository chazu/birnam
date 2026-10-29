//! Weakly canonicalized Symbol objects and byte-oriented interning.

use core::ffi::c_char;
use core::mem::offset_of;
use core::ptr::NonNull;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, BuildHasherDefault, Hasher};

use crate::abi::Oop;
use crate::boundary;
use crate::id::object_memory::ObjectMemory;
use crate::raw::AddressMap;

#[repr(C)]
struct RawSymbol {
    string: *mut c_char,
}

const _: () = {
    assert!(size_of::<RawSymbol>() == 8);
    assert!(align_of::<RawSymbol>() == 8);
    assert!(offset_of!(RawSymbol, string) == 0);
};

struct SymbolRecord {
    object: Oop,
    nul_terminated_bytes: Box<[u8]>,
    name_hash: u64,
    next_same_hash: Option<usize>,
}

impl SymbolRecord {
    fn bytes(&self) -> &[u8] {
        &self.nul_terminated_bytes[..self.nul_terminated_bytes.len() - 1]
    }

}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EmbeddedNul;

// This index receives only completed randomized name digests. Hashing that
// u64 again adds no collision protection: full bytes still distinguish names
// on the same digest chain. Keep the original name hasher and ordered authority.
#[derive(Default)]
struct NameDigestHasher(u64);
impl Hasher for NameDigestHasher {
    fn finish(&self) -> u64 { self.0 }
    fn write_u64(&mut self, digest: u64) { self.0 = digest; }
    fn write(&mut self, bytes: &[u8]) {
        self.0 = u64::from_ne_bytes(bytes.try_into().expect("name digest must be one u64"));
    }
}
type NameDigestIndex = HashMap<u64, usize, BuildHasherDefault<NameDigestHasher>>;

/// Ordered authority for interned Symbol identities.
pub(crate) struct Symbols {
    ordered: Vec<SymbolRecord>,
    by_address: AddressMap<usize>,
    // Derived weak index: owns no names or Oops. Full byte equality resolves
    // hash collisions; ordered remains the enumeration and name authority.
    by_name_hash: NameDigestIndex,
    name_hasher: RandomState,
}

impl Symbols {
    pub(crate) fn new() -> Self {
        let name_hasher = RandomState::new();
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
            by_name_hash: NameDigestIndex::default(),
            name_hasher,
        }
    }

    pub(crate) fn intern(
        &mut self,
        heap: &mut ObjectMemory,
        symbol_vtable: Oop,
        bytes: &[u8],
    ) -> Result<Oop, EmbeddedNul> {
        if bytes.contains(&0) {
            return Err(EmbeddedNul);
        }
        let hash = self.name_hasher.hash_one(bytes);
        Ok(self.intern_hashed(heap, symbol_vtable, bytes, hash))
    }

    /// Non-allocating identity query for a guarded native region. A missing
    /// symbol requires ordinary interning at its baseline observation boundary.
    pub(crate) fn existing(&self, bytes: &[u8]) -> Option<Oop> {
        self.find_hashed(bytes, self.name_hasher.hash_one(bytes))
    }

    #[cfg(test)]
    pub(crate) fn contains_bytes_linearly_for_test(&self, bytes: &[u8]) -> bool {
        self.ordered.iter().any(|record| record.bytes() == bytes)
    }

    fn find_hashed(&self, bytes: &[u8], hash: u64) -> Option<Oop> {
        self.find_hashed_index(bytes, hash).map(|index| self.ordered[index].object)
    }

    fn find_hashed_index(&self, bytes: &[u8], hash: u64) -> Option<usize> {
        let mut index = self.by_name_hash.get(&hash).copied();
        while let Some(current) = index {
            let record = &self.ordered[current];
            if record.bytes() == bytes {
                return Some(current);
            }
            index = record.next_same_hash;
        }
        None
    }

    // The caller validates NUL and hashes once. Keeping the supplied hash also
    // permits deterministic collision coverage without weakening production hashing.
    pub(crate) fn intern_hashed(
        &mut self,
        heap: &mut ObjectMemory,
        symbol_vtable: Oop,
        bytes: &[u8],
        hash: u64,
    ) -> Oop {
        if let Some(existing) = self.find_hashed(bytes, hash) {
            return existing;
        }

        let mut owned_name = Vec::with_capacity(bytes.len() + 1);
        owned_name.extend_from_slice(bytes);
        owned_name.push(0);
        let mut owned_name = owned_name.into_boxed_slice();
        let name_pointer = owned_name.as_mut_ptr().cast::<c_char>();

        let object = heap.allocate(size_of::<RawSymbol>());
        boundary::require_invariant(
            heap.set_vtable(object, symbol_vtable),
            "new Symbol block was not registered",
        );
        let raw = NonNull::new(object.cast::<RawSymbol>())
            .unwrap_or_else(|| boundary::fatal_invariant("new Symbol payload is null"));
        // SAFETY: object owns an aligned payload of exactly RawSymbol's size,
        // and name_pointer targets the boxed bytes retained by the record.
        unsafe {
            raw.as_ptr().write(RawSymbol {
                string: name_pointer,
            })
        };

        let index = self.ordered.len();
        self.ordered.push(SymbolRecord {
            object,
            nul_terminated_bytes: owned_name,
            name_hash: hash,
            next_same_hash: self.by_name_hash.insert(hash, index),
        });
        let previous = self.by_address.insert(object.addr(), index);
        boundary::require_invariant(previous.is_none(), "duplicate Symbol object address");
        object
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }

    #[cfg(test)]
    pub(crate) fn contains_bytes(&self, bytes: &[u8]) -> bool {
        self.find_hashed(bytes, self.name_hasher.hash_one(bytes))
            .is_some()
    }

    #[cfg(test)]
    pub(crate) fn at(&self, index: usize) -> Oop {
        self.ordered
            .get(index)
            .map(|record| record.object)
            .unwrap_or(core::ptr::null_mut())
    }

    pub(crate) fn bytes_of(&self, candidate: Oop) -> Option<&[u8]> {
        let index = *self.by_address.get(&candidate.addr())?;
        Some(self.ordered[index].bytes())
    }

    /// Retire names chosen by the caller before their allocations are released.
    pub(crate) fn remove_objects(&mut self, dead: &AddressMap<()>) -> usize {
        let before = self.ordered.len();
        self.by_name_hash.clear();
        self.by_address.clear();
        self.ordered.retain(|record| {
            !dead.contains_key(&record.object.addr())
        });
        for (index, record) in self.ordered.iter_mut().enumerate() {
            record.next_same_hash = self.by_name_hash.insert(record.name_hash, index);
            let previous = self.by_address.insert(record.object.addr(), index);
            boundary::require_invariant(previous.is_none(), "duplicate live Symbol address");
        }
        before.saturating_sub(self.ordered.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_collisions_and_retirement_preserve_canonical_names_and_order() {
        let mut heap = ObjectMemory::new();
        let behavior = heap.allocate(0);
        let mut symbols = Symbols::new();
        // Equal digests exercise byte comparison, even with head/middle/tail
        // removal. Distinct digests sharing bucket bits exercise index equality.
        let names: Vec<_> = (0..96).map(|i| format!("selector:{i}").into_bytes()).collect();
        let digest = |i: usize| if i < 48 { 0 } else { ((i as u64) << 48) | 7 };
        let objects: Vec<_> = names.iter().enumerate().map(|(i, name)|
            symbols.intern_hashed(&mut heap, behavior, name, digest(i))).collect();
        for (i, name) in names.iter().enumerate() {
            assert_eq!(symbols.find_hashed(name, digest(i)), Some(objects[i]));
            assert_eq!(symbols.intern_hashed(&mut heap, behavior, name, digest(i)), objects[i]);
            assert_eq!(symbols.at(i), objects[i]);
        }
        assert_eq!(symbols.find_hashed(b"absent", 0), None);
        let dead: AddressMap<_> = [0, 17, 47, 48, 95].into_iter().map(|i| (objects[i].addr(), ())).collect();
        assert_eq!(symbols.remove_objects(&dead), dead.len());
        let mut live = 0;
        for (i, name) in names.iter().enumerate() {
            if dead.contains_key(&objects[i].addr()) {
                assert_eq!(symbols.find_hashed(name, digest(i)), None);
                assert_eq!(symbols.bytes_of(objects[i]), None);
            } else {
                assert_eq!(symbols.find_hashed(name, digest(i)), Some(objects[i]));
                assert_eq!(symbols.at(live), objects[i]);
                live += 1;
            }
        }
        let again = symbols.intern_hashed(&mut heap, behavior, &names[17], digest(17));
        assert_ne!(again, objects[17]);
        assert_eq!(symbols.at(live), again);
        assert_eq!(symbols.find_hashed(&names[17], digest(17)), Some(again));
    }

    #[test]
    fn existing_names_follow_retirement_without_interning_absent_queries() {
        const NAMES: [&[u8]; 3] = [b"first", b"ordinaryName:", b"anotherName"];
        // Each instance has its own hash seed. Missing queries must neither
        // intern names nor retain objects, including after a formerly live hit.
        for _ in 0..3 {
            let mut heap = ObjectMemory::new();
            let behavior = heap.allocate(0);
            let mut symbols = Symbols::new();
            for name in NAMES { assert_eq!(symbols.existing(name), None); }
            assert_eq!(symbols.count(), 0);
            for name in NAMES {
                let name = name.to_vec();
                let first = symbols.intern(&mut heap, behavior, &name).unwrap();
                assert_eq!(symbols.existing(&name), Some(first));
                // Equal-length names must retain independent identities.
                let mut neighbor = name.clone(); neighbor[0] = b'!';
                let other = symbols.intern(&mut heap, behavior, &neighbor).unwrap();
                assert_eq!(symbols.existing(&neighbor), Some(other));
                assert_eq!(symbols.remove_objects(&[(first.addr(), ())].into_iter().collect()), 1);
                assert_eq!(symbols.existing(&name), None);
                assert_eq!(symbols.existing(&neighbor), Some(other));
                let second = symbols.intern(&mut heap, behavior, &name).unwrap();
                assert_ne!(first, second);
                assert_eq!(symbols.existing(&name), Some(second));
            }
        }
    }

    #[test]
    fn randomized_names_still_use_full_bytes_and_reject_embedded_nul() {
        let mut heap = ObjectMemory::new();
        let behavior = heap.allocate(0);
        let mut symbols = Symbols::new();
        for name in [b"".as_slice(), b"a", b"aa", b"at:", b"at:put:", "λ".as_bytes()] {
            let object = symbols.intern(&mut heap, behavior, name).unwrap();
            assert_eq!(symbols.intern(&mut heap, behavior, &name.to_vec()).unwrap(), object);
            assert_eq!(symbols.existing(name), Some(object));
            assert_eq!(symbols.bytes_of(object), Some(name));
            assert_eq!(symbols.contains_bytes(name), symbols.contains_bytes_linearly_for_test(name));
        }
        let count = symbols.count();
        assert_eq!(symbols.intern(&mut heap, behavior, b"a\0b"), Err(EmbeddedNul));
        assert_eq!(symbols.count(), count);
        assert_eq!(symbols.existing(b"missing"), None);
    }
}
