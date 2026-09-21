//! GC and private-behavior metadata above Id's method dictionaries.
use crate::abi::{Method, Oop, TraceHook};
use crate::vtable::AddMethod as NativeAddMethod;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddMethod {
    Added,
    Overwrote(MethodAddress),
    OverwroteNative(crate::abi::id_protocol::NativeEntry),
}
impl AddMethod {
    pub(crate) fn from_native(change: NativeAddMethod) -> Self {
        match change {
            NativeAddMethod::Added => Self::Added,
            NativeAddMethod::Overwrote(previous) => match legacy_method::try_decode(previous) {
                Some(method) => Self::Overwrote(method), None => Self::OverwroteNative(previous),
            },
        }
    }
}
use crate::legacy_method;
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::vtable::Vtables;
#[cfg(test)]
use crate::abi::SLOT_UNKNOWN;

pub(crate) struct Behaviors {
    dictionaries: crate::id_registries::Dictionaries,
    legacy_lookup: Oop,
    owners: crate::object_model::Records,
    layouts: crate::payload_layout::Service,
    hooks: crate::trace_hooks::Service,
}
impl core::ops::Deref for Behaviors {
    type Target = crate::id_registries::Dictionaries;
    fn deref(&self) -> &Self::Target { &self.dictionaries }
}
impl core::ops::DerefMut for Behaviors {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.dictionaries }
}
impl Behaviors {
    pub(crate) fn native_lookup(&self, vtable: Oop, selector: Oop) -> Option<crate::abi::id_protocol::NativeEntry> { self.dictionaries.lookup(vtable, selector) }
    pub(crate) fn set_legacy_lookup(&mut self, selector: Oop) { self.legacy_lookup = selector; }
    pub(crate) fn add_method(&mut self, vtable: Oop, selector: Oop, method: Method) -> AddMethod {
        AddMethod::from_native(self.dictionaries.add_method(vtable, selector, self.dictionaries.encode_legacy(method, selector == self.legacy_lookup)))
    }
    pub(crate) fn add_native_method(&mut self, vtable: Oop, selector: Oop, entry: crate::abi::id_protocol::NativeEntry) -> NativeAddMethod {
        self.dictionaries.add_method(vtable, selector, entry)
    }
    pub(crate) fn remove_native_method(&mut self, vtable: Oop, selector: Oop) -> Option<crate::abi::id_protocol::NativeEntry> {
        self.dictionaries.remove_method(vtable, selector)
    }
    pub(crate) fn native_direct_lookup(&self, vtable: Oop, selector: Oop) -> Option<crate::abi::id_protocol::NativeEntry> {
        self.dictionaries.direct_lookup(vtable, selector)
    }
    pub(crate) fn remove_method(&mut self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.dictionaries.remove_method(vtable, selector).map(legacy_method::decode)
    }
    // Optimizer/legacy-registration queries recognize only the old convention.
    // Actual dispatch uses native_lookup and retains arbitrary typed entries.
    pub(crate) fn lookup(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.dictionaries.lookup(vtable, selector).and_then(legacy_method::try_decode)
    }
    pub(crate) fn direct_lookup(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.dictionaries.direct_lookup(vtable, selector).and_then(legacy_method::try_decode)
    }
    pub(crate) fn native_lookup_head(&self, vtable: Oop, selector: Oop) -> Result<Option<crate::abi::id_protocol::NativeEntry>, Oop> { self.dictionaries.lookup_head(vtable, selector) }

    pub(crate) fn new() -> Self {
        Self::adopt(crate::owned::Owned::new(Vtables::new()).into())
    }

    /// Add upper metadata without replacing existing behavior objects or keys.
    pub(crate) fn adopt(dictionaries: crate::id_registries::Dictionaries) -> Self {
        Self::adopt_with(dictionaries, None, None)
    }
    pub(crate) fn adopt_with(dictionaries: crate::id_registries::Dictionaries, lifetime: Option<crate::lifetime::Shared>, model: Option<crate::object_model::Shared>) -> Self {
        let layouts = lifetime.clone().map(crate::payload_layout::Service::from_lifetime).unwrap_or_else(crate::payload_layout::Service::new);
        let hooks = lifetime.map(crate::trace_hooks::Service::from_lifetime).unwrap_or_else(crate::trace_hooks::Service::new);
        Self { dictionaries, legacy_lookup: core::ptr::null_mut(), owners: crate::object_model::Records::new(model), layouts, hooks }
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
        self.dictionaries.create(heap, parent, metavtable, permanence)
    }
    fn require_registered(&self, object: Oop) {
        boundary::require_invariant(self.dictionaries.is_registered(object), "unregistered vtable");
    }
    pub(crate) fn strong_edges(&self, vtable: Oop) -> Vec<Oop> {
        let mut edges = self.dictionaries.strong_edges(vtable);
        let owner = self.owner_of(vtable);
        if !owner.is_null() { edges.insert(usize::from(!self.parent_of(vtable).is_null()), owner); }
        edges
    }
    pub(crate) fn permanent_strong_edges(&self, heap: &Heap) -> Vec<Oop> {
        self.dictionaries.ordered()
            .iter()
            .copied()
            .filter(|vtable| heap.permanence_of(*vtable) == Some(Permanence::Permanent))
            .flat_map(|vtable| self.strong_edges(vtable))
            .collect()
    }
    pub(crate) fn has_permanent_binding(&self, heap: &Heap, methods: &[MethodAddress]) -> bool {
        self.dictionaries.ordered().iter().copied().any(|vtable| {
            heap.permanence_of(vtable) == Some(Permanence::Permanent)
                && self.dictionaries.methods(vtable).any(|method| legacy_method::try_decode(method).is_some_and(|method| methods.contains(&method)))
        })
    }
    pub(crate) fn dead_collectable_bindings(&self, heap: &Heap) -> Vec<(Oop, Vec<MethodAddress>)> {
        self.dictionaries.ordered()
            .iter()
            .copied()
            .filter(|vtable| {
                heap.permanence_of(*vtable) == Some(Permanence::Collectable)
                    && heap.is_marked(*vtable) == Some(false)
            })
            .map(|vtable| {
                let methods = self.dictionaries.methods(vtable).filter_map(legacy_method::try_decode).collect();
                (vtable, methods)
            })
            .collect()
    }
    pub(crate) fn remove_dead_collectable(&mut self, heap: &Heap) -> Vec<Oop> {
        let dead = self
            .ordered()
            .iter()
            .copied()
            .filter(|vtable| {
                heap.permanence_of(*vtable) == Some(Permanence::Collectable)
                    && heap.is_marked(*vtable) == Some(false)
            })
            .collect::<Vec<_>>();
        let addresses: Vec<_> = dead.iter().map(|object| object.addr()).collect();
        self.owners.retire(&addresses);
        self.layouts.retire(&addresses);
        self.hooks.retire_local(&addresses);
        self.dictionaries.remove_objects(&dead);
        dead
    }
    /// Retire upper layout/hook/owner records only. Id retires its dictionaries
    /// separately through the storage protocol, after this borrow ends.
    pub(crate) fn retire_metadata(&mut self, addresses: &[usize]) {
        self.owners.retire(addresses);
        self.layouts.retire(addresses);
        self.hooks.retire_local(addresses);
    }
    pub(crate) fn owner_of(&self, candidate: Oop) -> Oop { self.owners.get(candidate) }
    pub(crate) fn set_owner(&mut self, vtable: Oop, owner: Oop) {
        self.require_registered(vtable);
        self.owners.set(vtable, owner);
    }
    pub(crate) fn set_trace_hook(&mut self, vtable: Oop, hook: TraceHook) {
        let slot = self.hooks.slot(hook);
        self.replace_trace_hook(vtable, slot);
    }
    pub(crate) fn replace_trace_hook(&mut self, vtable: Oop, slot: birnam_abi::trace_hooks::SlotV1) {
        self.require_registered(vtable);
        self.hooks.replace(vtable, slot);
    }
    pub(crate) fn trace_hooks_api(&self) -> *mut birnam_abi::trace_hooks::ApiV1 {
        self.hooks.api()
    }
    pub(crate) fn trace_hook(&self, vtable: Oop) -> Option<TraceHook> {
        use birnam_abi::trace_hooks::{EntryV1, SourceV1};
        unsafe extern "C" fn read(context: *const core::ffi::c_void, vtable: Oop, output: *mut EntryV1) -> core::ffi::c_int {
            let registry = unsafe { &*context.cast::<Behaviors>() };
            if !registry.is_registered(vtable) { return 0; }
            unsafe { output.write(EntryV1 { parent: registry.parent_of(vtable), slot: registry.hooks.get(vtable) }) };
            1
        }
        // Canonical selection uses only synchronous, immutable registry facts.
        // The borrow ends before a caller invokes the returned hook.
        let source = SourceV1 { context: core::ptr::from_ref(self).cast(), budget: self.dictionaries.count(), read };
        self.hooks.resolve(&source, vtable)
    }
    pub(crate) fn set_layout(&mut self, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
        // Keep copying before the exact membership lookup, including when the
        // supplied bytes are the current record's own borrowed prefix.
        let owned = self.layouts.copy(kinds, count, rest);
        self.require_registered(vtable);
        self.layouts.replace(vtable, owned.into_carrier());
    }
    pub(crate) fn replace_layout(&mut self, vtable: Oop, owned: birnam_abi::payload_layout::OwnedV1) {
        self.require_registered(vtable);
        self.layouts.replace(vtable, owned);
    }
    pub(crate) fn layout_view(&self, vtable: Oop) -> birnam_abi::payload_layout::ViewV1 {
        self.require_registered(vtable);
        self.layouts.view(vtable)
    }
    pub(crate) fn lifetime_owner(&self) -> Option<birnam_abi::lifetime::OwnedV1> { self.layouts.native_owner() }
    pub(crate) fn layout_api(&self) -> *mut birnam_abi::payload_layout::ApiV1 {
        self.layouts.api()
    }
    pub(crate) fn slot_kind(&self, vtable: Oop, word: usize) -> u8 {
        self.layouts.slot_kind(self.layout_view(vtable), word)
    }
    pub(crate) fn all_oop_layout(&self, vtable: Oop, words: usize) -> bool {
        self.is_registered(vtable) && self.layouts.all_oop(self.layouts.view(vtable), words)
    }
    pub(crate) fn has_layout(&self, vtable: Oop) -> bool {
        self.is_registered(vtable) && self.layouts.has_layout(self.layouts.view(vtable))
    }
    pub(crate) fn checked_vtable_of(heap: &Heap, object: Oop) -> Oop {
        heap.vtable_of(object).unwrap_or(core::ptr::null_mut())
    }
    pub(crate) fn remove_dead_weak_bindings(&mut self, heap: &Heap) -> usize {
        self.dictionaries.remove_weak_keys(&heap.dead_collectable_addresses())
    }
    pub(crate) unsafe fn dispatch_vtable_of(object: Oop) -> Oop {
        unsafe { Vtables::dispatch_vtable_of(object) }
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

    fn new_hierarchy() -> (Heap, Behaviors, Oop, Oop, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Behaviors::new();
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
        assert_eq!(Behaviors::checked_vtable_of(&heap, meta), meta);
        assert_eq!(Behaviors::checked_vtable_of(&heap, child), meta);
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
                vtables.dictionaries.record(parent).key_index(key).is_some(),
                [1, 4, 7].contains(&index)
            );
        }
        for (position, index) in [1, 4, 7].into_iter().enumerate() {
            assert_eq!(vtables.selector_at(parent, position), keys[index]);
        }
        vtables.add_weak_opaque_binding(parent, keys[4], keys[1]);
        assert_eq!(vtables.dictionaries.record(parent).values[1].as_object(), Some(keys[1]));
        vtables.add_weak_opaque_binding(parent, keys[0], keys[0]);
        assert_eq!(vtables.selector_at(parent, 3), keys[0]);
        heap.clear_marks();
        assert_eq!(vtables.remove_dead_weak_bindings(&heap), 4);
        assert!(vtables.dictionaries.record(parent).key_indices.is_empty());
        vtables.add_weak_opaque_binding(parent, keys[7], keys[7]);
        assert_eq!(vtables.dictionaries.record(parent).key_index(keys[7]), Some(0));
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
        let first_pointer = vtables.layout_view(parent).kinds;
        assert_ne!(first_pointer, FIRST.as_ptr());
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_OOP);
        assert!(vtables.trace_hook(child).is_some());
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_RAW);
        assert_eq!(vtables.slot_kind(parent, 2), 9);
        assert_eq!(vtables.slot_kind(parent, 3), crate::abi::SLOT_RAW);

        vtables.set_layout(parent, SECOND.as_ptr(), SECOND.len(), crate::abi::SLOT_OOP);
        let second_pointer = vtables.layout_view(parent).kinds;
        assert_ne!(second_pointer, SECOND.as_ptr());
        assert_eq!(vtables.slot_kind(parent, 0), crate::abi::SLOT_CODE);
        assert_eq!(vtables.slot_kind(parent, 1), crate::abi::SLOT_OOP);
        assert_eq!(vtables.slot_kind(child, 0), SLOT_UNKNOWN);
    }

    #[test]
    fn trace_hooks_keep_exact_records_epoch_live_parents_and_bounded_cycles() {
        unsafe extern "C" fn nearer(_: Oop, _: crate::abi::MarkFn) { panic!("selection invoked hook"); }
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        let address = |hook: Option<TraceHook>| hook.map(|hook| hook as usize);
        let first = Some(trace_hook as *const () as usize);
        let second = Some(nearer as *const () as usize);
        let epoch = vtables.mutation_epoch();
        assert!(vtables.trace_hook(child).is_none());
        vtables.set_trace_hook(parent, trace_hook);
        assert_eq!(vtables.mutation_epoch(), epoch);
        assert_eq!(address(vtables.trace_hook(child)), first);
        assert_eq!(address(vtables.hooks.get(parent).hook), first);
        assert!(vtables.hooks.get(child).hook.is_none());
        vtables.set_trace_hook(parent, nearer);
        assert_eq!(vtables.mutation_epoch(), epoch);
        assert_eq!(address(vtables.trace_hook(child)), second);
        vtables.set_owner(parent, child);
        vtables.set_layout(parent, core::ptr::null(), 0, crate::abi::SLOT_RAW);
        assert_eq!(address(vtables.hooks.get(parent).hook), second);
        vtables.set_parent(child, child);
        assert!(vtables.trace_hook(child).is_none());
        vtables.set_parent(child, parent);
        assert_eq!(address(vtables.trace_hook(child)), second);
        vtables.set_trace_hook(child, trace_hook);
        assert_eq!(address(vtables.trace_hook(child)), first);
        assert!(vtables.trace_hook(core::ptr::null_mut()).is_none());
        assert!(vtables.trace_hook(core::ptr::without_provenance_mut(1)).is_none());
        // Internal damaged links retain absence and count-bound behavior.
        vtables.replace_trace_hook(parent, birnam_abi::trace_hooks::SlotV1::EMPTY);
        vtables.replace_trace_hook(child, birnam_abi::trace_hooks::SlotV1::EMPTY);
        vtables.dictionaries.record_mut(parent).parent = child;
        assert!(vtables.trace_hook(child).is_none());
        vtables.dictionaries.record_mut(parent).parent = core::ptr::without_provenance_mut(1);
        assert!(vtables.trace_hook(child).is_none());
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
    fn payload_layout_keeps_records_and_epoch_when_replacing_borrowed_input() {
        let (_heap, mut vtables, _meta, parent, child) = new_hierarchy();
        let epoch = vtables.mutation_epoch();
        let input = [crate::abi::SLOT_OOP, 9, crate::abi::SLOT_CODE];
        vtables.set_layout(parent, input.as_ptr(), input.len(), crate::abi::SLOT_RAW);
        let first = vtables.layout_view(parent);
        assert_ne!(first.kinds, input.as_ptr());
        assert_eq!(vtables.mutation_epoch(), epoch);
        vtables.set_owner(parent, child);
        vtables.set_trace_hook(parent, trace_hook);
        assert_eq!(vtables.layout_view(parent).kinds, first.kinds);
        vtables.set_layout(parent, first.kinds, first.count, crate::abi::SLOT_OOP);
        let second = vtables.layout_view(parent);
        assert_ne!(second.kinds, first.kinds);
        assert_eq!(vtables.mutation_epoch(), epoch);
        assert_eq!(vtables.slot_kind(parent, 1), 9);
        assert_eq!(vtables.slot_kind(parent, usize::MAX), crate::abi::SLOT_OOP);
        assert!(!vtables.has_layout(child));
        vtables.set_layout(parent, core::ptr::null(), 0, crate::abi::SLOT_RAW);
        let layout = vtables.layout_view(parent);
        assert!(layout.kinds.is_null());
        assert_eq!((layout.count, layout.rest, layout.is_set), (0, 0, 1));
        assert!(vtables.all_oop_layout(parent, 0));
        assert_eq!(vtables.mutation_epoch(), epoch);
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
            assert!(Behaviors::checked_vtable_of(&heap, candidate).is_null());
            assert!(vtables.owner_of(candidate).is_null());
        }

        assert_eq!(Behaviors::checked_vtable_of(&heap, object), parent);
        assert_eq!(vtables.owner_of(parent), object);
        // SAFETY: object is an exact live member here.
        assert_eq!(unsafe { Behaviors::dispatch_vtable_of(object) }, parent);

        assert!(heap.reclaim_for_test(object));
        assert!(Behaviors::checked_vtable_of(&heap, object).is_null());
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
                            let record = vtables.dictionaries.record(parent);
                            record
                                .keys
                                .iter()
                                .position(|candidate| *candidate == key)
                                .and_then(|index| record.values[index].as_entry().map(legacy_method::decode))
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
        let expect_moved = |vtables: &Behaviors, last: &mut u64, what: &str| {
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

#[cfg(test)]
#[path = "vtable/mutation_epoch_tests.rs"]
mod mutation_epoch_tests;

#[cfg(test)]
#[path = "vtable/key_index_tests.rs"]
mod key_index_tests;
