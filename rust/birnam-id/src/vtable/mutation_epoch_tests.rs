//! The same exact-value witness runs against the original u64 and native owner.
use super::*;
use crate::behaviors::AddMethod;
unsafe extern "C" fn one(_: Oop, _: *const Oop, _: usize) -> Oop { core::ptr::without_provenance_mut(1) }
unsafe extern "C" fn two(_: Oop, _: *const Oop, _: usize) -> Oop { core::ptr::without_provenance_mut(2) }
unsafe extern "C" fn trace(_: Oop, _: crate::abi::MarkFn) {}
fn hierarchy() -> (Heap, Behaviors, Oop, Oop, Oop) {
    let mut heap=Heap::new();let mut tables=Behaviors::new();
    let meta=tables.create(&mut heap,core::ptr::null_mut(),core::ptr::null_mut());assert!(heap.set_vtable(meta,meta));
    let parent=tables.create(&mut heap,core::ptr::null_mut(),meta);
    let child=tables.create(&mut heap,parent,meta);(heap,tables,meta,parent,child)
}
fn snapshot(tables: &Behaviors, parent: Oop, expected: u64, label: &str) {
    assert_eq!(tables.mutation_epoch(),expected);
    let record=tables.dictionaries.record(parent);let raw=unsafe { record.raw.as_ref() };
    assert_eq!(raw.tally,record.keys.len());assert_eq!(raw.size,record.logical_capacity);
    assert_eq!(raw.parent,record.parent);
    if raw.tally!=0 {
        assert_eq!(unsafe { core::slice::from_raw_parts(raw.keys,raw.tally) },record.keys);
        assert_eq!(unsafe { core::slice::from_raw_parts(raw.values,raw.tally) },record.values);
    }
    println!("epoch {label}: {expected}, tally {}, capacity {}, tables {}",raw.tally,raw.size,tables.count());
}
#[test]
fn exact_epoch_and_raw_view_transcript() {
    let mut owners=vec![hierarchy(),hierarchy()];
    let (mut heap,mut tables,meta,parent,child)=owners.remove(0);
    let mut key_storage=[0usize;2];let key=core::ptr::from_mut(&mut key_storage[0]).cast();let missing=core::ptr::from_mut(&mut key_storage[1]).cast();
    snapshot(&tables,parent,0,"moved registry creation is not a mutation");
    assert_eq!(tables.add_method(parent,key,one),AddMethod::Added);snapshot(&tables,parent,1,"add");
    assert_eq!(tables.add_method(parent,key,two),AddMethod::Overwrote(MethodAddress::from_method(one)));snapshot(&tables,parent,2,"overwrite");
    assert_eq!(tables.lookup(child,key),Some(MethodAddress::from_method(two)));
    assert_eq!(tables.remove_method(parent,missing),None);snapshot(&tables,parent,3,"absent removal still counts");
    tables.set_parent(child,parent);snapshot(&tables,parent,4,"identical parent still counts");
    tables.set_parent(child,core::ptr::null_mut());assert!(tables.lookup(child,key).is_none());snapshot(&tables,parent,5,"detach parent");
    tables.set_parent(child,parent);snapshot(&tables,parent,6,"restore parent");
    tables.set_owner(parent,child);tables.set_layout(parent,core::ptr::null(),0,crate::abi::SLOT_RAW);tables.set_trace_hook(parent,trace);
    snapshot(&tables,parent,6,"owner layout and hook leave epoch unchanged");
    let weak_a=heap.allocate(0,Permanence::Collectable);let weak_b=heap.allocate(0,Permanence::Collectable);
    tables.add_weak_opaque_binding(parent,weak_a,weak_a);snapshot(&tables,parent,7,"weak add");
    tables.add_weak_opaque_binding(parent,weak_a,weak_b);snapshot(&tables,parent,8,"weak overwrite");
    tables.add_weak_opaque_binding(parent,weak_b,weak_a);snapshot(&tables,parent,9,"second weak add");
    heap.clear_marks();assert!(heap.mark_collectable_address(weak_a.addr()));assert!(heap.mark_collectable_address(weak_b.addr()));
    assert_eq!(tables.remove_dead_weak_bindings(&heap),0);snapshot(&tables,parent,9,"empty weak sweep");
    heap.clear_marks();assert_eq!(tables.remove_dead_weak_bindings(&heap),2);snapshot(&tables,parent,10,"two weak removals count once");
    assert_eq!(tables.remove_dead_weak_bindings(&heap),0);snapshot(&tables,parent,10,"repeated empty weak sweep");
    assert_eq!(tables.remove_method(parent,key),Some(MethodAddress::from_method(two)));snapshot(&tables,parent,11,"present removal");
    let dead_a=tables.create_collectable(&mut heap,parent,meta);let dead_b=tables.create_collectable(&mut heap,parent,meta);
    assert_eq!(tables.mutation_epoch(),11);heap.clear_marks();
    assert_eq!(tables.remove_dead_collectable(&heap),vec![dead_a,dead_b]);snapshot(&tables,parent,12,"two registry removals count once");
    assert!(tables.remove_dead_collectable(&heap).is_empty());snapshot(&tables,parent,12,"empty registry sweep");
    tables.exhaust_mutation_epoch_for_test();
    tables.add_method(parent,key,one);tables.add_method(parent,key,two);tables.set_parent(child,parent);assert_eq!(tables.remove_method(parent,missing),None);
    assert_eq!(tables.lookup(child,key),Some(MethodAddress::from_method(two)));snapshot(&tables,parent,u64::MAX,"saturation retains live mutation");
    assert_eq!(owners[0].1.mutation_epoch(),0);
    drop(heap);drop(tables); // Native state destruction must not consult the Heap.
    let survivor=&mut owners[0].1;survivor.note_mutation();assert_eq!(survivor.mutation_epoch(),1);
    println!("epoch surviving registry after Heap and peer teardown: 1");
}
