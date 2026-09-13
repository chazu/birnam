//! Measurements use the actual source compiler/consumer and explicit GC batches.
use super::native_tier::closure_elision::{constructor_discoveries, constructor_queries, counters};
use super::*;
use std::time::Instant;

thread_local! { static EFFECTS: Cell<usize> = const { Cell::new(0) }; }

/// Real source factories; all assertions run outside any performance claim.
pub(super) unsafe extern "C" fn sequence(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 5);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let (factory, input, other) = (a[0], a[1], a[2]);
        let calls = r.integer_value(a[3]) as usize;
        let escaping = r.integer_value(a[4]) != 0;
        assert!((1..=33).contains(&calls));
        let selector = r.intern(b"value:value:").unwrap();
        let mut answer = r.canonical().nil;
        let invoke = || {
            let answer = r.send_slice(factory, selector, &[input, other]);
            if escaping {
                assert_eq!(
                    unsafe { (*r.state.get()).heap.read_payload_word(answer, 1) },
                    Some(input.addr())
                );
            } else {
                assert_eq!(answer, if (calls - 1) % 3 == 0 { input } else { other });
            }
            answer
        };
        for batch in 0..12 {
            for _ in 0..8 {
                answer = invoke();
            }
            if batch != 11 {
                r.gc_collect_from_native_range(0, 0);
            }
        }
        r.gc_collect_from_native_range(0, 0);
        let before = counters();
        let completed_before = super::native_tier::closure_elision::completed_calls();
        let allocated_before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        for _ in 0..8 {
            answer = invoke();
        }
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
        let bytes = heap.bytes_since_collection() - allocated_before;
        let saved = heap.allocation_count() - heap.physical_allocation_count();
        let after = counters();
        let completed = super::native_tier::closure_elision::completed_calls() - completed_before;
        println!(
            "closure-sequence calls={calls} escaping={} invocations=8 bytes={bytes} physical_saved={saved} attempts={} hits={} materialized={} completed_calls={completed}",
            usize::from(escaping),
            after.0 - before.0,
            after.1 - before.1,
            after.2 - before.2
        );
        if escaping {
            assert_eq!(
                r.send_slice(answer, r.intern(b"value:").unwrap(), &[other]),
                other
            );
        }
        r.canonical().nil
    })
}

pub(super) unsafe extern "C" fn prepare(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    with_active(|r| {
        let codegen = r.import(b"Codegen").unwrap();
        let selector = r.intern(b"materializeResumable:captures:").unwrap();
        let before = counters();
        let metadata_before = r.closure_metadata_payload_bytes();
        let start = Instant::now();
        let block = r.send_slice(codegen, selector, unsafe {
            core::slice::from_raw_parts(args, count)
        });
        let ns = start.elapsed().as_nanos();
        let after = counters();
        let metadata = r.closure_metadata_payload_bytes();
        println!(
            "closure-prepare ns={ns} floor_preparations={} floor_preparation_ns={} metadata_payload_before={metadata_before} metadata_payload_after={metadata}",
            after.3 - before.3,
            after.4 - before.4
        );
        block
    })
}

unsafe extern "C" fn replacement(closure: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    EFFECTS.set(EFFECTS.get() + 1);
    with_active(|r| unsafe {
        core::ptr::with_exposed_provenance_mut(
            (*r.state.get()).heap.read_payload_word(closure, 1).unwrap(),
        )
    })
}

pub(super) unsafe extern "C" fn measure(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let (block, input, shape, batches) =
        unsafe { (*args, *args.add(1), *args.add(2), *args.add(3)) };
    with_active(|r| {
        let shape = r.integer_value(shape);
        let batches = r.integer_value(batches) as usize;
        assert!((0..=3).contains(&shape) && (1..=500).contains(&batches));
        let uses = std::env::var("BIRNAM_CLOSURE_USES")
            .map(|v| v.parse::<usize>().expect("closure uses"))
            .unwrap_or(1);
        assert!((1..=32).contains(&uses));
        let selector = r.intern(b"value:").unwrap();
        let mut answer = r.canonical().nil;
        let invoke = || {
            let answer = r.send_slice(block, selector, &[input]);
            match shape {
                0 => assert_eq!(answer, input),
                1 | 2 => {
                    assert_ne!(answer, input);
                    assert_eq!(
                        unsafe { (*r.state.get()).heap.read_payload_word(answer, 1) },
                        Some(input.addr())
                    );
                }
                3 => assert_eq!(r.integer_value(answer), 73),
                _ => unreachable!(),
            }
            answer
        };
        let cold_start = Instant::now();
        for _ in 0..128 {
            answer = invoke();
        }
        let cold_ns = cold_start.elapsed().as_nanos();
        let replaced = if shape == 2 {
            let vt = unsafe { (*r.state.get()).heap.vtable_of(answer).unwrap() };
            let value = r.intern(b"value").unwrap();
            let method = unsafe { (*r.state.get()).vtables.lookup(vt, value).unwrap() };
            r.add_method(vt, value, replacement);
            Some((vt, value, method))
        } else {
            None
        };
        r.gc_collect_from_native_range(0, 0);
        let before = counters();
        let discoveries_before = constructor_discoveries();
        let queries_before = constructor_queries();
        let completed_before = super::native_tier::closure_elision::completed_calls();
        EFFECTS.set(0);
        let mut ns = 0;
        let mut gc_ns = 0;
        let mut allocation_bytes = 0;
        let mut physical_saved = 0;
        let mut journal_peak_bytes = 0;
        for _ in 0..batches {
            let allocation_start = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let start = Instant::now();
            for _ in 0..128 {
                invoke();
            }
            ns += start.elapsed().as_nanos();
            let heap = unsafe { &(*r.state.get()).heap };
            assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
            allocation_bytes += heap.bytes_since_collection() - allocation_start;
            physical_saved += heap.allocation_count() - heap.physical_allocation_count();
            journal_peak_bytes = journal_peak_bytes.max(heap.dead_allocation_storage_bytes());
            let start = Instant::now();
            r.gc_collect_from_native_range(0, 0);
            gc_ns += start.elapsed().as_nanos();
        }
        let after = counters();
        let discoveries = constructor_discoveries() - discoveries_before;
        let queries = constructor_queries() - queries_before;
        let calls = batches * 128;
        let attempts = after.0 - before.0;
        let hits = after.1 - before.1;
        let materialized = after.2 - before.2;
        let completed_calls =
            super::native_tier::closure_elision::completed_calls() - completed_before;
        let effects = EFFECTS.get();
        assert_eq!(effects, if shape == 2 { calls * uses } else { 0 });
        // Restore after the measurement snapshots. The first subsequent call
        // must regain admission, with no cooldown or stale native target.
        let mut restored_hits = 0;
        if let Some((vt, value, method)) = replaced {
            r.add_method(vt, value, method.to_method());
            let previous = counters();
            let queries_before_restore = constructor_queries();
            let completed_before_restore = super::native_tier::closure_elision::completed_calls();
            invoke();
            let restored = counters();
            restored_hits = restored.1 - previous.1;
            let expected = usize::from(attempts != 0);
            assert_eq!(restored_hits, expected);
            assert_eq!(restored.2 - previous.2, expected);
            assert_eq!(constructor_queries() - queries_before_restore, expected);
            assert_eq!(
                super::native_tier::closure_elision::completed_calls() - completed_before_restore,
                expected * uses
            );
            assert_eq!(EFFECTS.get(), effects);
        }
        println!(
            "closure-measure shape={shape} uses={uses} calls={calls} completed_calls={completed_calls} cold_ns={cold_ns} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} journal_peak_bytes={journal_peak_bytes} attempts={attempts} hits={hits} materialized={materialized} effects={effects} constructor_discoveries={discoveries} constructor_queries={queries} restored_hits={restored_hits}"
        );
        r.canonical().nil
    })
}
