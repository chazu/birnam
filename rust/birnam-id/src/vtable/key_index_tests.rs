//! Identical witness on the original map and extracted owner; oracle is linear.
use super::*;
use crate::vtable::AddMethod;
unsafe extern "C" fn one(_: Oop, _: *const Oop, _: usize) -> Oop { core::ptr::without_provenance_mut(1) }
unsafe extern "C" fn two(_: Oop, _: *const Oop, _: usize) -> Oop { core::ptr::without_provenance_mut(2) }
fn hierarchy() -> (Heap, Behaviors, Oop, Oop, Oop) {
    let mut heap=Heap::new();let mut tables=Behaviors::new();
    let meta=tables.create(&mut heap,core::ptr::null_mut(),core::ptr::null_mut());assert!(heap.set_vtable(meta,meta));
    let parent=tables.create(&mut heap,core::ptr::null_mut(),meta);
    let child=tables.create(&mut heap,parent,meta);(heap,tables,meta,parent,child)
}
fn snapshot(tables: &Behaviors, table: Oop, keys: &[Oop], expected: &[(usize,Oop)], weak: bool, epoch: u64, capacity: usize, label: &str) {
    let record=tables.dictionaries.record(table);let raw=unsafe { record.raw.as_ref() };
    assert_eq!(tables.mutation_epoch(),epoch);assert_eq!(raw.tally,expected.len());assert_eq!(raw.size,capacity);
    assert_eq!(record.logical_capacity,capacity);assert_eq!(raw.parent,record.parent);
    assert_eq!(record.key_indices.is_empty(),expected.len()<=16);
    assert_eq!(record.keys,expected.iter().map(|(i,_)| keys[*i]).collect::<Vec<_>>());
    assert_eq!(record.values.iter().map(|entry| if weak { entry.as_object().unwrap() } else { crate::legacy_method::decode(entry.as_entry().unwrap()).as_oop() }).collect::<Vec<_>>(),expected.iter().map(|(_,v)| *v).collect::<Vec<_>>());
    assert_eq!(record.weak_keys,vec![weak;expected.len()]);
    if raw.tally>0 {
        assert_eq!(unsafe { core::slice::from_raw_parts(raw.keys,raw.tally) },record.keys);
        assert_eq!(unsafe { core::slice::from_raw_parts(raw.values,raw.tally) },record.values);
    } else { assert!(raw.keys.is_null() && raw.values.is_null()); }
    for (i,&key) in keys.iter().enumerate() {
        assert_eq!(record.key_index(key),expected.iter().position(|(j,_)| *j==i));
        assert_eq!(tables.strong_edges(table).contains(&key),!weak && expected.iter().any(|(j,_)| *j==i));
    }
    for (position,(index,_)) in expected.iter().enumerate() { assert_eq!(tables.selector_at(table,position),keys[*index]); }
    assert!(tables.selector_at(table,expected.len()).is_null());
    let order:Vec<_>=expected.iter().map(|(i,_)| *i).collect();
    println!("key-index {label}: epoch {epoch}, capacity {capacity}, order {order:?}");
}
#[test]
fn threshold_rebuilds_live_values_moves_and_independent_teardown() {
    let mut owners=vec![hierarchy(),hierarchy()];
    let (mut heap,mut tables,meta,parent,child)=owners.remove(0);
    let mut storage=[0usize;24];let keys:Vec<Oop>=storage.iter_mut().map(|key| core::ptr::from_mut(key).cast()).collect();
    let first=MethodAddress::from_method(one).as_oop();let second=MethodAddress::from_method(two).as_oop();
    let mut expected=Vec::new();let mut epoch=0;let mut capacity=0;
    snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"empty");
    for i in 0..20 {
        assert_eq!(tables.add_method(parent,keys[i],one),AddMethod::Added);expected.push((i,first));epoch+=1;
        if expected.len()>capacity { capacity=if capacity==0 {4} else {capacity*2}; }
        snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"append");
    }
    assert_eq!(tables.add_method(parent,keys[7],two),AddMethod::Overwrote(MethodAddress::from_method(one)));
    expected[7].1=second;epoch+=1;
    assert_eq!(tables.lookup(child,keys[7]),Some(MethodAddress::from_method(two)));
    snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"overwrite is current");
    assert!(tables.remove_method(parent,keys[23]).is_none());epoch+=1;
    snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"missing removal");
    for index in [0,9,19,18] {
        let position=expected.iter().position(|(i,_)| *i==index).unwrap();let (_,value)=expected.remove(position);
        assert_eq!(tables.remove_method(parent,keys[index]).unwrap().as_oop(),value);epoch+=1;
        snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"front middle end and shrink");
    }
    for index in [0,9] { tables.add_method(parent,keys[index],two);expected.push((index,second));epoch+=1; }
    snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"address reinsertion and regrowth");
    for _ in 0..256 { tables.create(&mut heap,parent,meta); }
    tables.add_method(parent,keys[22],one);expected.push((22,first));epoch+=1;
    snapshot(&tables,parent,&keys,&expected,false,epoch,capacity,"registry rehash and append");
    for &(index,value) in &expected { assert_eq!(tables.lookup(child,keys[index]).unwrap().as_oop(),value); }
    assert_eq!(owners[0].1.mutation_epoch(),0);
    drop(heap);drop(tables);
    owners[0].1.note_mutation();assert_eq!(owners[0].1.mutation_epoch(),1);
    println!("key-index independent registry survives peer and Heap destruction");
}
#[test]
fn indexed_weak_compaction_retains_only_marked_keys_and_rebuilds_positions() {
    let (mut heap,mut tables,_meta,parent,_child)=hierarchy();
    let keys:Vec<_>=(0..32).map(|_| heap.allocate(0,Permanence::Collectable)).collect();
    let mut expected:Vec<_>=(0..32).map(|i| (i,keys[31-i])).collect();
    for &(i,value) in &expected { tables.add_weak_opaque_binding(parent,keys[i],value); }
    snapshot(&tables,parent,&keys,&expected,true,32,32,"all weak indexed");
    heap.clear_marks();for &key in &keys { heap.mark_collectable_address(key.addr()); }
    assert_eq!(tables.remove_dead_weak_bindings(&heap),0);
    snapshot(&tables,parent,&keys,&expected,true,32,32,"unchanged marked sweep");
    heap.clear_marks();for i in 0..32 { if i%3!=0 { heap.mark_collectable_address(keys[i].addr()); } }
    expected.retain(|(i,_)| i%3!=0);assert_eq!(tables.remove_dead_weak_bindings(&heap),11);
    snapshot(&tables,parent,&keys,&expected,true,33,32,"indexed middle survivors");
    heap.clear_marks();for &(i,_) in expected.iter().take(16) { heap.mark_collectable_address(keys[i].addr()); }
    expected.truncate(16);assert_eq!(tables.remove_dead_weak_bindings(&heap),5);
    snapshot(&tables,parent,&keys,&expected,true,34,32,"weak shrink to linear");
    for i in [0,3] { tables.add_weak_opaque_binding(parent,keys[i],keys[i]);expected.push((i,keys[i])); }
    snapshot(&tables,parent,&keys,&expected,true,36,32,"weak regrowth");
    tables.add_weak_opaque_binding(parent,keys[3],keys[0]);expected.last_mut().unwrap().1=keys[0];
    snapshot(&tables,parent,&keys,&expected,true,37,32,"weak overwrite");
    heap.clear_marks();assert_eq!(tables.remove_dead_weak_bindings(&heap),18);expected.clear();
    snapshot(&tables,parent,&keys,&expected,true,38,32,"all weak keys disappear");
    assert_eq!(tables.remove_dead_weak_bindings(&heap),0);
    snapshot(&tables,parent,&keys,&expected,true,38,32,"empty repeat sweep");
    drop(heap);drop(tables);
}
