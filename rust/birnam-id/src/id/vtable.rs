//! Raw vtable objects, ordered method dictionaries, and exact membership.
#![allow(dead_code)] // Direct allocation entry is used by the construction cutover.

use core::cell::Cell;
use core::mem::offset_of;
use core::ptr::NonNull;

use crate::abi::Oop;

use crate::boundary;
use crate::id::object_memory::ObjectMemory;
use crate::raw::AddressMap;

/// The payload shape retained for vtable objects themselves.
#[allow(non_snake_case)]
#[repr(C)]
pub(crate) struct RawVtable {
    pub(crate) size: usize,
    pub(crate) tally: usize,
    pub(crate) keys: *mut Oop,
    pub(crate) values: *mut Oop,
    pub(crate) parent: Oop,
}

const _: () = {
    assert!(size_of::<RawVtable>() == 40);
    assert!(align_of::<RawVtable>() == 8);
    assert!(offset_of!(RawVtable, size) == 0);
    assert!(offset_of!(RawVtable, tally) == 8);
    assert!(offset_of!(RawVtable, keys) == 16);
    assert!(offset_of!(RawVtable, values) == 24);
    assert!(offset_of!(RawVtable, parent) == 32);
};

pub(crate) struct VtableRecord {
    pub(crate) raw: NonNull<RawVtable>,
    pub(crate) logical_capacity: usize,
    pub(crate) keys: Vec<Oop>,
    pub(crate) values: Vec<Oop>,
    pub(crate) weak_keys: Vec<bool>,
    callable: Vec<bool>,
    // Derived identity lookup only. Ordered vectors retain raw-view and GC
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

    fn sync_raw_view(&mut self) {
        // SAFETY: raw is the live, uniquely runtime-owned payload allocated for
        // this record. No foreign call or other reference overlaps this update.
        let raw = unsafe { self.raw.as_mut() };
        raw.size = self.logical_capacity;
        raw.tally = self.keys.len();
        raw.keys = if self.keys.is_empty() {
            core::ptr::null_mut()
        } else {
            self.keys.as_mut_ptr()
        };
        raw.values = if self.values.is_empty() {
            core::ptr::null_mut()
        } else {
            self.values.as_mut_ptr()
        };
        raw.parent = self.parent;
    }

    fn grow_dictionary(&mut self) {
        let next_capacity = if self.logical_capacity == 0 {
            4
        } else {
            self.logical_capacity
                .checked_mul(2)
                .unwrap_or_else(|| boundary::fatal_invariant("vtable dictionary capacity overflow"))
        };
        self.keys.reserve_exact(next_capacity - self.keys.len());
        self.values.reserve_exact(next_capacity - self.values.len());
        self.callable
            .reserve_exact(next_capacity - self.callable.len());
        self.weak_keys
            .reserve_exact(next_capacity - self.weak_keys.len());
        self.logical_capacity = next_capacity;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddMethod<T = Oop> {
    Added,
    Overwrote(T),
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
    pub(crate) const PAYLOAD_SIZE: usize = size_of::<RawVtable>();

    /// Construct a behavior using Id allocation alone.
    pub(crate) fn create(
        &mut self,
        memory: &mut ObjectMemory,
        parent: Oop,
        metavtable: Oop,
    ) -> Oop {
        let object = memory.allocate(Self::PAYLOAD_SIZE);
        assert!(memory.set_vtable(object, metavtable));
        // SAFETY: the fresh allocation has the exact behavior payload extent.
        unsafe { self.register(object, parent) };
        object
    }

    /// Atomically construct a fresh behavior with its initial method. No caller
    /// can have observed this allocation yet, so no existing lookup changes.
    /// Subsequent edits still use add_method and advance both epochs.
    pub(crate) fn create_with_method(
        &mut self,
        memory: &mut ObjectMemory,
        parent: Oop,
        meta: Oop,
        selector: Oop,
        method: Oop,
    ) -> Oop {
        assert!(!method.is_null());
        let object = self.create(memory, parent, meta);
        self.insert_method(object, selector, method);
        object
    }

    /// Register a fresh behavior allocation supplied by the caller.
    /// # Safety
    /// `object` must retain at least PAYLOAD_SIZE aligned bytes until retirement.
    pub(crate) unsafe fn register(&mut self, object: Oop, parent: Oop) {
        boundary::require_invariant(
            parent.is_null() || self.is_registered(parent),
            "vtable parent is not registered",
        );
        let raw = NonNull::new(object.cast::<RawVtable>())
            .unwrap_or_else(|| boundary::fatal_invariant("new vtable payload is null"));
        // SAFETY: object owns a zeroed, aligned payload of exactly RawVtable's
        // size, and no typed value or reference exists there yet.
        unsafe {
            raw.as_ptr().write(RawVtable {
                size: 0,
                tally: 0,
                keys: core::ptr::null_mut(),
                values: core::ptr::null_mut(),
                parent,
            });
        }

        let previous = self.by_address.insert(
            object.addr(),
            VtableRecord {
                raw,
                logical_capacity: 0,
                keys: Vec::new(),
                values: Vec::new(),
                weak_keys: Vec::new(),
                callable: Vec::new(),
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
    pub(crate) fn is_registered_address(&self, address: usize) -> bool {
        self.by_address.contains_key(&address)
    }
    pub(crate) fn strong_edges(&self, vtable: Oop) -> Vec<Oop> {
        let Some(record) = self.by_address.get(&vtable.addr()) else {
            return Vec::new();
        };
        let mut edges = Vec::with_capacity(record.keys.len().saturating_add(2));
        if !record.parent.is_null() {
            edges.push(record.parent);
        }
        edges.extend(
            record
                .keys
                .iter()
                .copied()
                .zip(record.weak_keys.iter().copied())
                .filter_map(|(key, weak)| (!weak && !key.is_null()).then_some(key)),
        );
        edges
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
    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }
    pub(crate) fn view(&self) -> &[Oop] {
        &self.ordered
    }
    pub(crate) fn at(&self, index: usize) -> Oop {
        self.ordered
            .get(index)
            .copied()
            .unwrap_or(core::ptr::null_mut())
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
        record.sync_raw_view();
        self.note_mutation();
    }
    pub(crate) fn dictionary_capacity(&self, vtable: Oop) -> usize {
        self.record(vtable).logical_capacity
    }
    pub(crate) fn selector_at(&self, vtable: Oop, index: usize) -> Oop {
        self.record(vtable)
            .keys
            .get(index)
            .copied()
            .unwrap_or(core::ptr::null_mut())
    }
    pub(crate) fn selector_view(&self, vtable: Oop) -> &[Oop] {
        &self.record(vtable).keys
    }
    pub(crate) fn direct_lookup(&self, vtable: Oop, selector: Oop) -> Option<Oop> {
        let record = self.record(vtable);
        record
            .key_index(selector)
            // Object-valued internal dictionary entries are not methods.
            .and_then(|index| record.callable[index].then_some(record.values[index]))
    }
    pub(crate) fn lookup_head(&self, vtable: Oop, selector: Oop) -> Result<Option<Oop>, Oop> {
        let record = self.record(vtable);
        if let Some(index) = record.key_index(selector) {
            return Ok(record.callable[index].then_some(record.values[index]));
        }
        Err(record.parent)
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
    pub(crate) fn has_default_lookup(&self, meta: Oop, selector: Oop, expected: Oop) -> bool {
        self.default_lookup_fact(meta, selector, expected).is_some()
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
                return record.callable[index].then_some(record.values[index]);
            }
            if let Some(index) = record.key_index(selector) {
                if let Some(slot) = hint {
                    slot.set(Some(index));
                }
                return record.callable[index].then_some(record.values[index]);
            }
            // Never memoize inherited resolution or absence: read this live link.
            vtable = record.parent;
        }
        None
    }
    pub(crate) fn add_method(&mut self, vtable: Oop, selector: Oop, method: Oop) -> AddMethod<Oop> {
        assert!(!method.is_null(), "method binding requires a Method object");
        self.note_mutation();
        self.insert_method(vtable, selector, method)
    }
    fn insert_method(&mut self, vtable: Oop, selector: Oop, method: Oop) -> AddMethod<Oop> {
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(selector) {
            let previous = core::mem::replace(&mut record.values[index], method);
            record.callable[index] = true;
            record.sync_raw_view();
            return AddMethod::Overwrote(previous);
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(selector);
        record.values.push(method);
        record.callable.push(true);
        record.weak_keys.push(false);
        record.index_appended_key();
        record.sync_raw_view();
        AddMethod::Added
    }
    pub(crate) fn add_weak_opaque_binding(&mut self, vtable: Oop, key: Oop, value: Oop) {
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(key) {
            record.values[index] = value;
            record.callable[index] = false;
            record.weak_keys[index] = true;
            record.sync_raw_view();
            return;
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(key);
        record.values.push(value);
        record.callable.push(false);
        record.weak_keys.push(true);
        record.index_appended_key();
        record.sync_raw_view();
    }
    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<Oop> {
        self.note_mutation();
        let record = self.record_mut(vtable);
        let index = record.key_index(selector)?;
        record.keys.remove(index);
        let removed = record.values.remove(index);
        record.weak_keys.remove(index);
        let callable = record.callable.remove(index);
        record.rebuild_key_indices();
        record.sync_raw_view();
        callable.then_some(removed)
    }
    pub(crate) unsafe fn dispatch_vtable_of(object: Oop) -> Oop {
        // SAFETY: forwarded caller precondition.
        unsafe { ObjectMemory::vtable_of_valid_object(object) }
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
    pub(crate) fn methods(&self, vtable: Oop) -> impl Iterator<Item = Oop> + '_ {
        self.record(vtable)
            .values
            .iter()
            .copied()
            .zip(self.record(vtable).callable.iter())
            .filter_map(|(v, c)| (*c).then_some(v))
    }
    pub(crate) fn remove_weak_keys(&mut self, dead: &AddressMap<()>) -> usize {
        let mut removed = 0usize;
        for record in self.by_address.values_mut() {
            let original_length = record.keys.len();
            let mut index = 0usize;
            while index < record.keys.len() {
                let key = record.keys[index];
                let dead = record.weak_keys[index] && dead.contains_key(&key.addr());
                if dead {
                    record.keys.remove(index);
                    record.values.remove(index);
                    record.callable.remove(index);
                    record.weak_keys.remove(index);
                    removed = removed.saturating_add(1);
                } else {
                    index += 1;
                }
            }
            if record.keys.len() != original_length {
                record.rebuild_key_indices();
            }
            record.sync_raw_view();
        }
        if removed != 0 {
            self.note_mutation();
        }
        removed
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
        assert!(tables.has_default_lookup(meta, selector, method));
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
        assert!(tables.has_default_lookup(meta, key, expected));
        // A distinct Method object is not the canonical lookup Method.
        tables.add_method(meta, key, memory.allocate(0));
        assert!(!tables.has_default_lookup(meta, key, expected));
        tables.add_method(meta, key, expected);
        assert!(tables.has_default_lookup(meta, key, expected));
        tables.remove_method(meta, key);
        assert!(!tables.has_default_lookup(meta, key, expected));
        tables.add_method(parent, key, expected);
        tables.set_parent(meta, parent);
        assert!(tables.has_default_lookup(meta, key, expected));
        tables.set_parent(meta, core::ptr::null_mut());
        assert!(!tables.has_default_lookup(meta, key, expected));
        tables.set_parent(meta, parent);
        assert!(tables.has_default_lookup(meta, key, expected));
        tables.add_weak_opaque_binding(meta, key, parent);
        assert!(!tables.has_default_lookup(meta, key, expected));
        let mut dead = AddressMap::default();
        dead.insert(key.addr(), ());
        tables.remove_weak_keys(&dead);
        assert!(tables.has_default_lookup(meta, key, expected));
        // Positive facts must not survive registry retirement/address reuse.
        tables.remove_objects(&[meta]);
        unsafe {
            tables.register(meta, core::ptr::null_mut());
        }
        assert!(!tables.has_default_lookup(meta, key, expected));
        tables.add_method(meta, key, expected);
        assert!(tables.has_default_lookup(meta, key, expected));
        // Invalidation is explicit, not an equality check on a saturated epoch.
        tables.mutation_epoch = u64::MAX;
        tables.binding_epoch = u64::MAX;
        tables.remove_method(meta, key);
        assert!(!tables.has_default_lookup(meta, key, expected));
        tables.add_method(meta, key, expected);
        assert!(tables.has_default_lookup(meta, key, expected));
        tables.add_weak_opaque_binding(meta, key, parent);
        assert!(!tables.has_default_lookup(meta, key, expected));
    }

    #[test]
    fn slot_hints_follow_live_values_shifts_parents_and_reused_records() {
        fn reference(tables: &Vtables, mut at: Oop, key: Oop) -> Option<Oop> {
            while !at.is_null() {
                let row = tables.record(at);
                for ((&candidate, value), weak) in
                    row.keys.iter().zip(&row.values).zip(&row.callable)
                {
                    if candidate == key {
                        return (*weak).then_some(*value);
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
        // A populated large-dictionary hint must read a newly opaque value,
        // rather than returning the native entry that used to occupy the slot.
        assert_eq!(tables.lookup(other, keys[0]), Some(entry(101)));
        tables.add_weak_opaque_binding(other, keys[0], meta);
        assert_eq!(tables.lookup(other, keys[0]), None);
        let mut removed_key = AddressMap::default();
        removed_key.insert(keys[0].addr(), ());
        tables.remove_weak_keys(&removed_key);
        assert_eq!(tables.lookup(other, keys[0]), None);
        tables.add_method(other, keys[0], entry(101));
        assert_eq!(tables.lookup(other, keys[0]), Some(entry(101)));
        // An opaque entry shadows an inherited method just as it did before.
        tables.set_parent(child, other);
        tables.add_weak_opaque_binding(child, keys[0], meta);
        assert_eq!(tables.lookup(child, keys[0]), None);
        let mut dead = AddressMap::default();
        dead.insert(keys[0].addr(), ());
        tables.remove_weak_keys(&dead);
        assert_eq!(tables.lookup(child, keys[0]), Some(entry(101)));
        // Re-register the same still-allocated payload after retiring its record.
        // Hints do not keep a record or its previous entries alive.
        tables.remove_objects(&[child]);
        unsafe {
            tables.register(child, parent);
        }
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
        assert_eq!(tables.direct_lookup(live, key), Some(entry));
        assert_eq!(tables.binding_epoch(), bound + 2);
    }
    #[test]
    fn initialized_behavior_does_not_change_live_bindings_but_later_edits_do() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        memory.set_vtable(meta, meta);
        let key = memory.allocate(0);
        let entry = memory.allocate(0);
        let before = (tables.binding_epoch(), tables.mutation_epoch());
        let fresh = tables.create_with_method(&mut memory, meta, meta, key, entry);
        assert_eq!(tables.direct_lookup(fresh, key), Some(entry));
        assert_eq!(tables.parent_of(fresh), meta);
        assert_eq!((tables.binding_epoch(), tables.mutation_epoch()), before);
        tables.remove_method(fresh, key);
        tables.add_method(fresh, key, entry);
        assert_eq!(tables.binding_epoch(), before.0 + 2);
        assert_eq!(tables.mutation_epoch(), before.1 + 2);
        assert_eq!(tables.direct_lookup(fresh, key), Some(entry));
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
            Some(40)
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
        assert_eq!(tables.count(), 3);
    }
}
