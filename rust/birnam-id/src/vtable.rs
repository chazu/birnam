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
    parent: Oop,
    owner: Oop,
    trace_hook: Option<TraceHook>,
    layout_kinds: *const u8,
    layout_n: usize,
    layout_rest: u8,
    layout_set: bool,
}

impl VtableRecord {
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
        raw.layout_kinds = self.layout_kinds;
        raw.layout_n = self.layout_n;
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
}

impl Vtables {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
        }
    }

    pub(crate) fn create(&mut self, heap: &mut Heap, parent: Oop, metavtable: Oop) -> Oop {
        boundary::require_invariant(
            parent.is_null() || self.is_registered(parent),
            "vtable parent is not registered",
        );
        let object = heap.allocate(size_of::<RawVtable>(), Permanence::Permanent);
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
                parent,
                owner: core::ptr::null_mut(),
                trace_hook: None,
                layout_kinds: core::ptr::null(),
                layout_n: 0,
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
        self.by_address
            .get(&vtable.addr())
            .and_then(|record| record.trace_hook)
    }

    pub(crate) fn set_layout(&mut self, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
        boundary::require_invariant(
            count == 0 || !kinds.is_null(),
            "setLayout kinds is null with nonzero count",
        );
        let record = self.record_mut(vtable);
        record.layout_kinds = kinds;
        record.layout_n = count;
        record.layout_rest = rest;
        record.layout_set = true;
        record.sync_raw_view();
    }

    pub(crate) fn slot_kind(&self, vtable: Oop, word: usize) -> u8 {
        let record = self.record(vtable);
        if !record.layout_set {
            return SLOT_UNKNOWN;
        }
        if word >= record.layout_n {
            return record.layout_rest;
        }
        // SAFETY: set_layout requires a non-null borrowed static array whenever
        // layout_n is nonzero; the IdApi contract keeps it live with the vtable.
        unsafe { record.layout_kinds.add(word).read() }
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
            .keys
            .iter()
            .position(|candidate| *candidate == selector)
            // SAFETY: callers use this only for method dictionaries. Opaque
            // internal dictionaries are enumerated but never dispatched.
            .and_then(|index| unsafe { MethodAddress::from_oop(record.values[index]) })
    }

    pub(crate) fn lookup(&self, mut vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        while !vtable.is_null() {
            let record = self.record(vtable);
            if let Some(index) = record
                .keys
                .iter()
                .position(|candidate| *candidate == selector)
            {
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
        let record = self.record_mut(vtable);
        if let Some(index) = record
            .keys
            .iter()
            .position(|candidate| *candidate == selector)
        {
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
        record.sync_raw_view();
        AddMethod::Added
    }

    /// Synchronize an opaque key/value pair used by the floor's internal symbol
    /// and export dictionaries. These registered vtables are observable through
    /// MOP enumeration but are never dispatch families.
    pub(crate) fn add_opaque_binding(&mut self, vtable: Oop, key: Oop, value: Oop) {
        let record = self.record_mut(vtable);
        if let Some(index) = record.keys.iter().position(|candidate| *candidate == key) {
            record.values[index] = value;
            record.sync_raw_view();
            return;
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(key);
        record.values.push(value);
        record.sync_raw_view();
    }

    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        let record = self.record_mut(vtable);
        let index = record
            .keys
            .iter()
            .position(|candidate| *candidate == selector)?;
        record.keys.remove(index);
        let removed = record.values.remove(index);
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
    fn deep_delegation_and_large_dictionary_preserve_lookup_and_order() {
        let (mut heap, mut vtables, meta, root, _child) = new_hierarchy();
        let mut inherited_selector = Box::new(0usize);
        let inherited_selector_oop = core::ptr::from_mut(inherited_selector.as_mut()).cast();
        assert_eq!(
            vtables.add_method(root, inherited_selector_oop, answer_3),
            AddMethod::Added
        );

        let mut leaf = root;
        for _ in 0..4096 {
            leaf = vtables.create(&mut heap, leaf, meta);
        }
        assert_eq!(
            call(vtables.lookup(leaf, inherited_selector_oop).unwrap()),
            3
        );

        let mut selector_owners: Vec<Box<usize>> = (0..4096).map(Box::new).collect();
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
        assert_eq!(vtables.dictionary_capacity(leaf), 4096);
        for (index, selector) in selectors.iter().copied().enumerate() {
            assert_eq!(vtables.selector_at(leaf, index), selector);
            assert_eq!(call(vtables.direct_lookup(leaf, selector).unwrap()), 1);
        }

        for index in [0, 1023, 2047, 4095] {
            assert!(matches!(
                vtables.add_method(leaf, selectors[index], answer_2),
                AddMethod::Overwrote(previous) if call(previous) == 1
            ));
            assert_eq!(vtables.selector_at(leaf, index), selectors[index]);
            assert_eq!(call(vtables.lookup(leaf, selectors[index]).unwrap()), 2);
        }

        let removed = selectors[2047];
        assert_eq!(call(vtables.remove_method(leaf, removed).unwrap()), 2);
        assert!(vtables.direct_lookup(leaf, removed).is_none());
        assert_eq!(vtables.selector_at(leaf, 2047), selectors[2048]);
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
    fn trace_hook_and_borrowed_layout_metadata_replace_in_place() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        static FIRST: [u8; 3] = [crate::abi::SLOT_OOP, crate::abi::SLOT_RAW, 9];
        static SECOND: [u8; 1] = [crate::abi::SLOT_CODE];

        assert_eq!(vtables.slot_kind(parent, 0), SLOT_UNKNOWN);
        vtables.set_trace_hook(parent, trace_hook);
        vtables.set_layout(parent, FIRST.as_ptr(), FIRST.len(), crate::abi::SLOT_RAW);
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_OOP);
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_RAW);
        assert_eq!(vtables.slot_kind(parent, 2), 9);
        assert_eq!(vtables.slot_kind(parent, 3), crate::abi::SLOT_RAW);

        vtables.set_layout(parent, SECOND.as_ptr(), SECOND.len(), crate::abi::SLOT_OOP);
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_CODE);
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_OOP);
        assert_eq!(vtables.slot_kind(child, 0), SLOT_UNKNOWN);
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
}
