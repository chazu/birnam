//! Raw vtable objects, ordered method dictionaries, and exact membership.
#![allow(dead_code)] // R2.2 precedes runtime/bootstrap wiring.

use core::mem::offset_of;
use core::ptr::NonNull;

use crate::abi::{Method, Oop, TraceHook};
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
    values: Vec<MethodAddress>,
    parent: Oop,
    owner: Oop,
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
            self.values.as_mut_ptr().cast::<Oop>()
        };
        raw.parent = self.parent;
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
            .map(|index| record.values[index])
    }

    pub(crate) fn lookup(&self, mut vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        while !vtable.is_null() {
            let record = self.record(vtable);
            if let Some(index) = record
                .keys
                .iter()
                .position(|candidate| *candidate == selector)
            {
                return Some(record.values[index]);
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
            let previous = core::mem::replace(&mut record.values[index], method);
            record.sync_raw_view();
            return AddMethod::Overwrote(previous);
        }
        if record.keys.len() == record.logical_capacity {
            record.grow_dictionary();
        }
        record.keys.push(selector);
        record.values.push(method);
        record.sync_raw_view();
        AddMethod::Added
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
        Some(removed)
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
