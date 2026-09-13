//! Measurements use the actual source compiler/consumer and explicit GC batches.
use super::native_tier::closure_elision::counters;
use super::*;
use std::time::Instant;

thread_local! { static EFFECTS: Cell<usize> = const { Cell::new(0) }; }

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
        if shape == 2 {
            let vt = unsafe { (*r.state.get()).heap.vtable_of(answer).unwrap() };
            r.add_method(vt, r.intern(b"value").unwrap(), replacement);
        }
        r.gc_collect_from_native_range(0, 0);
        let before = counters();
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
        let calls = batches * 128;
        let attempts = after.0 - before.0;
        let hits = after.1 - before.1;
        let materialized = after.2 - before.2;
        let effects = EFFECTS.get();
        assert_eq!(effects, if shape == 2 { calls } else { 0 });
        println!(
            "closure-measure shape={shape} calls={calls} cold_ns={cold_ns} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} journal_peak_bytes={journal_peak_bytes} attempts={attempts} hits={hits} materialized={materialized} effects={effects}"
        );
        r.canonical().nil
    })
}
