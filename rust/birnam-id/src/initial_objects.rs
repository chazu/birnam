//! The initial Id object graph, constructed before any upper runtime service.
use crate::abi::{Oop, id_protocol::{Operand, LOADED}};
use crate::abi::id_protocol::NativeEntry;
use crate::object_memory::ObjectMemory;
use crate::symbol::Symbols;
use crate::vtable::Vtables;

pub(crate) struct InitialObjects {
    pub(crate) protocol: Box<crate::native_id::State>,
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
        let mut memory = crate::owned::Owned::new(ObjectMemory::new());
        let mut dictionaries = crate::owned::Owned::new(Vtables::new());
        let mut symbols = crate::owned::Owned::new(Symbols::new());
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
        let context = crate::id_dispatch::Context {
            dictionaries: dictionaries.pointer(), metavtable: vtable_vt,
            lookup, does_not_understand,
        };
        let entry = |method| NativeEntry::new(method, context.dictionaries.cast());
        dictionaries.add_method(vtable_vt, lookup, entry(crate::id_dispatch::vtable_lookup_method));
        dictionaries.add_method(vtable_vt, add_method, entry(crate::id_dispatch::vtable_add_method));
        // Only the first two bindings are ad hoc. The next uses Id's message path.
        unsafe {
            assert_eq!(crate::id_dispatch::send(context, vtable_vt, add_method, &[
                Operand::object(remove_method), Operand::entry(entry(crate::id_dispatch::vtable_remove_method)),
            ]).status, LOADED);
        }
        let protocol = unsafe { crate::native_id::State::new(memory, dictionaries, symbols, vtable_vt, symbol_vt, object_prototype, lookup, does_not_understand) };
        Self { protocol, vtable_vt, object_vt, symbol_vt,
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
        let primitives = unsafe { birnam_abi::primitive_allocation::select(initial.protocol.protocol) }.unwrap_or_else(|_| panic!("Id primitives"));
        let mut heap = Heap::from_id(initial.protocol.protocol, initial.object_vt, primitives, None, None);
        let allocations: Vec<_> = heap.storage_allocations().into_iter().map(|allocation|
            (allocation.object, allocation.identity, allocation.requested_size)
        ).collect();
        let last_identity = allocations.iter().map(|entry| entry.1).max().unwrap();
        let client = crate::id_registries::Client::new(initial.protocol.protocol);
        let mut symbols = crate::id_registries::Symbols::Id(client.clone());
        let name = symbols.c_name_of(initial.lookup);
        heap.retain_initial_objects(&[initial.vtable_vt, initial.object_vt, initial.symbol_vt, initial.object_prototype]);
        let behaviors = Behaviors::adopt(crate::id_registries::Dictionaries::Id(client));
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
        assert_eq!(allocation.identity, last_identity + 1);
    }
}
