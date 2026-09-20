//! Identical behavioral witness against the original Vec and extracted owner.
use super::*;

fn owner() -> (Heap, Oop) {
    let mut heap = Heap::new();
    let vt = heap.allocate(0, Permanence::Permanent);
    assert!(heap.set_vtable(vt, vt));
    (heap, vt)
}
fn counts(heap: &Heap) -> (usize, usize, usize) {
    (heap.allocation_count(), heap.allocated_bytes(), heap.bytes_since_collection())
}
fn check(heap: &Heap, handles: &[(u64, u64)]) {
    let rows = heap.deferred_index_rows_for_test();
    assert_eq!(rows.len(), handles.len());
    for &(id, index) in &rows {
        let tag = handles.iter().find(|&&(key, _)| key == id).unwrap().1;
        let bytes = heap.dead_allocations[index].payload();
        assert_eq!(u64::from_ne_bytes(bytes.try_into().unwrap()), tag);
    }
    let mut actual = rows.iter().map(|row| row.0).collect::<Vec<_>>(); actual.sort_unstable();
    let mut expected = handles.iter().map(|row| row.0).collect::<Vec<_>>(); expected.sort_unstable();
    assert_eq!(actual, expected);
}
fn report(name: &str, heap: &Heap) {
    let physical = heap.memory.last_identity();
    println!("deferred-index {name}: counts={:?} capacity={} native={} physical={physical} next={} rows={:?}", counts(heap), heap.deferred_allocations.capacity(), heap.dead_allocation_storage_bytes(), heap.next_deferred_id.get(), heap.deferred_index_rows_for_test());
}

#[test]
fn growth_mixed_rows_displaced_unhandled_entries_and_recovery_order() {
    let (mut heap, vt) = owner();
    let numeric = heap.prepare_numeric_batch(vt, vec![-73, 73]).unwrap();
    let mut handles = Vec::new();
    let mut oracle_rows = Vec::new();
    let mut ids = Vec::new();
    report("empty", &heap);
    for tag in 0..65u64 {
        let snapshot = heap.prepare_virtual_bytes(vt, &tag.to_ne_bytes()).unwrap();
        if tag % 5 == 4 {
            assert!(heap.record_dead_allocation(snapshot));
        } else {
            let index = heap.dead_allocations.len();
            let id = heap.record_virtual_clone(snapshot).unwrap();
            handles.push((id.0, tag)); ids.push(id); oracle_rows.push((id.0, index));
        }
        if tag % 8 == 0 {
            heap.record_numeric_result(numeric.clone(), 0).unwrap();
        }
        assert_eq!(heap.deferred_index_rows_for_test(), oracle_rows);
        assert_eq!(heap.deferred_allocations.capacity(), oracle_rows.capacity());
        check(&heap, &handles);
        if [0, 4, 16, 64].contains(&tag) { report(&format!("insert-{tag}"), &heap); }
    }
    let numeric_id = heap.record_numeric_result(numeric.clone(), 1).unwrap();
    let charged = counts(&heap);
    let old_rows = heap.deferred_index_rows_for_test();
    let number = heap.materialize_recorded_clone(numeric_id).unwrap();
    assert_eq!(heap.read_payload_word(number, 0), Some(73));
    assert_eq!(heap.deferred_index_rows_for_test(), old_rows);
    assert!(heap.materialize_recorded_clone(numeric_id).is_none());
    assert_eq!(counts(&heap), charged);
    report("numeric-fallback", &heap);
    // Front/middle/end removals displace both indexed and unhandled snapshots.
    for (step, id) in ids.iter().step_by(2).chain(ids.iter().skip(1).step_by(2)).enumerate() {
        let before = heap.deferred_index_rows_for_test();
        let row = before.iter().position(|&(key, _)| key == id.0).unwrap();
        let expected_order = before.iter().enumerate().filter_map(|(i, pair)| {
            if i == before.len()-1 { None } else if i == row { Some(before.last().unwrap().0) } else { Some(pair.0) }
        }).collect::<Vec<_>>();
        let tag = handles.iter().find(|&&(key, _)| key == id.0).unwrap().1;
        let object = heap.materialize_recorded_clone(*id).unwrap();
        assert_eq!(heap.read_payload_word(object, 0), Some(tag as usize));
        assert_eq!(heap.vtable_of(object), Some(vt));
        handles.retain(|&(key, _)| key != id.0);
        check(&heap, &handles);
        assert_eq!(heap.deferred_index_rows_for_test().iter().map(|p| p.0).collect::<Vec<_>>(), expected_order);
        assert!(heap.materialize_recorded_clone(*id).is_none());
        assert_eq!(counts(&heap), charged);
        if [0, 1, 25, 50, 51].contains(&step) { report(&format!("remove-{step}"), &heap); }
    }
    let capacity = heap.deferred_allocations.capacity();
    assert_eq!(heap.reclaim_dead_allocations(), 22);
    assert_eq!(heap.deferred_allocations.capacity(), capacity);
    let next = heap.record_virtual_clone(heap.prepare_dead_clone(vt).unwrap()).unwrap();
    assert!(next.0 > ids.last().unwrap().0);
    assert_eq!(heap.deferred_allocations.capacity(), capacity);
    let charged = counts(&heap); heap.settle_dead_allocations(); assert_eq!(counts(&heap), charged);
    assert!(heap.materialize_recorded_clone(next).is_none());
    assert!(heap.deferred_index_rows_for_test().is_empty());
    report("clear-regrow-settle", &heap);
}

#[test]
fn numeric_only_failed_admission_and_exhaustion_never_allocate_index_rows() {
    let (mut heap, vt) = owner();
    let numeric = heap.prepare_numeric_batch(vt, vec![19]).unwrap();
    let snapshot = heap.prepare_dead_clone(vt).unwrap();
    for i in 0..MAX_DEAD_ALLOCATIONS {
        if i % 2 == 0 { heap.record_numeric_result(numeric.clone(), 0).unwrap(); }
        else { assert!(heap.record_dead_allocation(snapshot.clone())); }
    }
    assert_eq!(heap.deferred_allocations.capacity(), 0);
    let charged = counts(&heap);
    assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
    assert!(heap.record_numeric_result(numeric.clone(), 1).is_none());
    assert_eq!(counts(&heap), charged);
    assert_eq!(heap.deferred_allocations.capacity(), 0);
    heap.reclaim_dead_allocations();
    heap.set_deferred_identity_for_test(u64::MAX);
    assert!(heap.record_virtual_clone(snapshot).is_none());
    assert!(heap.deferred_index_rows_for_test().is_empty());
    assert_eq!(heap.deferred_allocations.capacity(), 0);
    report("refusals-unallocated", &heap);
}

#[test]
fn moved_owner_outlives_peer_and_clear_retains_only_native_capacity() {
    let (mut first, vt) = owner();
    let snapshot = first.prepare_dead_clone(vt).unwrap();
    let weak = std::rc::Rc::downgrade(&snapshot);
    let id = first.record_virtual_clone(snapshot).unwrap();
    let (mut second, other_vt) = owner();
    second.record_virtual_clone(second.prepare_dead_clone(other_vt).unwrap()).unwrap();
    let mut moved = vec![first].pop().unwrap(); drop(second);
    assert_eq!(moved.deferred_index_rows_for_test(), vec![(id.0, 0)]);
    assert!(weak.upgrade().is_some());
    moved.reclaim_dead_allocations(); assert!(weak.upgrade().is_none());
    assert!(moved.deferred_allocations.capacity() > 0);
    let identity = moved.record_virtual_clone(moved.prepare_dead_clone(vt).unwrap()).unwrap();
    assert_ne!(identity, id);
    assert!(moved.materialize_recorded_clone(id).is_none());
    report("moved-owner", &moved);
    let _object = moved.materialize_recorded_clone(identity).unwrap();
    moved.reclaim_dead_allocations();
    report("moved-recovered", &moved);
    drop(moved);
}
