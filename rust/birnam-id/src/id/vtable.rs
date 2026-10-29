//! Vtable identities and their owned ordered method dictionaries.

use core::cell::Cell;

use crate::abi::Oop;

use crate::boundary;
use crate::id::object_memory::ObjectMemory;
use crate::raw::AddressMap;

pub(crate) struct VtableRecord {
    pub(crate) keys: Vec<Oop>,
    pub(crate) values: Vec<Oop>,
    // Derived identity lookup only. Ordered vectors retain lookup and GC
    // authority; these integer addresses never add roots or cache dispatch.
    pub(crate) key_indices: AddressMap<usize>,
    pub(crate) parent: Oop,
}

/// Dictionaries at or below this size answer lookups by scanning the ordered
/// keys; the derived index exists only for larger dictionaries. Most vtables
/// hold a handful of methods, and allocating and growing a hash table for each
/// of them measured +4.5 ms on every stdlib boot (epic as7 acceptance repeat,
/// 2026-09-09) without any lookup benefit at that size.
const KEY_INDEX_THRESHOLD: usize = 16;

impl VtableRecord {
    // Invariant: `key_indices` is either empty (scan the ordered keys) or a
    // complete map of every key to its position. It is never partially stale.
    pub(crate) fn key_index(&self, key: Oop) -> Option<usize> {
        if self.key_indices.is_empty() {
            return self.keys.iter().position(|candidate| *candidate == key);
        }
        self.key_indices.get(&key.addr()).copied()
    }

    fn rebuild_key_indices(&mut self) {
        self.key_indices.clear();
        if self.keys.len() > KEY_INDEX_THRESHOLD {
            for (index, key) in self.keys.iter().enumerate() {
                self.key_indices.insert(key.addr(), index);
            }
        }
    }

    /// Record that a key was just pushed at the end of `keys`.
    fn index_appended_key(&mut self) {
        if !self.key_indices.is_empty() {
            let last = self.keys.len() - 1;
            self.key_indices.insert(self.keys[last].addr(), last);
        } else if self.keys.len() > KEY_INDEX_THRESHOLD {
            self.rebuild_key_indices();
        }
    }

}

// Position hints contain no Objects, selected methods, contexts or parent links.
// Any hint is safe only after checking the current ordered key at that position.
const SLOT_HINTS: usize = 256;

/// Ordered registry of every vtable in one runtime.
pub(crate) struct Vtables {
    pub(crate) ordered: Vec<Oop>,
    by_address: AddressMap<VtableRecord>,
    // Monotonic count of dictionary, parent and registry mutations. Design 74A
    // §3.2: a reader that cached a verification against one epoch re-verifies
    // when the epoch moved. Over-counting is harmless; never skip a mutator.
    mutation_epoch: u64,
    // Explicit binding/parent/association changes, including ABA. Retiring an
    // unreachable behavior cannot change a live lookup and does not advance it.
    binding_epoch: u64,
    slot_hints: [Cell<Option<usize>>; SLOT_HINTS],
    // Only a positive fact about Id's own lookup primitive, never a selected
    // source method. Addresses identify the checked metabehavior, selector and
    // registry context; they retain no objects. Every existing-graph mutation
    // clears this fact, including mutations after epoch counters saturate.
    default_lookup: [Cell<Option<(usize, usize, usize, bool)>>; 2],
}

impl Vtables {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
            mutation_epoch: 0,
            binding_epoch: 0,
            slot_hints: std::array::from_fn(|_| Cell::new(None)),
            default_lookup: std::array::from_fn(|_| Cell::new(None)),
        }
    }
    pub(crate) fn mutation_epoch(&self) -> u64 {
        self.mutation_epoch
    }

    pub(crate) fn note_mutation(&mut self) {
        for fact in &self.default_lookup { fact.set(None); }
        self.mutation_epoch = self.mutation_epoch.saturating_add(1);
        self.binding_epoch = self.binding_epoch.saturating_add(1);
    }
    pub(crate) fn binding_epoch(&self) -> u64 {
        self.binding_epoch
    }

    /// Construct a behavior using Id allocation alone.
    pub(crate) fn create(
        &mut self,
        memory: &mut ObjectMemory,
        parent: Oop,
        metavtable: Oop,
    ) -> Oop {
        let object = memory.allocate(0);
        assert!(memory.set_vtable(object, metavtable));
        self.register(object, parent);
        object
    }

    /// Register the identity of a fresh vtable. Its dictionary is owned here;
    /// the object needs no duplicate native pointer/capacity payload.
    fn register(&mut self, object: Oop, parent: Oop) {
        boundary::require_invariant(
            parent.is_null() || self.is_registered(parent),
            "vtable parent is not registered",
        );
        let previous = self.by_address.insert(
            object.addr(),
            VtableRecord {
                keys: Vec::new(),
                values: Vec::new(),
                key_indices: AddressMap::default(),
                parent,
            },
        );
        boundary::require_invariant(previous.is_none(), "duplicate vtable address");
        self.ordered.push(object);
    }
    pub(crate) fn is_registered(&self, candidate: Oop) -> bool {
        !candidate.is_null() && self.by_address.contains_key(&candidate.addr())
    }
    pub(crate) fn record(&self, vtable: Oop) -> &VtableRecord {
        self.by_address
            .get(&vtable.addr())
            .unwrap_or_else(|| boundary::fatal_invariant("vtable is not registered"))
    }
    pub(crate) fn record_mut(&mut self, vtable: Oop) -> &mut VtableRecord {
        self.by_address
            .get_mut(&vtable.addr())
            .unwrap_or_else(|| boundary::fatal_invariant("vtable is not registered"))
    }
    pub(crate) fn parent_of(&self, vtable: Oop) -> Oop {
        self.by_address
            .get(&vtable.addr())
            .map(|record| record.parent)
            .unwrap_or(core::ptr::null_mut())
    }
    pub(crate) fn set_parent(&mut self, vtable: Oop, parent: Oop) {
        boundary::require_invariant(
            parent.is_null() || self.is_registered(parent),
            "vtable parent is not registered",
        );
        let record = self.record_mut(vtable);
        record.parent = parent;
        self.note_mutation();
    }
    /// Owner-local proof, valid until the next dictionary/graph mutation. The
    /// receiver's current metabehavior is still checked by each dispatch, and
    /// its requested method is still selected from the current dictionaries.
    #[inline]
    fn default_lookup_fact(&self, meta: Oop, selector: Oop, expected: Oop) -> Option<bool> {
        let key = (meta.addr(), selector.addr(), expected.addr());
        for fact in &self.default_lookup {
            if let Some((m, s, e, root)) = fact.get()
                && (m, s, e) == key { return Some(root); }
        }
        if !self.is_registered(meta) || self.lookup(meta, selector) != Some(expected) {
            return None;
        }
        // This owner controls every vtable/parent/binding mutation. The root
        // relation can share the same explicitly invalidated fact as lookup.
        let root = unsafe { ObjectMemory::vtable_of_valid_object(meta) } == meta;
        self.default_lookup.iter().find(|fact| fact.get().is_none())
            .unwrap_or(&self.default_lookup[0]).set(Some((key.0, key.1, key.2, root)));
        Some(root)
    }
    #[inline]
    pub(crate) fn has_default_root_lookup(&self, meta: Oop, selector: Oop, expected: Oop) -> bool {
        self.default_lookup_fact(meta, selector, expected) == Some(true)
    }
    // Expose this callback-free walk to its callers so the optimizer can
    // eliminate helper call/return work. Values and parents are still read live.
    #[inline(always)]
    pub(crate) fn lookup(&self, mut vtable: Oop, selector: Oop) -> Option<Oop> {
        while !vtable.is_null() {
            // Membership is still checked before consulting a hint. Retirement,
            // address reuse and slot shifts cannot make an old index authority.
            let record = self.record(vtable);
            // Tiny dictionaries already use a cheaper linear scan. Avoid adding
            // hint hashing to fresh one-method Block behaviors and empty parents.
            let hint = if record.key_indices.is_empty() {
                None
            } else {
                let hash = (vtable.addr().rotate_left(13) ^ selector.addr())
                    .wrapping_mul(0x9e3779b97f4a7c15);
                Some(&self.slot_hints[(hash >> (usize::BITS - 8)) & (SLOT_HINTS - 1)])
            };
            if let Some(slot) = hint
                && let Some(index) = slot.get()
                && record.keys.get(index) == Some(&selector)
            {
                return Some(record.values[index]);
            }
            if let Some(index) = record.key_index(selector) {
                if let Some(slot) = hint {
                    slot.set(Some(index));
                }
                return Some(record.values[index]);
            }
            // Never memoize inherited resolution or absence: read this live link.
            vtable = record.parent;
        }
        None
    }
    pub(crate) fn add_method(&mut self, vtable: Oop, selector: Oop, method: Oop) {
        assert!(!method.is_null(), "method binding requires a Method object");
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(selector) {
            record.values[index] = method;
            return;
        }
        record.keys.push(selector);
        record.values.push(method);
        record.index_appended_key();
    }
    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<Oop> {
        self.note_mutation();
        let record = self.record_mut(vtable);
        let index = record.key_index(selector)?;
        record.keys.remove(index);
        let removed = record.values.remove(index);
        record.rebuild_key_indices();
        Some(removed)
    }
    /// Retire dictionary records before the caller releases their allocations.
    pub(crate) fn remove_objects(&mut self, dead: &[Oop]) {
        for vtable in dead {
            self.by_address.remove(&vtable.addr()).unwrap_or_else(|| {
                boundary::fatal_invariant("dead vtable registry record disappeared")
            });
        }
        self.ordered
            .retain(|vtable| self.by_address.contains_key(&vtable.addr()));
        if !dead.is_empty() {
            for fact in &self.default_lookup { fact.set(None); }
            self.mutation_epoch = self.mutation_epoch.saturating_add(1);
        }
    }

}

#[cfg(test)]
mod core_tests {
    use super::*;
    #[test]
    fn default_lookup_fact_tracks_the_current_root_relation() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let nil = core::ptr::null_mut();
        let meta = tables.create(&mut memory, nil, nil);
        let parent = tables.create(&mut memory, nil, nil);
        let selector = memory.allocate(0);
        let method = memory.allocate(0);
        memory.set_vtable(meta, meta);
        tables.add_method(meta, selector, method);
        assert!(tables.has_default_root_lookup(meta, selector, method));
        memory.set_vtable(meta, parent);
        tables.note_mutation();
        assert!(tables.default_lookup_fact(meta, selector, method).is_some());
        assert!(!tables.has_default_root_lookup(meta, selector, method));
    }
    #[test]
    fn default_lookup_fact_tracks_context_inheritance_retirement_and_saturation() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        memory.set_vtable(meta, meta);
        let parent = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let key = memory.allocate(0);
        let expected = memory.allocate(0);
        tables.add_method(meta, key, expected);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        // A distinct Method object is not the canonical lookup Method.
        tables.add_method(meta, key, memory.allocate(0));
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(meta, key, expected);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        tables.remove_method(meta, key);
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(parent, key, expected);
        tables.set_parent(meta, parent);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        tables.set_parent(meta, core::ptr::null_mut());
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.set_parent(meta, parent);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(meta, key, parent);
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.remove_method(meta, key);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        // Positive facts must not survive registry retirement/address reuse.
        tables.remove_objects(&[meta]);
        tables.register(meta, core::ptr::null_mut());
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(meta, key, expected);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        // Invalidation is explicit, not an equality check on a saturated epoch.
        tables.mutation_epoch = u64::MAX;
        tables.binding_epoch = u64::MAX;
        tables.remove_method(meta, key);
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(meta, key, expected);
        assert!(tables.default_lookup_fact(meta, key, expected).is_some());
        tables.add_method(meta, key, parent);
        assert!(!tables.default_lookup_fact(meta, key, expected).is_some());
    }

    #[test]
    fn slot_hints_follow_live_values_shifts_parents_and_reused_records() {
        fn reference(tables: &Vtables, mut at: Oop, key: Oop) -> Option<Oop> {
            while !at.is_null() {
                let row = tables.record(at);
                for (&candidate, &value) in row.keys.iter().zip(&row.values) {
                    if candidate == key {
                        return Some(value);
                    }
                }
                at = row.parent;
            }
            None
        }
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        memory.set_vtable(meta, meta);
        let parent = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let other = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let child = tables.create(&mut memory, parent, meta);
        let keys: Vec<_> = (0..40).map(|_| memory.allocate(0)).collect();
        let entry = |value: usize| core::ptr::without_provenance_mut(value << 3);
        for (i, &key) in keys.iter().enumerate() {
            tables.add_method(parent, key, entry(i + 1));
            tables.add_method(other, key, entry(i + 101));
        }
        for round in 0..80 {
            let key = keys[round % keys.len()];
            for &lookup in &keys {
                assert_eq!(
                    tables.lookup(child, lookup),
                    reference(&tables, child, lookup)
                );
            }
            match round % 4 {
                0 => {
                    tables.add_method(parent, key, entry(round + 501));
                }
                1 => {
                    tables.remove_method(parent, key);
                }
                2 => {
                    tables.set_parent(child, if round % 8 == 2 { other } else { parent });
                }
                _ => {
                    tables.add_method(child, key, entry(round + 901));
                }
            }
        }
        // A populated hint must observe replacement, removal and inheritance.
        assert_eq!(tables.lookup(other, keys[0]), Some(entry(101)));
        tables.add_method(other, keys[0], entry(102));
        assert_eq!(tables.lookup(other, keys[0]), Some(entry(102)));
        tables.remove_method(other, keys[0]);
        assert_eq!(tables.lookup(other, keys[0]), None);
        tables.add_method(other, keys[0], entry(101));
        tables.set_parent(child, other);
        tables.add_method(child, keys[0], entry(201));
        assert_eq!(tables.lookup(child, keys[0]), Some(entry(201)));
        tables.remove_method(child, keys[0]);
        assert_eq!(tables.lookup(child, keys[0]), Some(entry(101)));
        // Re-register the same still-allocated payload after retiring its record.
        // Hints do not keep a record or its previous entries alive.
        tables.remove_objects(&[child]);
        tables.register(child, parent);
        tables.add_method(child, keys[0], entry(9999));
        assert_eq!(tables.lookup(child, keys[0]), Some(entry(9999)));
        for &key in &keys {
            assert_eq!(tables.lookup(child, key), reference(&tables, child, key));
        }
        // Force stale/out-of-range entries and cache collisions independently
        // of the hash, then check fallback against a linear reference.
        for hint in &tables.slot_hints {
            hint.set(Some(usize::MAX));
        }
        assert_eq!(tables.lookup(child, keys[0]), Some(entry(9999)));
        for &key in &keys {
            assert_eq!(tables.lookup(other, key), reference(&tables, other, key));
        }
        for &key in &keys {
            assert_eq!(tables.lookup(child, key), reference(&tables, child, key));
        }
    }

    #[test]
    fn binding_epoch_detects_aba_but_ignores_dead_behavior_retirement() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        memory.set_vtable(meta, meta);
        let live = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let dead = tables.create(&mut memory, live, meta);
        let survivor = tables.create(&mut memory, live, meta);
        let before = tables.binding_epoch();
        let global = tables.mutation_epoch();
        tables.remove_objects(&[dead]);
        assert_eq!(tables.binding_epoch(), before);
        assert_eq!(tables.mutation_epoch(), global + 1);
        assert_eq!(tables.ordered, vec![meta, live, survivor]);
        tables.set_parent(survivor, meta);
        tables.set_parent(survivor, live);
        assert_eq!(tables.parent_of(survivor), live);
        assert_eq!(
            tables.binding_epoch(),
            before + 2,
            "ABA must invalidate snapshots"
        );
        let key = memory.allocate(0);
        let entry = memory.allocate(0);
        tables.add_method(live, key, entry);
        let bound = tables.binding_epoch();
        tables.remove_method(live, key);
        tables.add_method(live, key, entry);
        assert_eq!(tables.lookup(live, key), Some(entry));
        assert_eq!(tables.binding_epoch(), bound + 2);
    }
    #[test]
    fn installing_on_a_fresh_behavior_still_advances_binding_epochs() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        memory.set_vtable(meta, meta);
        let key = memory.allocate(0);
        let entry = memory.allocate(0);
        let before = (tables.binding_epoch(), tables.mutation_epoch());
        let fresh = tables.create(&mut memory, meta, meta);
        tables.add_method(fresh, key, entry);
        assert_eq!(tables.lookup(fresh, key), Some(entry));
        assert_eq!(tables.parent_of(fresh), meta);
        assert_eq!((tables.binding_epoch(), tables.mutation_epoch()), (before.0 + 1, before.1 + 1));
        tables.remove_method(fresh, key);
        tables.add_method(fresh, key, entry);
        assert_eq!(tables.binding_epoch(), before.0 + 3);
        assert_eq!(tables.mutation_epoch(), before.1 + 3);
        assert_eq!(tables.lookup(fresh, key), Some(entry));
    }
    #[test]
    fn behavior_graph_and_dictionary_need_only_id_allocation() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(memory.set_vtable(meta, meta));
        let parent = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let child = tables.create(&mut memory, parent, meta);
        assert_eq!(
            memory
                .get(child.addr())
                .map(|allocation| allocation.requested_size()),
            Some(0)
        );
        let identity = memory.allocate(0);
        let mut symbols = crate::id::symbol::Symbols::new();
        let symbol_behavior = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let keys: Vec<_> = (0..24)
            .map(|index| {
                symbols
                    .intern(
                        &mut memory,
                        symbol_behavior,
                        format!("key-{index}").as_bytes(),
                    )
                    .unwrap()
            })
            .collect();
        assert_eq!(
            symbols
                .intern(&mut memory, symbol_behavior, b"key-7")
                .unwrap(),
            keys[7]
        );
        for &key in &keys {
            tables.add_method(parent, key, identity);
        }
        for &key in &keys {
            assert_eq!(tables.lookup(child, key), Some(identity));
        }
        tables.remove_method(parent, keys[7]);
        assert!(tables.lookup(child, keys[7]).is_none());
        tables.remove_objects(&[child]);
        assert!(!tables.is_registered(child));
        assert!(memory.detach(child.addr()).is_some());
        assert_eq!(tables.ordered.len(), 3);
    }
}
