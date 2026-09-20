//! The initial Id object graph, constructed before any upper runtime service.
use crate::abi::Oop;
use crate::object_memory::ObjectMemory;
use crate::symbol::Symbols;
use crate::vtable::Vtables;

pub(crate) struct InitialObjects {
    pub(crate) memory: ObjectMemory,
    pub(crate) dictionaries: Vtables,
    pub(crate) symbols: Symbols,
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
    pub(crate) object_prototype: Oop,
    pub(crate) symbol_prototype: Oop,
}

impl InitialObjects {
    pub(crate) fn new() -> Self {
        let mut memory = ObjectMemory::new();
        let mut dictionaries = Vtables::new();
        let mut symbols = Symbols::new();
        let vtable_vt = dictionaries.create(&mut memory, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(memory.set_vtable(vtable_vt, vtable_vt));
        let object_vt = dictionaries.create(&mut memory, core::ptr::null_mut(), vtable_vt);
        dictionaries.set_parent(vtable_vt, object_vt);
        let symbol_vt = dictionaries.create(&mut memory, object_vt, vtable_vt);
        let object_prototype = memory.allocate(0);
        assert!(memory.set_vtable(object_prototype, object_vt));
        let lookup = symbols.intern(&mut memory, symbol_vt, b"lookup").unwrap();
        let does_not_understand = symbols.intern(&mut memory, symbol_vt, b"doesNotUnderstand:").unwrap();
        let symbol_prototype = symbols.intern(&mut memory, symbol_vt, b"Symbol").unwrap();
        let add_method = symbols.intern(&mut memory, symbol_vt, b"addMethod").unwrap();
        let remove_method = symbols.intern(&mut memory, symbol_vt, b"removeMethod:").unwrap();
        dictionaries.add_method(vtable_vt, lookup, crate::id_dispatch::vtable_lookup_method);
        dictionaries.add_method(vtable_vt, add_method, crate::id_dispatch::vtable_add_method);
        // The first two bindings terminate the metacircular construction. Use
        // Id's own message path for the next binding, before any Runtime exists.
        unsafe { crate::id_dispatch::enter(&mut dictionaries, vtable_vt, lookup, does_not_understand, || {
            crate::id_dispatch::send(vtable_vt, add_method, &[
                remove_method, crate::raw::MethodAddress::from_method(crate::id_dispatch::vtable_remove_method).as_oop(),
            ]).expect("Id primitive installation was unbound");
        }) };
        Self { memory, dictionaries, symbols, vtable_vt, object_vt, symbol_vt,
            lookup, does_not_understand, object_prototype, symbol_prototype }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviors::Behaviors;
    use crate::heap::{Heap, Permanence};

    #[test]
    fn upper_adoption_preserves_the_initial_graph_and_first_collection() {
        let initial = InitialObjects::new();
        let allocations: Vec<_> = initial.memory.iter().map(|(_, allocation)|
            (allocation.payload().as_ptr().cast(), allocation.identity(), allocation.requested_size())
        ).collect();
        let name = initial.symbols.c_name_of(initial.lookup);
        let mut heap = Heap::adopt(initial.memory);
        heap.retain_initial_objects(&[initial.vtable_vt, initial.object_vt, initial.symbol_vt, initial.object_prototype]);
        let behaviors = Behaviors::adopt(initial.dictionaries);
        let mut symbols = initial.symbols;
        let roots: Vec<_> = (0..symbols.count()).map(|index| symbols.at(index)).collect();
        crate::gc::trace_conservative_for_test(&mut heap, &roots);
        assert!(heap.dead_collectable_addresses().is_empty());
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 0);
        assert_eq!(symbols.c_name_of(initial.lookup), name);
        assert_eq!(heap.intern_symbol(&mut symbols, initial.symbol_vt, b"lookup"), Ok(initial.lookup));
        for (object, identity, requested_size) in allocations {
            let allocation = heap.storage_allocations().into_iter().find(|entry| entry.object == object).unwrap();
            assert_eq!((allocation.identity, allocation.requested_size), (identity, requested_size));
        }
        assert_eq!(behaviors.parent_of(initial.vtable_vt), initial.object_vt);
        assert_eq!(behaviors.parent_of(initial.symbol_vt), initial.object_vt);
        assert_eq!(heap.vtable_of(initial.vtable_vt), Some(initial.vtable_vt));
        assert_eq!(heap.permanence_of(initial.lookup), Some(Permanence::Collectable));
        heap.clear_marks();
        let dictionary_roots = behaviors.permanent_strong_edges(&heap);
        crate::gc::trace_conservative_for_test(&mut heap, &dictionary_roots);
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 2);
        let dead = heap.detach_dead_collectable(initial.does_not_understand.addr()).unwrap();
        dead.reclaim();
        assert!(symbols.existing(b"doesNotUnderstand:").is_none());
        assert_eq!(symbols.existing(b"lookup"), Some(initial.lookup));
        let replacement = heap.intern_symbol(&mut symbols, initial.symbol_vt, b"doesNotUnderstand:").unwrap();
        let allocation = heap.storage_allocations().into_iter().find(|entry| entry.object == replacement).unwrap();
        assert_eq!(allocation.identity, 10);
    }
}
