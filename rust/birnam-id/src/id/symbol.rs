//! Weakly canonicalized Symbol objects and byte-oriented interning.
#![allow(dead_code)] // R2.3 precedes bootstrap/export wiring.

use core::cell::Cell;
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

    fn c_name(&self) -> *const c_char {
        self.nul_terminated_bytes.as_ptr().cast()
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

// Private admission queries repeatedly use these byte names. Digests belong
// to this Symbols instance's immutable hasher, not to a Symbol's lifetime.
const QUERY_NAMES: [&[u8]; 11] = [
    b"tryRepresentationIdentityOf:behavior:bytes:",
    b"behaviorOf:",
    b"allocationSizeOf:",
    b"identityOf:",
    b"tryRepresentationsOf:behavior:bytes:",
    b"allocate:bytes:",
    b"allocationChargeFor:",
    b"isBehavior:",
    b"behaviorEpoch",
    b"bind:to:",
    b"usesDefaultLookup:",
];

/// Ordered authority for interned Symbol identities.
pub(crate) struct Symbols {
    ordered: Vec<SymbolRecord>,
    by_address: AddressMap<usize>,
    // Derived weak index: owns no names or Oops. Full byte equality resolves
    // hash collisions; ordered remains the enumeration and name authority.
    by_name_hash: NameDigestIndex,
    name_hasher: RandomState,
    query_digests: [u64; QUERY_NAMES.len()],
    // Positions only: no Symbol is retained and no absence is memoized. A hit
    // must match the current ordered record's digest and complete name bytes.
    query_positions: [Cell<usize>; QUERY_NAMES.len()],
    owned_name_bytes: usize,
}

impl Symbols {
    pub(crate) fn new() -> Self {
        let name_hasher = RandomState::new();
        let query_digests = QUERY_NAMES.map(|name| name_hasher.hash_one(name));
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
            by_name_hash: NameDigestIndex::default(),
            name_hasher,
            query_digests,
            query_positions: std::array::from_fn(|_| Cell::new(usize::MAX)),
            owned_name_bytes: 0,
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
        let query = match bytes {
            b"tryRepresentationIdentityOf:behavior:bytes:" => 0,
            b"behaviorOf:" => 1,
            b"allocationSizeOf:" => 2,
            b"identityOf:" => 3,
            b"tryRepresentationsOf:behavior:bytes:" => 4,
            b"allocate:bytes:" => 5,
            b"allocationChargeFor:" => 6,
            b"isBehavior:" => 7,
            b"behaviorEpoch" => 8,
            b"bind:to:" => 9,
            b"usesDefaultLookup:" => 10,
            _ => return self.find_hashed(bytes, self.name_hasher.hash_one(bytes)),
        };
        let hash = self.query_digests[query];
        if let Some(record) = self.ordered.get(self.query_positions[query].get()) {
            if record.name_hash == hash && record.bytes() == bytes {
                return Some(record.object);
            }
        }
        let index = self.find_hashed_index(bytes, hash);
        self.query_positions[query].set(index.unwrap_or(usize::MAX));
        index.map(|index| self.ordered[index].object)
    }

    #[cfg(test)]
    pub(crate) fn find_hashed_for_test(&self, bytes: &[u8], hash: u64) -> Option<Oop> {
        self.find_hashed(bytes, hash)
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
        self.owned_name_bytes = self
            .owned_name_bytes
            .checked_add(bytes.len().saturating_add(1))
            .unwrap_or_else(|| boundary::fatal_invariant("Symbol name byte counter overflow"));
        let previous = self.by_address.insert(object.addr(), index);
        boundary::require_invariant(previous.is_none(), "duplicate Symbol object address");
        object
    }

    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }

    pub(crate) fn owned_name_bytes(&self) -> usize {
        self.owned_name_bytes
    }

    pub(crate) fn contains_bytes(&self, bytes: &[u8]) -> bool {
        self.find_hashed(bytes, self.name_hasher.hash_one(bytes))
            .is_some()
    }

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

    pub(crate) fn c_name_of(&self, candidate: Oop) -> *const c_char {
        let Some(index) = self.by_address.get(&candidate.addr()) else {
            return core::ptr::null();
        };
        self.ordered[*index].c_name()
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
        self.owned_name_bytes = self
            .ordered
            .iter()
            .map(|record| record.nul_terminated_bytes.len())
            .sum();
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
        assert_eq!(symbols.owned_name_bytes(), names.iter().enumerate()
            .filter(|(i, _)| !dead.contains_key(&objects[*i].addr()) || *i == 17)
            .map(|(_, name)| name.len() + 1).sum());
    }

    #[test]
    fn fixed_query_digests_follow_each_inventory_through_retirement_and_reinterning() {
        // Each instance has its own hash seed. Missing queries must neither
        // intern names nor retain objects, including after a formerly live hit.
        for _ in 0..3 {
            let mut heap = ObjectMemory::new();
            let behavior = heap.allocate(0);
            let mut symbols = Symbols::new();
            for name in QUERY_NAMES { assert_eq!(symbols.existing(name), None); }
            assert_eq!(symbols.count(), 0);
            assert_eq!(symbols.owned_name_bytes(), 0);
            for name in QUERY_NAMES {
                let name = name.to_vec();
                let first = symbols.intern(&mut heap, behavior, &name).unwrap();
                assert_eq!(symbols.existing(&name), Some(first));
                // A different name of the same length must use the fallback,
                // rather than another name's cached digest or Symbol identity.
                let mut neighbor = name.clone(); neighbor[0] = b'!';
                let other = symbols.intern(&mut heap, behavior, &neighbor).unwrap();
                assert_eq!(symbols.existing(&neighbor), Some(other));
                // Arbitrary stale hints, including a live wrong name and an
                // out-of-range slot, cannot substitute for current membership.
                let query = QUERY_NAMES.iter().position(|q| *q == name.as_slice()).unwrap();
                for hint in [symbols.ordered.len()-1, usize::MAX, 0] {
                    symbols.query_positions[query].set(hint);
                    assert_eq!(symbols.existing(&name), Some(first));
                }
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
    fn query_position_hint_rejects_a_same_digest_wrong_name() {
        let mut heap = ObjectMemory::new();
        let behavior = heap.allocate(0);
        let mut symbols = Symbols::new();
        let query = 1;
        let name = QUERY_NAMES[query];
        let target = symbols.intern(&mut heap, behavior, name).unwrap();
        let digest = symbols.query_digests[query];
        symbols.intern_hashed(&mut heap, behavior, b"different-query", digest);
        symbols.query_positions[query].set(1);
        assert_eq!(symbols.existing(name), Some(target));
        symbols.remove_objects(&[(target.addr(), ())].into_iter().collect());
        symbols.query_positions[query].set(0);
        assert_eq!(symbols.existing(name), None);
        let replacement = symbols.intern(&mut heap, behavior, name).unwrap();
        assert_ne!(replacement, target);
        assert_eq!(symbols.existing(name), Some(replacement));
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
