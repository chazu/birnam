//! The same heap-path witness runs against the preserved pre-extraction source.
use super::*;
fn counts(heap: &Heap) -> (usize, usize, usize, usize) {
    (
        heap.physical_allocation_count(),
        heap.allocation_count(),
        heap.allocated_bytes(),
        heap.bytes_since_collection(),
    )
}
fn seed(heap: &mut Heap, bytes: usize) {
    heap.budget.replace(bytes);
}
fn check(heap: &Heap, stage: &str, expected: (usize, usize, usize, usize)) {
    assert_eq!(counts(heap), expected);
    println!("\nbudget {stage}: {expected:?}");
}
#[test]
fn allocation_budget_transcript() {
    let mut owners = vec![Heap::new(), Heap::new()];
    let mut heap = owners.remove(0);
    assert_eq!(counts(&heap), (0, 0, 0, 0));
    assert_eq!(counts(&owners[0]), (0, 0, 0, 0));
    let vt = heap.allocate(0, Permanence::Permanent);
    heap.set_vtable(vt, vt);
    let object = heap.allocate(9, Permanence::Collectable);
    heap.set_vtable(object, vt);
    let snapshot = heap.prepare_dead_clone(object).unwrap();
    let batch = heap.prepare_numeric_batch(vt, vec![3, 7]).unwrap();
    let mode = std::env::var("BIRNAM_BUDGET_HEAP_CASE").unwrap_or_default();
    if !mode.is_empty() && mode != "normal" {
        seed(&mut heap, usize::MAX);
        match mode.as_str() {
            "physical" => {
                heap.allocate(0, Permanence::Collectable);
            }
            "snapshot" => {
                heap.record_dead_allocation(snapshot);
            }
            "numeric" => {
                heap.record_numeric_result(batch, 0);
            }
            _ => panic!("unknown budget case"),
        }
        panic!("expected byte-counter overflow");
    }
    check(&heap, "physical", (2, 2, 32, 32));
    assert!(heap.record_dead_allocation(snapshot.clone()));
    check(&heap, "dead", (2, 3, 56, 56));
    let deferred = heap.record_virtual_clone(snapshot).unwrap();
    check(&heap, "virtual", (2, 4, 80, 80));
    heap.record_numeric_result(batch, 0).unwrap();
    check(&heap, "numeric", (2, 5, 96, 96));
    assert!(heap.record_dead_frame_indices(vt, &[0, 1, 1]));
    check(&heap, "counted", (2, 8, 144, 144));
    assert!(heap.materialize_recorded_clone(deferred).is_some());
    assert!(heap.materialize_recorded_clone(deferred).is_none());
    check(&heap, "materialized-one", (3, 8, 144, 144));
    heap.settle_dead_allocations();
    check(&heap, "settled", (8, 8, 144, 144));
    assert!(heap.reclaim_for_test(object));
    check(&heap, "reclaimed", (7, 7, 120, 144));
    heap.reset_allocation_bytes();
    check(&heap, "reset", (7, 7, 120, 0));
    let snapshot = heap.prepare_dead_clone(vt).unwrap();
    seed(&mut heap, usize::MAX - snapshot.total_size());
    assert!(heap.record_dead_allocation(snapshot));
    heap.settle_dead_allocations();
    check(&heap, "max-settled", (8, 8, 128, usize::MAX));
    assert!(heap.record_dead_frame_indices(vt, &[]));
    assert!(!heap.record_dead_frame_indices(vt, &[0]));
    check(&heap, "preview-refused", (8, 8, 128, usize::MAX));
    assert_eq!(counts(&owners[0]), (0, 0, 0, 0));
    drop(heap);
    owners[0].allocate(0, Permanence::Permanent);
    check(&owners[0], "other-owner", (1, 1, 8, 8));
}
#[test]
fn overflow_paths_preserve_the_original_fatal_diagnostic() {
    use std::os::unix::process::ExitStatusExt;
    for mode in ["physical", "snapshot", "numeric"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "heap::allocation_budget_tests::allocation_budget_transcript",
                "--nocapture",
            ])
            .env("BIRNAM_BUDGET_HEAP_CASE", mode)
            .output()
            .unwrap();
        assert_eq!(output.status.signal(), Some(6), "{mode}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "birnam-id invariant failed: GC allocation byte counter overflow\n"
        );
    }
}
