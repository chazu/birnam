//! Raw vtable objects, ordered method dictionaries, and exact membership.
#![allow(dead_code)] // R2.2 precedes runtime/bootstrap wiring.

use core::mem::offset_of;
use core::ptr::NonNull;

use crate::abi::{Method, Oop, SLOT_UNKNOWN, TraceHook};
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::{AddressMap, MethodAddress};

/// The payload shape retained for vtable objects themselves.
#[allow(non_snake_case)]
#[repr(C)]
struct RawVtable {
    size: usize,
    tally: usize,
    keys: *mut Oop,
    values: *mut Oop,
    parent: Oop,
    trace_hook: Option<TraceHook>,
    layout_kinds: *const u8,
    layout_n: usize,
    layout_rest: u8,
    layout_set: u8,
    owner: Oop,
}

const _: () = {
    assert!(size_of::<RawVtable>() == 80);
    assert!(align_of::<RawVtable>() == 8);
    assert!(offset_of!(RawVtable, size) == 0);
    assert!(offset_of!(RawVtable, tally) == 8);
    assert!(offset_of!(RawVtable, keys) == 16);
    assert!(offset_of!(RawVtable, values) == 24);
    assert!(offset_of!(RawVtable, parent) == 32);
    assert!(offset_of!(RawVtable, trace_hook) == 40);
    assert!(offset_of!(RawVtable, layout_kinds) == 48);
    assert!(offset_of!(RawVtable, layout_n) == 56);
    assert!(offset_of!(RawVtable, layout_rest) == 64);
    assert!(offset_of!(RawVtable, layout_set) == 65);
    assert!(offset_of!(RawVtable, owner) == 72);
};

struct VtableRecord {
    raw: NonNull<RawVtable>,
    logical_capacity: usize,
    keys: Vec<Oop>,
    values: Vec<Oop>,
    weak_keys: Vec<bool>,
    // Derived identity lookup only. Ordered vectors retain raw-view and GC
    // authority; these integer addresses never add roots or cache dispatch.
    key_indices: AddressMap<usize>,
    parent: Oop,
    owner: Oop,
    trace_hook: Option<TraceHook>,
    layout_kinds: Box<[u8]>,
    layout_rest: u8,
    layout_set: bool,
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
    fn key_index(&self, key: Oop) -> Option<usize> {
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
        raw.trace_hook = self.trace_hook;
        raw.layout_kinds = if self.layout_kinds.is_empty() {
            core::ptr::null()
        } else {
            self.layout_kinds.as_ptr()
        };
        raw.layout_n = self.layout_kinds.len();
        raw.layout_rest = self.layout_rest;
        raw.layout_set = u8::from(self.layout_set);
        raw.owner = self.owner;
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
pub(crate) enum AddMethod {
    Added,
    Overwrote(MethodAddress),
}

/// Ordered registry of every vtable in one runtime.
pub(crate) struct Vtables {
    ordered: Vec<Oop>,
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

    pub(crate) fn create(&mut self, heap: &mut Heap, parent: Oop, metavtable: Oop) -> Oop {
        self.create_with_permanence(heap, parent, metavtable, Permanence::Permanent)
    }

    pub(crate) fn create_collectable(
        &mut self,
        heap: &mut Heap,
        parent: Oop,
        metavtable: Oop,
    ) -> Oop {
        self.create_with_permanence(heap, parent, metavtable, Permanence::Collectable)
    }

    fn create_with_permanence(
        &mut self,
        heap: &mut Heap,
        parent: Oop,
        metavtable: Oop,
        permanence: Permanence,
    ) -> Oop {
        boundary::require_invariant(
            parent.is_null() || self.is_registered(parent),
            "vtable parent is not registered",
        );
        let object = heap.allocate(size_of::<RawVtable>(), permanence);
        boundary::require_invariant(
            heap.set_vtable(object, metavtable),
            "new vtable block was not registered",
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
                trace_hook: None,
                layout_kinds: core::ptr::null(),
                layout_n: 0,
                layout_rest: 0,
                layout_set: 0,
                owner: core::ptr::null_mut(),
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
                owner: core::ptr::null_mut(),
                trace_hook: None,
                layout_kinds: Box::new([]),
                layout_rest: 0,
                layout_set: false,
            },
        );
        boundary::require_invariant(previous.is_none(), "duplicate vtable address");
        self.ordered.push(object);
        object
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
        if !record.owner.is_null() {
            edges.push(record.owner);
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

    pub(crate) fn permanent_strong_edges(&self, heap: &Heap) -> Vec<Oop> {
        self.ordered
            .iter()
            .copied()
            .filter(|vtable| heap.permanence_of(*vtable) == Some(Permanence::Permanent))
            .flat_map(|vtable| self.strong_edges(vtable))
            .collect()
    }

    pub(crate) fn has_permanent_binding(&self, heap: &Heap, methods: &[MethodAddress]) -> bool {
        self.ordered.iter().copied().any(|vtable| {
            heap.permanence_of(vtable) == Some(Permanence::Permanent)
                && self
                    .record(vtable)
                    .values
                    .iter()
                    .filter_map(|value| unsafe { MethodAddress::from_oop(*value) })
                    .any(|method| methods.contains(&method))
        })
    }

    pub(crate) fn dead_collectable_bindings(&self, heap: &Heap) -> Vec<(Oop, Vec<MethodAddress>)> {
        self.ordered
            .iter()
            .copied()
            .filter(|vtable| {
                heap.permanence_of(*vtable) == Some(Permanence::Collectable)
                    && heap.is_marked(*vtable) == Some(false)
            })
            .map(|vtable| {
                let methods = self
                    .record(vtable)
                    .values
                    .iter()
                    .filter_map(|value| unsafe { MethodAddress::from_oop(*value) })
                    .collect();
                (vtable, methods)
            })
            .collect()
    }

    pub(crate) fn remove_dead_collectable(&mut self, heap: &Heap) -> Vec<Oop> {
        let dead = self
            .ordered
            .iter()
            .copied()
            .filter(|vtable| {
                heap.permanence_of(*vtable) == Some(Permanence::Collectable)
                    && heap.is_marked(*vtable) == Some(false)
            })
            .collect::<Vec<_>>();
        for vtable in &dead {
            self.by_address.remove(&vtable.addr()).unwrap_or_else(|| {
                boundary::fatal_invariant("dead vtable registry record disappeared")
            });
        }
        self.ordered.retain(|vtable| !dead.contains(vtable));
        if !dead.is_empty() {
            self.note_mutation();
        }
        dead
    }

    /// Remove weak dictionary entries whose collectable key did not survive
    /// the mark phase. This must run before either the Symbol registry or Heap
    /// detaches the corresponding storage.
    pub(crate) fn remove_dead_weak_bindings(&mut self, heap: &Heap) -> usize {
        let mut removed = 0usize;
        for record in self.by_address.values_mut() {
            let original_length = record.keys.len();
            let mut index = 0usize;
            while index < record.keys.len() {
                let key = record.keys[index];
                let dead = record.weak_keys[index]
                    && heap.permanence_of(key) == Some(Permanence::Collectable)
                    && heap.is_marked(key) == Some(false);
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

    fn record(&self, vtable: Oop) -> &VtableRecord {
        self.by_address
            .get(&vtable.addr())
            .unwrap_or_else(|| boundary::fatal_invariant("vtable is not registered"))
    }

    fn record_mut(&mut self, vtable: Oop) -> &mut VtableRecord {
        self.by_address
            .get_mut(&vtable.addr())
            .unwrap_or_else(|| boundary::fatal_invariant("vtable is not registered"))
    }

    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
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

    pub(crate) fn owner_of(&self, candidate: Oop) -> Oop {
        self.by_address
            .get(&candidate.addr())
            .map(|record| record.owner)
            .unwrap_or(core::ptr::null_mut())
    }

    pub(crate) fn set_owner(&mut self, vtable: Oop, owner: Oop) {
        let record = self.record_mut(vtable);
        record.owner = owner;
        record.sync_raw_view();
    }

    pub(crate) fn set_trace_hook(&mut self, vtable: Oop, hook: TraceHook) {
        let record = self.record_mut(vtable);
        record.trace_hook = Some(hook);
        record.sync_raw_view();
    }

    pub(crate) fn trace_hook(&self, vtable: Oop) -> Option<TraceHook> {
        let mut current = vtable;
        let mut remaining = self.by_address.len();
        while !current.is_null() && remaining != 0 {
            let record = self.by_address.get(&current.addr())?;
            if let Some(hook) = record.trace_hook {
                return Some(hook);
            }
            current = record.parent;
            remaining -= 1;
        }
        None
    }

    pub(crate) fn set_layout(&mut self, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
        boundary::require_invariant(
            count == 0 || !kinds.is_null(),
            "setLayout kinds is null with nonzero count",
        );
        let owned = if count == 0 {
            Box::new([])
        } else {
            // SAFETY: the caller supplies `count` readable bytes for this
            // synchronous setter. The Runtime copies them before returning so
            // the vtable never borrows storage from a reclaimable mapping.
            unsafe { core::slice::from_raw_parts(kinds, count) }
                .to_vec()
                .into_boxed_slice()
        };
        let record = self.record_mut(vtable);
        record.layout_kinds = owned;
        record.layout_rest = rest;
        record.layout_set = true;
        record.sync_raw_view();
    }

    pub(crate) fn slot_kind(&self, vtable: Oop, word: usize) -> u8 {
        let record = self.record(vtable);
        if !record.layout_set {
            return SLOT_UNKNOWN;
        }
        if word >= record.layout_kinds.len() {
            return record.layout_rest;
        }
        record.layout_kinds[word]
    }

    /// Inspect the current exact-vtable layout with one registry lookup.
    /// Explicit prefix kinds and the repeated tail both apply to this extent.
    pub(crate) fn all_oop_layout(&self, vtable: Oop, words: usize) -> bool {
        self.by_address.get(&vtable.addr()).is_some_and(|record| {
            record.layout_set
                && record.layout_kinds[..words.min(record.layout_kinds.len())]
                    .iter()
                    .all(|&kind| kind == crate::abi::SLOT_OOP)
                && (words <= record.layout_kinds.len()
                    || record.layout_rest == crate::abi::SLOT_OOP)
        })
    }

    pub(crate) fn has_layout(&self, vtable: Oop) -> bool {
        self.by_address
            .get(&vtable.addr())
            .is_some_and(|record| record.layout_set)
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

    pub(crate) fn direct_lookup(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        let record = self.record(vtable);
        record
            .key_index(selector)
            // SAFETY: callers use this only for method dictionaries. Opaque
            // internal dictionaries are enumerated but never dispatched.
            .and_then(|index| unsafe { MethodAddress::from_oop(record.values[index]) })
    }

    /// Resolve one dictionary, returning its delegation edge on a miss.
    /// The caller may continue without reading this dictionary a second time.
    #[inline]
    pub(crate) fn lookup_head(
        &self,
        vtable: Oop,
        selector: Oop,
    ) -> Result<Option<MethodAddress>, Oop> {
        let record = self.record(vtable);
        if let Some(index) = record.key_index(selector) {
            // SAFETY: same method-dictionary contract as lookup/direct_lookup.
            return Ok(unsafe { MethodAddress::from_oop(record.values[index]) });
        }
        Err(record.parent)
    }

    pub(crate) fn lookup(&self, mut vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        while !vtable.is_null() {
            let record = self.record(vtable);
            if let Some(index) = record.key_index(selector) {
                // SAFETY: dispatch reaches registered method dictionaries;
                // opaque internal dictionaries are not receiver families.
                return unsafe { MethodAddress::from_oop(record.values[index]) };
            }
            vtable = record.parent;
        }
        None
    }

    pub(crate) fn add_method(&mut self, vtable: Oop, selector: Oop, method: Method) -> AddMethod {
        let method = MethodAddress::from_method(method);
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(selector) {
            let previous = core::mem::replace(&mut record.values[index], method.as_oop());
            record.sync_raw_view();
            let previous = unsafe { MethodAddress::from_oop(previous) }
                .unwrap_or_else(|| boundary::fatal_invariant("method binding held null data"));
            return AddMethod::Overwrote(previous);
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(selector);
        record.values.push(method.as_oop());
        record.weak_keys.push(false);
        record.index_appended_key();
        record.sync_raw_view();
        AddMethod::Added
    }

    /// Synchronize an opaque key/value pair used by the floor's internal symbol
    /// and export dictionaries. These registered vtables are observable through
    /// MOP enumeration but are never dispatch families.
    pub(crate) fn add_weak_opaque_binding(&mut self, vtable: Oop, key: Oop, value: Oop) {
        self.note_mutation();
        let record = self.record_mut(vtable);
        if let Some(index) = record.key_index(key) {
            record.values[index] = value;
            record.weak_keys[index] = true;
            record.sync_raw_view();
            return;
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(key);
        record.values.push(value);
        record.weak_keys.push(true);
        record.index_appended_key();
        record.sync_raw_view();
    }

    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.note_mutation();
        let record = self.record_mut(vtable);
        let index = record.key_index(selector)?;
        record.keys.remove(index);
        let removed = record.values.remove(index);
        record.weak_keys.remove(index);
        record.rebuild_key_indices();
        record.sync_raw_view();
        // SAFETY: remove_method is used only for direct method dictionaries.
        unsafe { MethodAddress::from_oop(removed) }
            .or_else(|| boundary::fatal_invariant("method binding held null data"))
    }

    /// Checked reflection path: arbitrary carrier values never reach a header
    /// dereference unless exact heap membership succeeds first.
    pub(crate) fn checked_vtable_of(heap: &Heap, object: Oop) -> Oop {
        heap.vtable_of(object).unwrap_or(core::ptr::null_mut())
    }

    /// Dispatch path for a receiver already proven valid by language/runtime
    /// construction. It deliberately performs no membership lookup.
    ///
    /// # Safety
    ///
    /// `object` must be the exact non-null payload address of a live block.
    pub(crate) unsafe fn dispatch_vtable_of(object: Oop) -> Oop {
        // SAFETY: forwarded caller precondition.
        unsafe { Heap::vtable_of_valid_object(object) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;

    unsafe extern "C" fn answer_1(_: Oop, _: *const Oop, _: usize) -> Oop {
        1usize as Oop
    }
    unsafe extern "C" fn answer_2(_: Oop, _: *const Oop, _: usize) -> Oop {
        2usize as Oop
    }
    unsafe extern "C" fn answer_3(_: Oop, _: *const Oop, _: usize) -> Oop {
        3usize as Oop
    }
    unsafe extern "C" fn trace_hook(_: Oop, _: crate::abi::MarkFn) {}

    fn call(method: MethodAddress) -> usize {
        // SAFETY: test methods accept the ABI's null/zero argument form.
        unsafe { (method.to_method())(core::ptr::null_mut(), core::ptr::null(), 0) }.addr()
    }

    fn new_hierarchy() -> (Heap, Vtables, Oop, Oop, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Vtables::new();
        let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(heap.set_vtable(meta, meta));
        let parent = vtables.create(&mut heap, core::ptr::null_mut(), meta);
        let child = vtables.create(&mut heap, parent, meta);
        (heap, vtables, meta, parent, child)
    }

    #[test]
    fn raw_vtable_layout_and_creation_order_are_stable() {
        let (heap, vtables, meta, parent, child) = new_hierarchy();
        assert_eq!(vtables.count(), 3);
        assert_eq!(vtables.at(0), meta);
        assert_eq!(vtables.at(1), parent);
        assert_eq!(vtables.at(2), child);
        assert!(vtables.at(3).is_null());
        assert_eq!(vtables.parent_of(meta), core::ptr::null_mut());
        assert_eq!(vtables.parent_of(child), parent);
        assert_eq!(Vtables::checked_vtable_of(&heap, meta), meta);
        assert_eq!(Vtables::checked_vtable_of(&heap, child), meta);
    }

    #[test]
    fn dictionary_grows_overwrites_and_enumerates_in_insertion_order() {
        let (_heap, mut vtables, _meta, parent, _child) = new_hierarchy();
        let mut selectors = [0usize; 20];

        assert_eq!(vtables.dictionary_capacity(parent), 0);
        for (index, selector) in selectors.iter_mut().enumerate() {
            let selector_oop = core::ptr::from_mut(selector).cast::<c_void>();
            assert_eq!(
                vtables.add_method(parent, selector_oop, answer_1),
                AddMethod::Added
            );
            let expected_capacity = match index + 1 {
                1..=4 => 4,
                5..=8 => 8,
                9..=16 => 16,
                _ => 32,
            };
            assert_eq!(vtables.dictionary_capacity(parent), expected_capacity);
        }

        let overwritten = vtables.add_method(
            parent,
            core::ptr::from_mut(&mut selectors[7]).cast(),
            answer_2,
        );
        assert!(matches!(overwritten, AddMethod::Overwrote(previous) if call(previous) == 1));
        assert_eq!(
            call(
                vtables
                    .direct_lookup(parent, vtables.selector_at(parent, 7))
                    .unwrap()
            ),
            2
        );
        for (index, selector) in selectors.iter_mut().enumerate() {
            assert_eq!(
                vtables.selector_at(parent, index),
                core::ptr::from_mut(selector).cast()
            );
        }
        assert!(vtables.selector_at(parent, selectors.len()).is_null());
    }

    #[test]
    fn indexed_mutations_preserve_order_and_current_inherited_resolution() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        let mut storage = [0usize; 64];
        let selectors: Vec<Oop> = storage
            .iter_mut()
            .map(|v| core::ptr::from_mut(v).cast())
            .collect();
        for &selector in &selectors {
            vtables.add_method(parent, selector, answer_3);
        }
        let mut expected: Vec<(Oop, usize)> = Vec::new();
        let mut random = 17u64;
        for _ in 0..1024 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let selector = selectors[(random >> 32) as usize % selectors.len()];
            let position = expected.iter().position(|(key, _)| *key == selector);
            if random & 3 == 0 {
                let removed = position.map(|index| expected.remove(index).1);
                assert_eq!(vtables.remove_method(child, selector).map(call), removed);
            } else {
                let value = if random & 4 == 0 { 1 } else { 2 };
                vtables.add_method(
                    child,
                    selector,
                    if value == 1 { answer_1 } else { answer_2 },
                );
                if let Some(index) = position {
                    expected[index].1 = value;
                } else {
                    expected.push((selector, value));
                }
            }
            for &key in &selectors {
                let direct = expected
                    .iter()
                    .find(|(selector, _)| *selector == key)
                    .map(|(_, value)| *value);
                assert_eq!(vtables.direct_lookup(child, key).map(call), direct);
                assert_eq!(
                    vtables.lookup(child, key).map(call),
                    Some(direct.unwrap_or(3))
                );
            }
            for (index, (key, _)) in expected.iter().enumerate() {
                assert_eq!(vtables.selector_at(child, index), *key);
            }
            assert!(vtables.selector_at(child, expected.len()).is_null());
        }
    }

    #[test]
    fn weak_index_compaction_preserves_middle_survivors_and_reinsertion() {
        let (mut heap, mut vtables, _meta, parent, _child) = new_hierarchy();
        let keys: Vec<_> = (0..9)
            .map(|_| heap.allocate(0, Permanence::Collectable))
            .collect();
        for &key in &keys {
            vtables.add_weak_opaque_binding(parent, key, key);
        }
        heap.clear_marks();
        for index in [1, 4, 7] {
            heap.mark_collectable_address(keys[index].addr());
        }
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 6);
        for (index, &key) in keys.iter().enumerate() {
            assert_eq!(
                vtables.record(parent).key_index(key).is_some(),
                [1, 4, 7].contains(&index)
            );
        }
        for (position, index) in [1, 4, 7].into_iter().enumerate() {
            assert_eq!(vtables.selector_at(parent, position), keys[index]);
        }
        vtables.add_weak_opaque_binding(parent, keys[4], keys[1]);
        assert_eq!(vtables.record(parent).values[1], keys[1]);
        vtables.add_weak_opaque_binding(parent, keys[0], keys[0]);
        assert_eq!(vtables.selector_at(parent, 3), keys[0]);
        heap.clear_marks();
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 4);
        assert!(vtables.record(parent).key_indices.is_empty());
        vtables.add_weak_opaque_binding(parent, keys[7], keys[7]);
        assert_eq!(vtables.record(parent).key_index(keys[7]), Some(0));
    }

    #[test]
    fn weak_opaque_keys_are_visible_but_neither_traced_nor_retained_after_marking() {
        let (mut heap, mut vtables, _meta, parent, child) = new_hierarchy();
        let weak_key = heap.allocate(0, Permanence::Collectable);
        let strong_key = heap.allocate(0, Permanence::Collectable);
        assert!(heap.set_vtable(weak_key, parent));
        assert!(heap.set_vtable(strong_key, parent));

        vtables.add_weak_opaque_binding(parent, weak_key, core::ptr::null_mut());
        assert_eq!(
            vtables.add_method(child, strong_key, answer_1),
            AddMethod::Added
        );
        assert_eq!(vtables.selector_at(parent, 0), weak_key);
        assert!(!vtables.strong_edges(parent).contains(&weak_key));
        assert!(vtables.strong_edges(child).contains(&strong_key));

        heap.clear_marks();
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 1);
        assert!(vtables.selector_at(parent, 0).is_null());
        assert_eq!(vtables.selector_at(child, 0), strong_key);
    }

    #[test]
    fn deep_delegation_and_large_dictionary_preserve_lookup_and_order() {
        let (mut heap, mut vtables, meta, root, _child) = new_hierarchy();
        let count = if cfg!(miri) { 128 } else { 4096 };
        let mut inherited_selector = Box::new(0usize);
        let inherited_selector_oop = core::ptr::from_mut(inherited_selector.as_mut()).cast();
        assert_eq!(
            vtables.add_method(root, inherited_selector_oop, answer_3),
            AddMethod::Added
        );

        let mut leaf = root;
        for _ in 0..count {
            leaf = vtables.create(&mut heap, leaf, meta);
        }
        assert_eq!(
            call(vtables.lookup(leaf, inherited_selector_oop).unwrap()),
            3
        );

        let mut selector_owners: Vec<Box<usize>> = (0..count).map(Box::new).collect();
        let selectors: Vec<Oop> = selector_owners
            .iter_mut()
            .map(|owner| core::ptr::from_mut(owner.as_mut()).cast())
            .collect();
        for selector in selectors.iter().copied() {
            assert_eq!(
                vtables.add_method(leaf, selector, answer_1),
                AddMethod::Added
            );
        }
        assert_eq!(vtables.dictionary_capacity(leaf), count);
        for (index, selector) in selectors.iter().copied().enumerate() {
            assert_eq!(vtables.selector_at(leaf, index), selector);
            assert_eq!(call(vtables.direct_lookup(leaf, selector).unwrap()), 1);
        }

        for index in [0, count / 4 - 1, count / 2 - 1, count - 1] {
            assert!(matches!(
                vtables.add_method(leaf, selectors[index], answer_2),
                AddMethod::Overwrote(previous) if call(previous) == 1
            ));
            assert_eq!(vtables.selector_at(leaf, index), selectors[index]);
            assert_eq!(call(vtables.lookup(leaf, selectors[index]).unwrap()), 2);
        }

        let removed_index = count / 2 - 1;
        let removed = selectors[removed_index];
        assert_eq!(call(vtables.remove_method(leaf, removed).unwrap()), 2);
        assert!(vtables.direct_lookup(leaf, removed).is_none());
        assert_eq!(
            vtables.selector_at(leaf, removed_index),
            selectors[removed_index + 1]
        );
        assert!(vtables.selector_at(leaf, selectors.len() - 1).is_null());
    }

    #[test]
    fn exact_removal_reveals_parent_and_preserves_remaining_order() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        let mut selector = 0usize;
        let mut other = 0usize;
        let selector: Oop = core::ptr::from_mut(&mut selector).cast();
        let other: Oop = core::ptr::from_mut(&mut other).cast();

        assert_eq!(
            vtables.add_method(parent, selector, answer_1),
            AddMethod::Added
        );
        assert_eq!(
            vtables.add_method(child, selector, answer_2),
            AddMethod::Added
        );
        assert_eq!(vtables.add_method(child, other, answer_3), AddMethod::Added);
        assert_eq!(call(vtables.lookup(child, selector).unwrap()), 2);

        assert_eq!(call(vtables.remove_method(child, selector).unwrap()), 2);
        assert_eq!(call(vtables.lookup(child, selector).unwrap()), 1);
        assert_eq!(vtables.selector_at(child, 0), other);
        assert!(vtables.selector_at(child, 1).is_null());
        assert_eq!(vtables.remove_method(child, selector), None);

        assert!(matches!(
            vtables.add_method(parent, selector, answer_3),
            AddMethod::Overwrote(_)
        ));
        assert_eq!(call(vtables.lookup(child, selector).unwrap()), 3);
    }

    #[test]
    fn trace_hook_and_runtime_owned_layout_metadata_replace_in_place() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        static FIRST: [u8; 3] = [crate::abi::SLOT_OOP, crate::abi::SLOT_RAW, 9];
        static SECOND: [u8; 1] = [crate::abi::SLOT_CODE];

        assert_eq!(vtables.slot_kind(parent, 0), SLOT_UNKNOWN);
        vtables.set_trace_hook(parent, trace_hook);
        vtables.set_layout(parent, FIRST.as_ptr(), FIRST.len(), crate::abi::SLOT_RAW);
        // SAFETY: the registered record and its raw view remain live in this
        // vtable harness.
        let first_pointer = unsafe { vtables.record(parent).raw.as_ref().layout_kinds };
        assert_ne!(first_pointer, FIRST.as_ptr());
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_OOP);
        assert!(vtables.trace_hook(child).is_some());
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_RAW);
        assert_eq!(vtables.slot_kind(parent, 2), 9);
        assert_eq!(vtables.slot_kind(parent, 3), crate::abi::SLOT_RAW);

        vtables.set_layout(parent, SECOND.as_ptr(), SECOND.len(), crate::abi::SLOT_OOP);
        // SAFETY: the registered record and its replacement raw view remain
        // live in this vtable harness.
        let second_pointer = unsafe { vtables.record(parent).raw.as_ref().layout_kinds };
        assert_ne!(second_pointer, SECOND.as_ptr());
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_CODE);
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_OOP);
        assert_eq!(vtables.slot_kind(child, 0), SLOT_UNKNOWN);
    }

    #[test]
    fn all_oop_layout_checks_current_prefix_tail_and_exact_vtable() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        assert!(!vtables.all_oop_layout(parent, 1));
        assert!(!vtables.all_oop_layout(core::ptr::null_mut(), 1));
        let mixed = [crate::abi::SLOT_OOP, crate::abi::SLOT_RAW];
        vtables.set_layout(parent, mixed.as_ptr(), mixed.len(), crate::abi::SLOT_OOP);
        assert!(vtables.all_oop_layout(parent, 1));
        assert!(!vtables.all_oop_layout(parent, 2));
        assert!(!vtables.all_oop_layout(parent, 128));
        assert!(!vtables.all_oop_layout(child, 1));
        vtables.set_layout(parent, mixed.as_ptr(), 1, crate::abi::SLOT_RAW);
        assert!(vtables.all_oop_layout(parent, 1));
        assert!(!vtables.all_oop_layout(parent, 2));
        vtables.set_layout(parent, core::ptr::null(), 0, crate::abi::SLOT_OOP);
        assert!(vtables.all_oop_layout(parent, 128));
        vtables.set_layout(parent, core::ptr::null(), 0, crate::abi::SLOT_UNKNOWN);
        assert!(!vtables.all_oop_layout(parent, 1));
    }

    #[test]
    fn selector_equality_is_pointer_identity_not_pointed_to_value() {
        let (_heap, mut vtables, _meta, parent, _child) = new_hierarchy();
        let mut first = 7u64;
        let mut second = 7u64;
        let first: Oop = core::ptr::from_mut(&mut first).cast();
        let second: Oop = core::ptr::from_mut(&mut second).cast();
        assert_eq!(
            vtables.add_method(parent, first, answer_1),
            AddMethod::Added
        );
        assert!(vtables.direct_lookup(parent, second).is_none());
        assert_eq!(call(vtables.direct_lookup(parent, first).unwrap()), 1);
    }

    #[test]
    fn checked_membership_rejects_methods_and_foreign_addresses() {
        let (mut heap, mut vtables, _meta, parent, _child) = new_hierarchy();
        let object = heap.allocate(0, Permanence::Collectable);
        assert!(heap.set_vtable(object, parent));
        vtables.set_owner(parent, object);

        let method_address = MethodAddress::from_method(answer_1).as_oop();
        let foreign_words = [0usize; 2];
        let candidates = [
            core::ptr::null_mut(),
            method_address,
            core::ptr::addr_of!(foreign_words).cast_mut().cast(),
            0x1000usize as Oop,
            0x1001usize as Oop,
        ];
        for candidate in candidates {
            assert!(Vtables::checked_vtable_of(&heap, candidate).is_null());
            assert!(vtables.owner_of(candidate).is_null());
        }

        assert_eq!(Vtables::checked_vtable_of(&heap, object), parent);
        assert_eq!(vtables.owner_of(parent), object);
        // SAFETY: object is an exact live member here.
        assert_eq!(unsafe { Vtables::dispatch_vtable_of(object) }, parent);

        assert!(heap.reclaim_for_test(object));
        assert!(Vtables::checked_vtable_of(&heap, object).is_null());
    }

    #[test]
    fn parent_repointing_and_empty_tables_are_total() {
        let (_heap, mut vtables, meta, parent, child) = new_hierarchy();
        let mut missing = 0usize;
        let missing: Oop = core::ptr::from_mut(&mut missing).cast();
        assert!(vtables.direct_lookup(child, missing).is_none());
        assert!(vtables.lookup(child, missing).is_none());
        assert!(vtables.selector_at(child, 0).is_null());

        vtables.set_parent(child, meta);
        assert_eq!(vtables.parent_of(child), meta);
        vtables.set_parent(child, parent);
        assert_eq!(vtables.parent_of(child), parent);
    }
    #[test]
    #[ignore = "diagnostic scaling probe; host-gated timing acceptance is separate"]
    fn method_lookup_scaling() {
        use std::hint::black_box;
        use std::time::Instant;
        for count in [4, 32, 256, 4096] {
            let (_heap, mut vtables, _meta, parent, _child) = new_hierarchy();
            let mut owners: Vec<_> = (0..=count).map(Box::new).collect();
            let keys: Vec<Oop> = owners
                .iter_mut()
                .map(|value| core::ptr::from_mut(value.as_mut()).cast())
                .collect();
            for &key in &keys[..count] {
                vtables.add_method(parent, key, answer_1);
            }
            for (kind, key) in [("hit", keys[count - 1]), ("miss", keys[count])] {
                for indexed in [false, true] {
                    let started = Instant::now();
                    for _ in 0..100_000 {
                        let key = black_box(key);
                        let result = if indexed {
                            vtables.direct_lookup(parent, key)
                        } else {
                            let record = vtables.record(parent);
                            record
                                .keys
                                .iter()
                                .position(|candidate| *candidate == key)
                                .and_then(|index| unsafe {
                                    MethodAddress::from_oop(record.values[index])
                                })
                        };
                        black_box(result);
                    }
                    eprintln!(
                        "method count={count} kind={kind} indexed={indexed} ns={}",
                        started.elapsed().as_nanos() / 100_000
                    );
                }
            }
        }
    }

    #[test]
    fn every_dictionary_mutator_moves_the_mutation_epoch() {
        let (mut heap, mut vtables, meta, parent, child) = new_hierarchy();
        let mut storage = [0usize; 3];
        let keys: Vec<Oop> = storage
            .iter_mut()
            .map(|v| core::ptr::from_mut(v).cast())
            .collect();
        let expect_moved = |vtables: &Vtables, last: &mut u64, what: &str| {
            assert!(
                vtables.mutation_epoch() > *last,
                "{what} did not move the epoch"
            );
            *last = vtables.mutation_epoch();
        };
        let mut last = vtables.mutation_epoch();

        assert_eq!(
            vtables.add_method(parent, keys[0], answer_1),
            AddMethod::Added
        );
        expect_moved(&vtables, &mut last, "add_method (added)");
        assert!(matches!(
            vtables.add_method(parent, keys[0], answer_2),
            AddMethod::Overwrote(_)
        ));
        expect_moved(&vtables, &mut last, "add_method (overwrote)");
        assert!(vtables.remove_method(parent, keys[0]).is_some());
        expect_moved(&vtables, &mut last, "remove_method");
        vtables.set_parent(child, core::ptr::null_mut());
        expect_moved(&vtables, &mut last, "set_parent");
        vtables.set_parent(child, parent);
        expect_moved(&vtables, &mut last, "set_parent (restore)");

        let weak = heap.allocate(0, Permanence::Collectable);
        vtables.add_weak_opaque_binding(parent, weak, weak);
        expect_moved(&vtables, &mut last, "add_weak_opaque_binding");
        heap.clear_marks();
        assert!(heap.mark_collectable_address(weak.addr()));
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 0);
        assert_eq!(
            vtables.mutation_epoch(),
            last,
            "a sweep that removed nothing is not a mutation"
        );
        heap.clear_marks();
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 1);
        expect_moved(&vtables, &mut last, "remove_dead_weak_bindings");

        let collectable = vtables.create_collectable(&mut heap, parent, meta);
        assert!(vtables.is_registered(collectable));
        heap.clear_marks();
        assert_eq!(vtables.remove_dead_collectable(&heap), vec![collectable]);
        expect_moved(&vtables, &mut last, "remove_dead_collectable");
        assert!(vtables.remove_dead_collectable(&heap).is_empty());
        assert_eq!(
            vtables.mutation_epoch(),
            last,
            "an empty vtable sweep is not a mutation"
        );
    }
}
