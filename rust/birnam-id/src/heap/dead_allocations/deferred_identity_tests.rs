//! Run unchanged against the original scalar and extracted cell.
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
fn report(name: &str, heap: &Heap) {
    let mut physical = heap.blocks.values().map(|b| (b.identity(), b.total_size, b.metadata)).collect::<Vec<_>>();
    physical.sort_unstable();
    let handles = heap.deferred_allocations.iter().map(|(id, index)| (id.0, index)).collect::<Vec<_>>();
    println!("deferred-identity {name}: counts={:?} physical={physical:?} clones={handles:?} numeric={}", counts(heap), heap.numeric_allocations.len());
}

#[test]
fn mixed_journals_recover_exact_bits_once_without_recharging() {
    let (mut heap, vt) = owner();
    let bytes = (1..=13).collect::<Vec<u8>>();
    let snapshot = heap.prepare_virtual_bytes(vt, &bytes).unwrap();
    let batch = heap.prepare_numeric_batch(vt, vec![i64::MIN, 17, 17]).unwrap();
    let weak = std::rc::Rc::downgrade(&batch);
    let a = heap.record_virtual_clone(snapshot.clone()).unwrap();
    let b = heap.record_numeric_result(batch.clone(), 0).unwrap();
    let c = heap.record_virtual_clone(snapshot.clone()).unwrap();
    let d = heap.record_numeric_result(batch.clone(), 1).unwrap();
    let e = heap.record_numeric_result(batch.clone(), 1).unwrap();
    assert_eq!([a.0, b.0, c.0, d.0, e.0], [0, 1, 2, 3, 4]);
    assert!(heap.record_dead_allocation(snapshot));
    drop(batch);
    let charged = counts(&heap);
    assert_eq!(charged, (7, 128, 128));
    report("mixed-recorded", &heap);
    let mut objects = Vec::new();
    for (id, numeric) in [(c, None), (d, Some(17)), (a, None), (e, Some(17)), (b, Some(i64::MIN))] {
        let object = heap.materialize_recorded_clone(id).unwrap();
        assert!(!objects.contains(&object));
        objects.push(object);
        assert_eq!(heap.vtable_of(object), Some(vt));
        if let Some(value) = numeric {
            assert_eq!(heap.read_payload_word(object, 0), Some(value as usize));
        } else {
            let payload = unsafe { core::slice::from_raw_parts(object.cast::<u8>(), 16) };
            assert_eq!(&payload[..13], bytes.as_slice());
            assert_eq!(&payload[13..], &[0, 0, 0]);
        }
        assert!(heap.materialize_recorded_clone(id).is_none());
        assert_eq!(counts(&heap), charged);
        report(&format!("recovered-{}", id.0), &heap);
    }
    assert!(weak.upgrade().is_none());
    assert_eq!(heap.reclaim_dead_allocations(), 1);
    let next = heap.record_virtual_clone(heap.prepare_dead_clone(vt).unwrap()).unwrap();
    assert_eq!(next.0, 5);
    let charged = counts(&heap);
    heap.settle_dead_allocations();
    assert_eq!(counts(&heap), charged);
    assert!(heap.materialize_recorded_clone(next).is_none());
    assert_eq!(heap.next_allocation_identity.get(), 7);
    report("settled", &heap);
}

#[test]
fn capacity_invalid_indices_and_stale_batches_preserve_issuance() {
    let (mut heap, vt) = owner();
    let batch = heap.prepare_numeric_batch(vt, vec![41]).unwrap();
    let snapshot = heap.prepare_dead_clone(vt).unwrap();
    for i in 0..MAX_DEAD_ALLOCATIONS {
        if i % 2 == 0 {
            assert_eq!(heap.record_numeric_result(batch.clone(), 0).unwrap().0, (i / 2) as u64);
        } else {
            assert!(heap.record_dead_allocation(snapshot.clone()));
        }
    }
    let full = counts(&heap);
    assert!(heap.admits_virtual_allocations(0));
    assert!(!heap.admits_virtual_allocations(1));
    assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
    assert!(heap.record_numeric_result(batch.clone(), 0).is_none());
    assert_eq!(counts(&heap), full);
    assert_eq!(heap.reclaim_dead_allocations(), MAX_DEAD_ALLOCATIONS);
    let before = counts(&heap);
    assert!(heap.record_numeric_result(batch.clone(), 1).is_none());
    assert_eq!(counts(&heap), before);
    let stale = heap.record_virtual_clone(snapshot.clone()).unwrap();
    assert_eq!(stale.0, 2048);
    heap.reclaim_dead_allocations();
    let current = heap.record_numeric_result(batch.clone(), 0).unwrap();
    assert_eq!(current.0, 2049);
    assert!(heap.materialize_recorded_clone(stale).is_none());
    heap.settle_dead_allocations();
    assert!(heap.materialize_recorded_clone(current).is_none());
    heap.reset_allocation_bytes();
    let next = heap.record_numeric_result(batch, 0).unwrap();
    assert_eq!(next.0, 2050);
    assert_eq!(heap.next_allocation_identity.get(), 2);
    report("capacity-and-stale", &heap);
}

#[test]
fn last_handle_and_refused_successor_keep_unhandled_work_admissible() {
    for numeric in [false, true] {
        let (mut heap, vt) = owner();
        let batch = heap.prepare_numeric_batch(vt, vec![7]).unwrap();
        let snapshot = heap.prepare_dead_clone(vt).unwrap();
        heap.set_deferred_identity_for_test(u64::MAX - 1);
        assert!(heap.admits_virtual_allocations(1));
        assert!(!heap.admits_virtual_allocations(2));
        let last = if numeric { heap.record_numeric_result(batch.clone(), 0) } else { heap.record_virtual_clone(snapshot.clone()) }.unwrap();
        assert_eq!(last.0, u64::MAX - 1);
        let before = counts(&heap);
        assert!(heap.admits_virtual_allocations(0));
        assert!(!heap.admits_virtual_allocations(1));
        assert!(heap.record_virtual_clone(snapshot.clone()).is_none());
        assert!(heap.record_numeric_result(batch, 0).is_none());
        assert_eq!(counts(&heap), before);
        let object = heap.materialize_recorded_clone(last).unwrap();
        assert_eq!(heap.vtable_of(object), Some(vt));
        assert!(heap.materialize_recorded_clone(last).is_none());
        assert_eq!(counts(&heap), before);
        assert!(heap.record_dead_allocation(snapshot));
        assert_eq!(heap.reclaim_dead_allocations(), 1);
        report(if numeric { "last-numeric" } else { "last-clone" }, &heap);
    }
}

#[test]
fn moved_owners_start_at_zero_and_do_not_reset_a_surviving_peer() {
    let (mut first, vt) = owner();
    let snapshot = first.prepare_dead_clone(vt).unwrap();
    assert_eq!(first.record_virtual_clone(snapshot.clone()).unwrap().0, 0);
    let (mut second, vt2) = owner();
    let batch = second.prepare_numeric_batch(vt2, vec![9]).unwrap();
    assert_eq!(second.record_numeric_result(batch.clone(), 0).unwrap().0, 0);
    let mut moved = vec![first].pop().unwrap();
    drop(second);
    drop(batch);
    assert_eq!(moved.record_virtual_clone(snapshot.clone()).unwrap().0, 1);
    assert_eq!(moved.reclaim_dead_allocations(), 2);
    assert_eq!(moved.record_virtual_clone(snapshot).unwrap().0, 2);
    report("moved-owner", &moved);
    drop(moved);
    let (mut fresh, vt3) = owner();
    let snapshot = fresh.prepare_dead_clone(vt3).unwrap();
    assert_eq!(fresh.record_virtual_clone(snapshot).unwrap().0, 0);
    report("fresh-owner", &fresh);
}
