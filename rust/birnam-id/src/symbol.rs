//! Weakly canonicalized Symbol objects and byte-oriented interning.
#![allow(dead_code)] // R2.3 precedes bootstrap/export wiring.

use core::ffi::c_char;
use core::mem::offset_of;
use core::ptr::NonNull;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::BuildHasher;

use crate::abi::Oop;
use crate::boundary;
use crate::heap::{Heap, Permanence};
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
        heap: &mut Heap,
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
    fn intern_hashed(
        &mut self,
        heap: &mut Heap,
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

        let object = heap.allocate(size_of::<RawSymbol>(), Permanence::Collectable);
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

    /// Drop weak interner records for collectable Symbols that did not survive
    /// marking. Name bytes are reclaimed here before the Heap detaches their
    /// RawSymbol payloads.
    pub(crate) fn remove_dead_collectable(&mut self, heap: &Heap) -> usize {
        let before = self.ordered.len();
        self.by_name_hash.clear();
        self.by_address.clear();
        self.ordered.retain(|record| {
            heap.permanence_of(record.object) != Some(Permanence::Collectable)
                || heap.is_marked(record.object) != Some(false)
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
    use crate::vtable::Vtables;
    use std::ffi::CStr;

    fn symbol_harness() -> (Heap, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Vtables::new();
        let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(heap.set_vtable(meta, meta));
        let symbol_vtable = vtables.create(&mut heap, core::ptr::null_mut(), meta);
        (heap, symbol_vtable)
    }

    #[test]
    fn repeated_equal_bytes_return_one_live_collectable_identity() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let first = symbols.intern(&mut heap, symbol_vtable, b"lookup").unwrap();
        let second = symbols.intern(&mut heap, symbol_vtable, b"lookup").unwrap();
        assert_eq!(first, second);
        assert_eq!(symbols.count(), 1);
        assert_eq!(symbols.at(0), first);
        assert!(symbols.at(1).is_null());
        assert_eq!(heap.permanence_of(first), Some(Permanence::Collectable));
        assert_eq!(Vtables::checked_vtable_of(&heap, first), symbol_vtable);
    }

    #[test]
    fn arbitrary_non_nul_bytes_never_require_utf8() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let cases: [&[u8]; 6] = [
            b"",
            b"ascii",
            b"doesNotUnderstand:",
            b"at:put:",
            b"!@#$%^&*",
            &[0xff, 0xfe, b':'],
        ];
        for bytes in cases {
            let symbol = symbols.intern(&mut heap, symbol_vtable, bytes).unwrap();
            assert_eq!(symbols.bytes_of(symbol), Some(bytes));
            // CStr validates only the terminating NUL; no UTF-8 conversion is
            // attempted for the non-UTF-8 selector.
            let c_name = symbols.c_name_of(symbol);
            // SAFETY: each record retains a stable NUL-terminated boxed name.
            assert_eq!(unsafe { CStr::from_ptr(c_name) }.to_bytes(), bytes);
        }
        assert_eq!(symbols.count(), cases.len());
        assert_eq!(
            symbols.intern(&mut heap, symbol_vtable, b"bad\0name"),
            Err(EmbeddedNul)
        );
    }

    #[test]
    fn enumeration_order_and_rooted_c_name_pointers_survive_unrelated_growth() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let anchor = symbols
            .intern(&mut heap, symbol_vtable, b"anchor:")
            .unwrap();
        let anchor_pointer = symbols.c_name_of(anchor);

        let mut expected = vec![anchor];
        let count = if cfg!(miri) { 128 } else { 4096 };
        for index in 0..count {
            let name = format!("symbol-{index}");
            let symbol = symbols
                .intern(&mut heap, symbol_vtable, name.as_bytes())
                .unwrap();
            expected.push(symbol);
            let _ = heap.allocate(index as usize & 31, Permanence::Collectable);
        }

        assert_eq!(symbols.count(), expected.len());
        for (index, expected_symbol) in expected.into_iter().enumerate() {
            assert_eq!(symbols.at(index), expected_symbol);
        }
        assert_eq!(symbols.c_name_of(anchor), anchor_pointer);
        // SAFETY: unrelated Vec/Heap growth cannot move the boxed name bytes.
        assert_eq!(
            unsafe { CStr::from_ptr(anchor_pointer) }.to_bytes(),
            b"anchor:"
        );
    }

    #[test]
    fn sweep_removes_dead_identity_and_reinterning_creates_a_fresh_oop() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let rooted = symbols.intern(&mut heap, symbol_vtable, b"rooted").unwrap();
        let first_dead = symbols
            .intern(&mut heap, symbol_vtable, b"ephemeral")
            .unwrap();
        assert_eq!(symbols.owned_name_bytes(), b"rooted\0ephemeral\0".len());

        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[rooted]);
        assert_eq!(symbols.remove_dead_collectable(&heap), 1);
        assert_eq!(symbols.count(), 1);
        assert_eq!(symbols.owned_name_bytes(), b"rooted\0".len());
        assert_eq!(symbols.at(0), rooted);
        assert_eq!(symbols.bytes_of(first_dead), None);
        assert!(symbols.c_name_of(first_dead).is_null());

        let second = symbols
            .intern(&mut heap, symbol_vtable, b"ephemeral")
            .unwrap();
        assert_ne!(second, first_dead);
        assert_eq!(symbols.bytes_of(second), Some(&b"ephemeral"[..]));
    }

    #[test]
    fn colliding_names_survive_middle_compaction_and_all_dead_cycles() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, vt) = symbol_harness();
        let mut symbols = Symbols::new();
        let first = symbols.intern_hashed(&mut heap, vt, b"first", 7);
        let middle = symbols.intern_hashed(&mut heap, vt, b"middle", 7);
        let last = symbols.intern_hashed(&mut heap, vt, b"last", 7);
        let middle_name = symbols.c_name_of(middle);
        assert_eq!(symbols.intern_hashed(&mut heap, vt, b"first", 7), first);
        assert_eq!(symbols.intern_hashed(&mut heap, vt, b"middle", 7), middle);
        assert_eq!(symbols.intern_hashed(&mut heap, vt, b"last", 7), last);
        assert_eq!(symbols.find_hashed(b"absent", 7), None);

        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[middle, last]);
        assert_eq!(symbols.remove_dead_collectable(&heap), 1);
        assert_eq!(symbols.at(0), middle);
        assert_eq!(symbols.at(1), last);
        assert_eq!(symbols.c_name_of(middle), middle_name);
        assert_eq!(symbols.find_hashed(b"first", 7), None);
        assert_eq!(symbols.find_hashed(b"middle", 7), Some(middle));
        assert_eq!(symbols.find_hashed(b"last", 7), Some(last));
        assert_eq!(symbols.owned_name_bytes(), b"middle\0last\0".len());

        let grown = symbols.intern_hashed(&mut heap, vt, b"grown", 7);
        assert_eq!(symbols.at(2), grown);
        assert_eq!(symbols.find_hashed(b"middle", 7), Some(middle));
        heap.clear_marks();
        assert_eq!(symbols.remove_dead_collectable(&heap), 3);
        assert_eq!(symbols.owned_name_bytes(), 0);
        assert_eq!(symbols.find_hashed(b"middle", 7), None);
        assert!(symbols.bytes_of(middle).is_none());
        let recreated = symbols.intern_hashed(&mut heap, vt, b"middle", 7);
        assert_ne!(recreated, middle);
        assert_eq!(symbols.at(0), recreated);
        assert_eq!(symbols.find_hashed(b"middle", 7), Some(recreated));
    }

    #[test]
    fn indexed_lookup_is_runtime_local_and_rebuilds_after_sweep() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, vt) = symbol_harness();
        let (mut other_heap, other_vt) = symbol_harness();
        let mut symbols = Symbols::new();
        let mut other = Symbols::new();
        let dead = symbols.intern(&mut heap, vt, b"dead").unwrap();
        let live = symbols.intern(&mut heap, vt, b"live").unwrap();
        let independent = other.intern(&mut other_heap, other_vt, b"live").unwrap();
        assert_ne!(live, independent);
        assert!(symbols.contains_bytes(b"dead"));
        assert!(!symbols.contains_bytes(b"live\0"));
        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[live]);
        assert_eq!(symbols.remove_dead_collectable(&heap), 1);
        assert!(!symbols.contains_bytes(b"dead"));
        assert!(symbols.contains_bytes(b"live"));
        assert_eq!(symbols.intern(&mut heap, vt, b"live").unwrap(), live);
        assert_eq!(
            other.intern(&mut other_heap, other_vt, b"live").unwrap(),
            independent
        );
        assert!(symbols.bytes_of(dead).is_none());
        let count = heap.allocation_count();
        for _ in 0..128 {
            assert_eq!(symbols.intern(&mut heap, vt, b"live").unwrap(), live);
        }
        assert_eq!(heap.allocation_count(), count);
    }

    #[test]
    #[ignore = "diagnostic benchmark; run with --release --ignored --nocapture"]
    fn symbol_lookup_scaling_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        for population in [1000, 10_000, 100_000] {
            let (mut heap, vt) = symbol_harness();
            let mut symbols = Symbols::new();
            for index in 0..population {
                symbols
                    .intern(&mut heap, vt, format!("name-{index:06}").as_bytes())
                    .unwrap();
            }
            let last = format!("name-{:06}", population - 1);
            for (kind, bytes) in [("hit", last.as_bytes()), ("miss", &b"name-absent"[..])] {
                for strategy in ["indexed", "linear"] {
                    let mut samples = [0.0f64; 3];
                    for sample in &mut samples {
                        let started = Instant::now();
                        for _ in 0..10_000 {
                            let result = if strategy == "indexed" {
                                symbols.contains_bytes(black_box(bytes))
                            } else {
                                symbols
                                    .ordered
                                    .iter()
                                    .any(|record| record.bytes() == black_box(bytes))
                            };
                            black_box(result);
                        }
                        *sample = started.elapsed().as_nanos() as f64 / 10_000.0;
                    }
                    samples.sort_by(f64::total_cmp);
                    println!(
                        "symbols={population} kind={kind} strategy={strategy} median_ns={:.2}",
                        samples[1]
                    );
                }
            }
        }
    }

    #[test]
    fn foreign_carriers_are_not_symbols() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let _ = symbols.intern(&mut heap, symbol_vtable, b"real").unwrap();
        let mut foreign = 0usize;
        let foreign: Oop = core::ptr::from_mut(&mut foreign).cast();
        assert_eq!(symbols.bytes_of(core::ptr::null_mut()), None);
        assert_eq!(symbols.bytes_of(foreign), None);
        assert!(symbols.c_name_of(foreign).is_null());
    }
}
