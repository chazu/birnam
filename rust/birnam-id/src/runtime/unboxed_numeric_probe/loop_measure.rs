//! Test-floor costs of real Id loops; no production ABI or timing policy.
use super::*;
use crate::runtime::native_tier::FUSED_NUMERIC_HITS as HITS;
use crate::runtime::numeric::loop_reuse::PREDICTED_OPERATIONS_REUSED;
use std::time::Instant;

pub(super) unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 5);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let block = args[0];
        let operations = r.integer_value(args[2]);
        assert!(matches!(operations, 3 | 16 | 32));
        let shape = r.integer_value(args[3]) as usize;
        let batches = r.integer_value(args[4]) as usize;
        assert!(shape < 4 && (1..=500).contains(&batches));
        let _block = ScopedOopRootGuard::new(r, block);
        let _numeric = ScopedOopRootGuard::new(r, args[1]);
        let selector = r.intern(b"value:").unwrap();
        let expected = [
            11 * operations + 3,
            11 * operations + 127,
            (operations + 1) * 127,
            11 * operations + 3,
        ][shape];
        let invoke = || {
            let answer = r.send_slice(block, selector, &args[1..2]);
            assert!(r.is_integer(answer));
            assert_eq!(r.integer_value(answer), expected);
            answer
        };
        // First complete loop includes its hot kernel preparation. Materializing
        // CodeObjects is outside this interval and is not reported as cold time.
        let start = Instant::now();
        invoke();
        let cold_ns = start.elapsed().as_nanos();
        r.gc_collect_from_native_range(0, 0);
        for _ in 0..3 {
            invoke();
            r.gc_collect_from_native_range(0, 0);
        }
        let before = (
            PREDICTED_OPERATIONS_REUSED.get(),
            HITS.get(),
            COMPLETED.get(),
        );
        let resources = r.native_tier.borrow().loop_resources();
        let units = r.native_tier.borrow().unboxed_resources();
        let mut ns = 0;
        let mut gc_ns = 0;
        let mut allocation_bytes = 0;
        let mut physical_saved = 0;
        let mut journal_peak_bytes = 0;
        for _ in 0..batches {
            let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let start = Instant::now();
            let answer = invoke();
            ns += start.elapsed().as_nanos();
            let root = ScopedOopRootGuard::new(r, answer);
            let heap = unsafe { &(*r.state.get()).heap };
            assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
            allocation_bytes += heap.bytes_since_collection() - allocated;
            physical_saved += heap.allocation_count() - heap.physical_allocation_count();
            journal_peak_bytes = journal_peak_bytes.max(heap.dead_allocation_storage_bytes());
            let start = Instant::now();
            r.gc_collect_from_native_range(0, 0);
            gc_ns += start.elapsed().as_nanos();
            assert!(r.is_integer(answer));
            assert_eq!(r.integer_value(answer), expected);
            assert_eq!(
                r.native_tier.borrow().loop_resources(),
                resources,
                "loop owner/cache capacity stays bounded across runs and GC"
            );
            assert_eq!(
                r.native_tier.borrow().unboxed_resources(),
                units,
                "no new numeric/native units or recovery maps in steady loops"
            );
            drop(root);
        }
        let reused = PREDICTED_OPERATIONS_REUSED.get() - before.0;
        let fused = HITS.get() - before.1;
        let completed = COMPLETED.get() - before.2;
        let journal_retained_bytes =
            unsafe { (*r.state.get()).heap.dead_allocation_storage_bytes() };
        println!(
            "loop-measure operations={operations} shape={shape} iterations={} answer={expected} cold_ns={cold_ns} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} reused={reused} fused={fused} completed={completed} owners={} cached={} cache_bytes={} native_units={} numeric_units={} journal_peak_bytes={journal_peak_bytes} journal_retained_bytes={journal_retained_bytes}",
            batches * 128,
            resources.0,
            resources.1,
            resources.2,
            units.2,
            units.3
        );
        r.canonical().nil
    })
}
