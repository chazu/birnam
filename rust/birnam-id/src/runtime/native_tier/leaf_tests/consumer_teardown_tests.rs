//! Complete Runtime/consumer destruction, without counter resets or process exit.
//! Run each ignored test in its own single-test process: TinyCC totals are global
//! to this linked compiler, and other concurrent compiler tests invalidate them.
use super::*;

fn repeat(family: &str, exercise: fn()) {
    let baseline = Unit::resource_totals();
    let heap_baseline = Unit::compiler_heap_totals();
    assert_eq!(baseline, (0, 0), "isolated consumer process required");
    assert_eq!(heap_baseline.blocks, 0);
    assert_eq!(heap_baseline.requested_bytes, 0);
    for cycle in 0..3 {
        // These are the production-driver fixtures, including their positive
        // optimizer activity, result/identity, mutation, GC and recovery checks.
        // Returning destroys all their Runtime registries, selected owners,
        // temporary native fixtures and baseline/candidate Units normally.
        exercise();
        let resources = Unit::resource_totals();
        let heap = Unit::compiler_heap_totals();
        assert_eq!(resources, baseline, "{family} cycle={cycle} native owners");
        assert_eq!(heap, heap_baseline, "{family} cycle={cycle} compiler heap");
        println!(
            "consumer-teardown family={family} cycle={cycle} native_units={} relocated_bytes={} heap_blocks={} heap_bytes={}",
            resources.0, resources.1, heap.blocks, heap.requested_bytes
        );
    }
}

#[test]
#[ignore = "requires an isolated single-test process; run consumer-teardown-smoke.py"]
fn unboxed_numeric() {
    repeat("unboxed_numeric", unboxed_numeric_tests::unboxed_numeric_driver_saves_results_and_recovers_each_original_boundary);
}

#[test]
#[ignore = "requires an isolated single-test process; run consumer-teardown-smoke.py"]
fn loop_invariants() {
    repeat("loop_invariants", loop_invariant_tests::loop_invariant_numeric_work_preserves_inputs_methods_lookup_gc_and_fresh_results);
}

#[test]
#[ignore = "requires an isolated single-test process; run consumer-teardown-smoke.py"]
fn discarded_clones() {
    repeat("discarded_clones", discarded_clone_tests::discarded_clones_preserve_counts_identity_boundaries_and_late_binding);
}

#[test]
#[ignore = "requires an isolated single-test process; run consumer-teardown-smoke.py"]
fn virtual_fields() {
    repeat("virtual_fields", virtual_aggregate_tests::longer_virtual_field_regions_recover_at_every_call_and_preserve_reentry);
}
