//! Measurements use the actual source compiler/consumer and explicit GC batches.
use super::native_tier::closure_elision::counters;
use super::*;
use std::time::Instant;

thread_local! { static EFFECTS: Cell<usize> = const { Cell::new(0) }; }
thread_local! {
    static CLEANUP_CALLBACKS: Cell<usize> = const { Cell::new(0) };
    static CLEANUP_OBSERVATION: Cell<(usize, usize, usize, usize)> = const { Cell::new((0, 0, 0, 0)) };
}

unsafe extern "C" fn observing_string_cleanup(object: Oop) {
    let cleanup = with_active(|r| {
        let heap = unsafe { &(*r.state.get()).heap };
        let remaining = heap.dead_collectable_addresses();
        let remaining_bytes = remaining
            .iter()
            .map(|address| {
                let object = core::ptr::with_exposed_provenance_mut(*address);
                Heap::allocation_total_size(heap.payload_size(object).unwrap()).unwrap()
            })
            .sum();
        CLEANUP_OBSERVATION.set((
            r.gc_live_count(),
            r.gc_live_bytes(),
            remaining.len(),
            remaining_bytes,
        ));
        assert!(
            !unsafe { &(*r.state.get()).heap }.has_dead_allocations(),
            "observing cleanup must see settled allocations"
        );
        r.string_finalizer()
    });
    CLEANUP_CALLBACKS.set(CLEANUP_CALLBACKS.get() + 1);
    unsafe { cleanup(object) };
}

/// Real source factories; all assertions run outside any performance claim.
pub(super) unsafe extern "C" fn sequence(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 5);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_source_probe_native_roots(|r| {
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
                r.gc_collect_for_source_probe();
            }
        }
        r.gc_collect_for_source_probe();
        let cleanup_mode = std::env::var("BIRNAM_TEST_CLOSURE_STRING_CLEANUP")
            .ok()
            .map(|value| value.parse::<usize>().expect("cleanup mode"));
        let cleanup_string = cleanup_mode.map(|mode| {
            assert!((1..=3).contains(&mode));
            r.make_string_with_finalizer(
                b"compiled closure cleanup interaction",
                if mode == 2 {
                    observing_string_cleanup
                } else {
                    r.string_finalizer()
                },
            )
        });
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
        let counted_indices = heap.counted_frame_index_count();
        let after = counters();
        let completed = super::native_tier::closure_elision::completed_calls() - completed_before;
        println!(
            "closure-sequence calls={calls} escaping={} invocations=8 bytes={bytes} physical_saved={saved} counted_indices={counted_indices} attempts={} hits={} materialized={} completed_calls={completed}",
            usize::from(escaping),
            after.0 - before.0,
            after.1 - before.1,
            after.2 - before.2
        );
        // This new collection deliberately tests a retained returned value.
        // Only that actual result gains an explicit root; scratch String storage
        // stays outside the established outer native-root interval.
        let _answer_root = cleanup_mode.map(|_| ScopedOopRootGuard::new(r, answer));
        if let (Some(mode), Some(string)) = (cleanup_mode, cleanup_string) {
            if mode == 3 {
                r.set_finalizer(string, observing_string_cleanup);
            }
            CLEANUP_CALLBACKS.set(0);
            CLEANUP_OBSERVATION.set((0, 0, 0, 0));
            let pending = saved;
            let before_live = r.gc_live_count();
            let before_bytes = r.gc_live_bytes();
            let settled_before = Heap::settled_allocation_count_for_test();
            let reclaimed = r.gc_collect_for_source_probe();
            let settled = Heap::settled_allocation_count_for_test().wrapping_sub(settled_before);
            assert_eq!(settled, if mode == 1 { 0 } else { pending });
            assert_eq!(CLEANUP_CALLBACKS.get(), usize::from(mode != 1));
            assert!(
                r.string_bytes(string).is_none(),
                "owned String storage must be released"
            );
            let heap = unsafe { &(*r.state.get()).heap };
            assert!(heap.payload_size(string).is_none());
            assert!(!heap.has_dead_allocations());
            assert_eq!(r.gc_live_count(), before_live - reclaimed);
            if escaping {
                assert_eq!(heap.read_payload_word(answer, 1), Some(input.addr()));
            } else {
                assert_eq!(answer, if (calls - 1) % 3 == 0 { input } else { other });
            }
            println!(
                "closure-cleanup mode={mode} pending={pending} settled={settled} before_live={before_live} before_bytes={before_bytes} reclaimed={reclaimed} after_live={} after_bytes={} callbacks={} callback_live={} callback_bytes={} remaining_dead={} remaining_dead_bytes={} string_released=1",
                r.gc_live_count(),
                r.gc_live_bytes(),
                CLEANUP_CALLBACKS.get(),
                CLEANUP_OBSERVATION.get().0,
                CLEANUP_OBSERVATION.get().1,
                CLEANUP_OBSERVATION.get().2,
                CLEANUP_OBSERVATION.get().3
            );
        }
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
    with_source_probe_native_roots(|r| {
        let codegen = r.import(b"Codegen").unwrap();
        let selector = r.intern(b"materializeResumable:captures:").unwrap();
        let before = counters();
        let metadata_before = r.closure_metadata_payload_bytes();
        let pending_before = r.closure_pending_resources();
        let start = Instant::now();
        let block = r.send_slice(codegen, selector, unsafe {
            core::slice::from_raw_parts(args, count)
        });
        let ns = start.elapsed().as_nanos();
        let after = counters();
        let metadata = r.closure_metadata_payload_bytes();
        let pending = r.closure_pending_resources();
        println!(
            "closure-prepare ns={ns} floor_preparations={} floor_preparation_ns={} metadata_payload_before={metadata_before} metadata_payload_after={metadata} pending_records_before={} pending_bytes_before={} pending_records_after={} pending_bytes_after={}",
            after.3 - before.3,
            after.4 - before.4,
            pending_before.0,
            pending_before.1,
            pending.0,
            pending.1
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
    with_source_probe_native_roots(|r| {
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
        let cold_preparations_before = counters().3;
        let cold_pending_before = r.closure_pending_resources();
        let cold_start = Instant::now();
        for _ in 0..128 {
            answer = invoke();
        }
        let cold_ns = cold_start.elapsed().as_nanos();
        let cold_preparations = counters().3 - cold_preparations_before;
        let cold_pending_after = r.closure_pending_resources();
        let replaced = if shape == 2 {
            let vt = unsafe { (*r.state.get()).heap.vtable_of(answer).unwrap() };
            let value = r.intern(b"value").unwrap();
            let method = unsafe { (*r.state.get()).vtables.lookup(vt, value).unwrap() };
            r.add_method(vt, value, replacement);
            Some((vt, value, method))
        } else {
            None
        };
        r.gc_collect_for_source_probe();
        let before = counters();
        let completed_before = super::native_tier::closure_elision::completed_calls();
        let prediction_kernels_before = super::numeric::ENTRY_PREDICTION_KERNELS.get();
        let pending = r.closure_pending_resources();
        let native_storage = birnam_native_compiler::Unit::resource_totals();
        let (private_native_units, private_relocated_bytes) = native_storage;
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
            r.gc_collect_for_source_probe();
            gc_ns += start.elapsed().as_nanos();
            assert_eq!(
                birnam_native_compiler::Unit::resource_totals(),
                native_storage,
                "private native ownership and relocated storage stay stable after collection"
            );
            assert_eq!(
                r.closure_pending_resources(),
                pending,
                "pending inputs stay bounded during steady execution"
            );
        }
        let after = counters();
        assert_eq!(after.3, before.3, "warmed execution derives no new plans");
        let calls = batches * 128;
        let attempts = after.0 - before.0;
        let hits = after.1 - before.1;
        let materialized = after.2 - before.2;
        let completed_calls =
            super::native_tier::closure_elision::completed_calls() - completed_before;
        let effects = EFFECTS.get();
        let entry_prediction_kernels =
            super::numeric::ENTRY_PREDICTION_KERNELS.get() - prediction_kernels_before;
        assert_eq!(effects, if shape == 2 { calls * uses } else { 0 });
        // Restore after the measurement snapshots. The first subsequent call
        // must regain admission, with no cooldown or stale native target.
        let mut restored_hits = 0;
        if let Some((vt, value, method)) = replaced {
            r.add_method(vt, value, method.to_method());
            let previous = counters();
            let completed_before_restore = super::native_tier::closure_elision::completed_calls();
            invoke();
            let restored = counters();
            restored_hits = restored.1 - previous.1;
            let expected = usize::from(attempts != 0);
            assert_eq!(restored_hits, expected);
            assert_eq!(restored.2 - previous.2, expected);
            assert_eq!(
                super::native_tier::closure_elision::completed_calls() - completed_before_restore,
                expected * uses
            );
            assert_eq!(EFFECTS.get(), effects);
        }
        println!(
            "closure-measure shape={shape} uses={uses} calls={calls} completed_calls={completed_calls} entry_prediction_kernels={entry_prediction_kernels} cold_ns={cold_ns} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} journal_peak_bytes={journal_peak_bytes} attempts={attempts} hits={hits} materialized={materialized} effects={effects} restored_hits={restored_hits} cold_floor_preparations={cold_preparations} cold_pending_records_before={} cold_pending_bytes_before={} cold_pending_records_after={} cold_pending_bytes_after={} pending_records={} pending_bytes={} private_native_units={private_native_units} private_relocated_bytes={private_relocated_bytes}",
            cold_pending_before.0,
            cold_pending_before.1,
            cold_pending_after.0,
            cold_pending_after.1,
            pending.0,
            pending.1
        );
        r.canonical().nil
    })
}
