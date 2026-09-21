//! Weakly canonicalized Symbol objects and byte-oriented interning.
#![allow(dead_code)] // R2.3 precedes bootstrap/export wiring.

use core::ffi::c_char;
use core::mem::offset_of;
use core::ptr::NonNull;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::BuildHasher;

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

/// Ordered authority for interned Symbol identities.
pub(crate) struct Symbols {
    ordered: Vec<SymbolRecord>,
    by_address: AddressMap<usize>,
    // Derived weak index: owns no names or Oops. Full byte equality resolves
    // hash collisions; ordered remains the enumeration and name authority.
    by_name_hash: HashMap<u64, usize>,
    name_hasher: RandomState,
    owned_name_bytes: usize,
}

impl Symbols {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
            by_name_hash: HashMap::new(),
            name_hasher: RandomState::new(),
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
        self.find_hashed(bytes, self.name_hasher.hash_one(bytes))
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
        let mut index = self.by_name_hash.get(&hash).copied();
        while let Some(current) = index {
            let record = &self.ordered[current];
            if record.bytes() == bytes {
                return Some(record.object);
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
