use crate::initial_objects::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviors::Behaviors;
    use crate::heap::{Heap, Permanence};

    #[test]
    fn upper_adoption_preserves_the_initial_graph_and_first_collection() {
        let initial = InitialObjects::new();
        let primitives = unsafe { birnam_abi::id_primitives::select(initial.protocol.protocol) }.unwrap_or_else(|_| panic!("Id primitives"));
        let mut heap = Heap::from_id(initial.protocol.protocol, initial.object_vt, primitives, None, None);
        let allocations: Vec<_> = heap.storage_allocations().into_iter().map(|allocation|
            (allocation.object, allocation.identity, allocation.requested_size)
        ).collect();
        let last_identity = allocations.iter().map(|entry| entry.1).max().unwrap();
        let client = crate::id_registries::Client::new(initial.protocol.protocol, primitives);
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
