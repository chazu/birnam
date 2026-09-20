//! Raw vtable objects, ordered method dictionaries, and exact membership.
#![allow(dead_code)] // Direct allocation entry is used by the construction cutover.

use core::mem::offset_of;
use core::ptr::NonNull;

use crate::abi::Oop;
use crate::abi::id_protocol::{NativeEntry, Operand};
use crate::boundary;
use crate::object_memory::ObjectMemory;
use crate::raw::AddressMap;

/// The payload shape retained for vtable objects themselves.
#[allow(non_snake_case)]
#[repr(C)]
pub(crate) struct RawVtable {
    pub(crate) size: usize,
    pub(crate) tally: usize,
    pub(crate) keys: *mut Oop,
    pub(crate) values: *mut Operand,
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
    pub(crate) values: Vec<Operand>,
    pub(crate) weak_keys: Vec<bool>,
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
        self.weak_keys
            .reserve_exact(next_capacity - self.weak_keys.len());
        self.logical_capacity = next_capacity;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddMethod<T = NativeEntry> {
    Added,
    Overwrote(T),
}

/// Ordered registry of every vtable in one runtime.
pub(crate) struct Vtables {
    pub(crate) ordered: Vec<Oop>,
    by_address: AddressMap<VtableRecord>,
    // Monotonic count of dictionary, parent and registry mutations. Design 74A
    // §3.2: a reader that cached a verification against one epoch re-verifies
    // when the epoch moved. Over-counting is harmless; never skip a mutator.
    mutation_epoch: u64,

}

impl Vtables {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
            mutation_epoch: 0,
        }
    }
    pub(crate) fn mutation_epoch(&self) -> u64 {
        self.mutation_epoch
    }
    #[cfg(test)]
    pub(crate) fn exhaust_mutation_epoch_for_test(&mut self) {
        self.mutation_epoch = u64::MAX;
    }
    pub(crate) fn note_mutation(&mut self) {
        self.mutation_epoch = self.mutation_epoch.saturating_add(1);
    }
    pub(crate) const PAYLOAD_SIZE: usize = size_of::<RawVtable>();

    /// Construct a behavior using Id allocation alone.
    pub(crate) fn create(&mut self, memory: &mut ObjectMemory, parent: Oop, metavtable: Oop) -> Oop {
        let object = memory.allocate(Self::PAYLOAD_SIZE);
        assert!(memory.set_vtable(object, metavtable));
        // SAFETY: the fresh allocation has the exact behavior payload extent.
        unsafe { self.register(object, parent) };
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
    pub(crate) fn view(&self) -> birnam_abi::vtable_enumeration::ViewV1 {
        birnam_abi::vtable_enumeration::ViewV1 {
            entries: self.ordered.as_ptr(),
            count: self.ordered.len(),
        }
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
    pub(crate) fn selector_view(&self, vtable: Oop) -> birnam_abi::vtable_enumeration::ViewV1 {
        let keys = &self.record(vtable).keys;
        birnam_abi::vtable_enumeration::ViewV1 { entries: keys.as_ptr(), count: keys.len() }
    }
    pub(crate) fn direct_lookup(&self, vtable: Oop, selector: Oop) -> Option<NativeEntry> {
        let record = self.record(vtable);
        record
            .key_index(selector)
            // Object-valued internal dictionary entries are not methods.
            .and_then(|index| record.values[index].as_entry())
    }
    pub(crate) fn lookup_head(
        &self,
        vtable: Oop,
        selector: Oop,
    ) -> Result<Option<NativeEntry>, Oop> {
        let record = self.record(vtable);
        if let Some(index) = record.key_index(selector) {
            return Ok(record.values[index].as_entry());
        }
        Err(record.parent)
    }
    pub(crate) fn lookup(&self, mut vtable: Oop, selector: Oop) -> Option<NativeEntry> {
        while !vtable.is_null() {
            let record = self.record(vtable);
            if let Some(index) = record.key_index(selector) {
                return record.values[index].as_entry();
            }
            vtable = record.parent;
        }
        None
    }
    pub(crate) fn add_method(&mut self, vtable: Oop, selector: Oop, method: NativeEntry) -> AddMethod<NativeEntry> {
        assert!(method.method.is_some(), "method binding requires a native entry");
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(selector) {
            let previous = core::mem::replace(&mut record.values[index], Operand::entry(method)).as_entry().expect("method dictionary held non-entry");
            record.sync_raw_view();
            return AddMethod::Overwrote(previous);
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(selector);
        record.values.push(Operand::entry(method));
        record.weak_keys.push(false);
        record.index_appended_key();
        record.sync_raw_view();
        AddMethod::Added
    }
    pub(crate) fn add_weak_opaque_binding(&mut self, vtable: Oop, key: Oop, value: Oop) {
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(key) {
            record.values[index] = Operand::object(value);
            record.weak_keys[index] = true;
            record.sync_raw_view();
            return;
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(key);
        record.values.push(Operand::object(value));
        record.weak_keys.push(true);
        record.index_appended_key();
        record.sync_raw_view();
    }
    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<NativeEntry> {
        self.note_mutation();
        let record = self.record_mut(vtable);
        let index = record.key_index(selector)?;
        record.keys.remove(index);
        let removed = record.values.remove(index);
        record.weak_keys.remove(index);
        record.rebuild_key_indices();
        record.sync_raw_view();
        removed.as_entry()
    }
    pub(crate) unsafe fn dispatch_vtable_of(object: Oop) -> Oop {
        // SAFETY: forwarded caller precondition.
        unsafe { ObjectMemory::vtable_of_valid_object(object) }
    }
    /// Retire dictionary records before the caller releases their allocations.
    pub(crate) fn remove_objects(&mut self, dead: &[Oop]) {
        for vtable in dead {
            self.by_address.remove(&vtable.addr()).unwrap_or_else(|| boundary::fatal_invariant("dead vtable registry record disappeared"));
        }
        self.ordered.retain(|vtable| !dead.contains(vtable));
        if !dead.is_empty() { self.note_mutation(); }
    }
    pub(crate) fn methods(&self, vtable: Oop) -> impl Iterator<Item = NativeEntry> + '_ {
        self.record(vtable).values.iter().filter_map(|value| value.as_entry())
    }
    pub(crate) fn remove_weak_keys(&mut self, dead: &[usize]) -> usize {
        let dead: AddressMap<()> = dead.iter().map(|address| (*address, ())).collect();
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
    fn behavior_graph_and_dictionary_need_only_id_allocation() {
        let mut memory = ObjectMemory::new();
        let mut tables = Vtables::new();
        let meta = tables.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(memory.set_vtable(meta, meta));
        let parent = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let child = tables.create(&mut memory, parent, meta);
        assert_eq!(memory.get(child.addr()).map(|allocation| allocation.requested_size()), Some(40));
        unsafe extern "C" fn identity(_: *mut core::ffi::c_void, receiver: Oop, _: *const crate::abi::id_protocol::Operand, _: usize) -> crate::abi::id_protocol::Outcome { crate::abi::id_protocol::Outcome::loaded(crate::abi::id_protocol::Operand::object(receiver)) }
        let identity = NativeEntry::new(identity, core::ptr::null_mut());
        let mut symbols = crate::symbol::Symbols::new();
        let symbol_behavior = tables.create(&mut memory, core::ptr::null_mut(), meta);
        let keys: Vec<_> = (0..24).map(|index| symbols.intern(&mut memory, symbol_behavior, format!("key-{index}").as_bytes()).unwrap()).collect();
        assert_eq!(symbols.intern(&mut memory, symbol_behavior, b"key-7").unwrap(), keys[7]);
        for &key in &keys { tables.add_method(parent, key, identity); }
        for &key in &keys { assert_eq!(tables.lookup(child, key), Some(identity)); }
        tables.remove_method(parent, keys[7]);
        assert!(tables.lookup(child, keys[7]).is_none());
        tables.remove_objects(&[child]);
        assert!(!tables.is_registered(child));
        assert!(memory.detach(child.addr()).is_some());
        assert_eq!(tables.count(), 3);
    }
}
