//! Test-only cost and counter measurements for real installed source Blocks.
use super::*;
use crate::runtime::native_tier::unboxed_numeric::{PREP_ATTEMPTS, PREP_NS, PREPARATIONS};
use std::time::Instant;

pub(super) unsafe extern "C" fn prepare(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let _code = ScopedOopRootGuard::new(r, args[0]);
        let _captures = ScopedOopRootGuard::new(r, args[1]);
        let codegen = r.import(b"Codegen").unwrap();
        let selector = r.intern(b"materializeResumable:captures:").unwrap();
        let before = r.native_tier.borrow().unboxed_resources();
        let counters = (PREP_ATTEMPTS.get(), PREPARATIONS.get(), PREP_NS.get());
        let start = Instant::now();
        let block = r.send_slice(codegen, selector, args);
        let ns = start.elapsed().as_nanos();
        let after = r.native_tier.borrow().unboxed_resources();
        println!(
            "unboxed-prepare ns={ns} attempts={} preparations={} preparation_ns={} maps_before={} maps_after={} metadata_before={} metadata_after={}",
            PREP_ATTEMPTS.get() - counters.0,
            PREPARATIONS.get() - counters.1,
            PREP_NS.get() - counters.2,
            before.0,
            after.0,
            before.1,
            after.1
        );
        block
    })
}

pub(super) unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 6);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let (block, input, other) = (args[0], args[1], args[2]);
        let operations = r.integer_value(args[3]) as usize;
        let shape = r.integer_value(args[4]) as usize;
        let batches = r.integer_value(args[5]) as usize;
        assert!(matches!(operations, 3 | 16 | 32) && shape <= 4 && (1..=500).contains(&batches));
        let _block = ScopedOopRootGuard::new(r, block);
        let _input = ScopedOopRootGuard::new(r, input);
        let _other = ScopedOopRootGuard::new(r, other);
        let selector = r.intern(b"value:value:").unwrap();
        let value = r.intern(b"value").unwrap();
        let invoke = |changed: bool| {
            let answer = r.send_slice(block, selector, &[input, other]);
            assert_ne!(answer, input);
            assert_ne!(answer, other);
            if shape == 2 {
                assert!(r.is_float(answer));
                assert_eq!(r.float_value(answer), 11.5 + 3.0 * operations as f64);
            } else {
                let numeric = if shape == 3 {
                    r.send_slice(answer, value, &[])
                } else {
                    answer
                };
                let expected = if shape == 4 {
                    14
                } else if changed {
                    11 - 3 * operations as i64
                } else {
                    11 + 3 * operations as i64
                };
                assert_eq!(r.integer_value(numeric), expected);
            }
            answer
        };
        // Measure the first 128 actual invocations, including hot preparation.
        let start = Instant::now();
        for _ in 0..128 {
            invoke(false);
        }
        let cold_ns = start.elapsed().as_nanos();
        let replaced = if shape == 1 {
            let vt = r.canonical().smallint_vt;
            let add = r.intern(b"+").unwrap();
            let subtract = r.intern(b"-").unwrap();
            let (original, replacement) = unsafe {
                let vtables = &(*r.state.get()).vtables;
                (
                    vtables.lookup(vt, add).unwrap(),
                    vtables.lookup(vt, subtract).unwrap(),
                )
            };
            r.add_method(vt, add, replacement.to_method());
            for _ in 0..128 {
                invoke(true);
            }
            Some((vt, add, original))
        } else {
            None
        };
        r.gc_collect_from_native_range(0, 0);
        let before = (COMPLETED.get(), MATERIALIZED.get(), PREPARATIONS.get());
        let resources = r.native_tier.borrow().unboxed_resources();
        let scratch = r.native_tier.borrow().unboxed_scratch_resources();
        let native_storage = birnam_native_compiler::Unit::resource_totals();
        let (private_native_units, private_relocated_bytes) = native_storage;
        let mut ns = 0;
        let mut gc_ns = 0;
        let mut allocation_bytes = 0;
        let mut physical_saved = 0;
        let mut journal_peak_bytes = 0;
        for _ in 0..batches {
            let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let start = Instant::now();
            for _ in 0..128 {
                invoke(shape == 1);
            }
            ns += start.elapsed().as_nanos();
            let heap = unsafe { &(*r.state.get()).heap };
            assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
            allocation_bytes += heap.bytes_since_collection() - allocated;
            physical_saved += heap.allocation_count() - heap.physical_allocation_count();
            journal_peak_bytes = journal_peak_bytes.max(heap.dead_allocation_storage_bytes());
            let start = Instant::now();
            r.gc_collect_from_native_range(0, 0);
            gc_ns += start.elapsed().as_nanos();
            assert_eq!(
                birnam_native_compiler::Unit::resource_totals(),
                native_storage,
                "private native ownership and relocated storage stay stable after collection"
            );
            assert_eq!(
                r.native_tier.borrow().unboxed_scratch_resources(),
                scratch,
                "scratch capacity and returned leases stay bounded after collection"
            );
        }
        let completed = COMPLETED.get() - before.0;
        let materialized = MATERIALIZED.get() - before.1;
        assert_eq!(
            PREPARATIONS.get(),
            before.2,
            "no new maps in steady execution"
        );
        assert_eq!(
            r.native_tier.borrow().unboxed_resources(),
            resources,
            "bounded retained maps/native owners"
        );
        let journal_retained_bytes =
            unsafe { (*r.state.get()).heap.dead_allocation_storage_bytes() };
        let mut restored_completed = 0;
        if let Some((vt, selector, original)) = replaced {
            r.add_method(vt, selector, original.to_method());
            let before = COMPLETED.get();
            invoke(false);
            restored_completed = COMPLETED.get() - before;
        }
        let calls = batches * 128;
        println!(
            "unboxed-measure operations={operations} shape={shape} calls={calls} cold_ns={cold_ns} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} completed={completed} materialized={materialized} restored_completed={restored_completed} maps={} metadata_bytes={} native_units={} numeric_units={} journal_peak_bytes={journal_peak_bytes} journal_retained_bytes={journal_retained_bytes} scratch_cached={} scratch_bytes={} private_native_units={private_native_units} private_relocated_bytes={private_relocated_bytes}",
            resources.0, resources.1, resources.2, resources.3, scratch.0, scratch.1
        );
        r.canonical().nil
    })
}
