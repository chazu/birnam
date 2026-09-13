//! Later-target mutation of real source closure regions, using the test floor.
use super::native_tier::closure_elision::{completed_calls, counters};
use super::*;

thread_local! {
    // Deliberately untraced: the active Call/Frame must own these values while
    // the replacement collects. Addresses here only check identity afterward.
    static INPUT: Cell<usize> = const { Cell::new(0) };
    static OTHER: Cell<usize> = const { Cell::new(0) };
    static OBSERVED: Cell<usize> = const { Cell::new(0) };
    static EFFECTS: Cell<usize> = const { Cell::new(0) };
    static COLLECT: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "C" fn prime(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    unsafe { *args }
}

unsafe extern "C" fn replacement(closure: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    with_active(|r| {
        let capture = || unsafe { (*r.state.get()).heap.read_payload_word(closure, 1) };
        assert_eq!(capture(), Some(INPUT.get()));
        OBSERVED.set(closure.addr());
        EFFECTS.set(EFFECTS.get() + 1);
        if COLLECT.get() {
            r.gc_collect_from_native_range(0, 0);
        }
        assert_eq!(capture(), Some(INPUT.get()));
        assert_eq!(unsafe { (*args).addr() }, OTHER.get());
        unsafe { *args }
    })
}

pub(super) unsafe extern "C" fn check(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 5);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let (factory, input, other) = (a[0], a[1], a[2]);
        let position = r.integer_value(a[3]) as usize;
        let collecting = r.integer_value(a[4]) != 0;
        assert!((1..=2).contains(&position));
        let invoke_selector = r.intern(b"value:value:").unwrap();
        let alternate = r.intern(b"closureSequenceTail:").unwrap();
        let object_vt = r.globals().object_vt;
        // This test uses a private fresh selector. Never overwrite an existing
        // inherited or direct binding just to bootstrap the source factory.
        assert!(r.lookup_method(object_vt, alternate).is_none());
        r.add_method(object_vt, alternate, prime);
        let invoke = || r.send_slice(factory, invoke_selector, &[input, other]);
        let first = invoke();
        let _first = ScopedOopRootGuard::new(r, first);
        let vt = unsafe { (*r.state.get()).heap.vtable_of(first).unwrap() };
        let value = r.intern(b"value:").unwrap();
        let method = r.lookup_method(vt, value).unwrap();
        assert!(unsafe { (*r.state.get()).vtables.direct_lookup(vt, alternate) }.is_none());
        r.add_method(vt, alternate, method.to_method());
        assert_eq!(
            r.remove_method(object_vt, alternate),
            Some(MethodAddress::from_method(prime))
        );
        for _ in 0..128 {
            invoke();
        }
        r.add_method(vt, alternate, replacement);
        INPUT.set(input.addr());
        OTHER.set(other.addr());
        EFFECTS.set(0);
        COLLECT.set(collecting);
        r.gc_collect_from_native_range(0, 0);
        let before = counters();
        let completed_before = completed_calls();
        let allocated_before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        let mut previous = first;
        for _ in 0..8 {
            // Keep the previous result alive so address reuse cannot masquerade
            // as shared identity. The current recovered closure has no such root.
            let _previous = ScopedOopRootGuard::new(r, previous);
            OBSERVED.set(0);
            let answer = invoke();
            assert_eq!(answer.addr(), OBSERVED.get());
            assert_ne!(answer, previous);
            assert_eq!(
                unsafe { (*r.state.get()).heap.read_payload_word(answer, 1) },
                Some(input.addr())
            );
            previous = answer;
        }
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
        // bytes_since_collection resets during the callback, so only the
        // non-collecting matrix makes an exact allocation-charge comparison.
        let bytes = if collecting {
            0
        } else {
            heap.bytes_since_collection() - allocated_before
        };
        let saved = heap.allocation_count() - heap.physical_allocation_count();
        let after = counters();
        let completed = completed_calls() - completed_before;
        assert_eq!(EFFECTS.get(), 8);
        let _previous = ScopedOopRootGuard::new(r, previous);
        r.add_method(vt, alternate, method.to_method());
        let restored_before = counters();
        let restored_completed_before = completed_calls();
        let restored = invoke();
        assert_ne!(restored, previous);
        assert_eq!(
            unsafe { (*r.state.get()).heap.read_payload_word(restored, 1) },
            Some(input.addr())
        );
        assert_eq!(EFFECTS.get(), 8);
        let restored_after = counters();
        println!(
            "closure-sequence-mutation position={position} collecting={} invocations=8 bytes={bytes} physical_saved={saved} attempts={} hits={} materialized={} completed_calls={completed} effects=8 restored_hits={} restored_materialized={} restored_calls={}",
            usize::from(collecting),
            after.0 - before.0,
            after.1 - before.1,
            after.2 - before.2,
            restored_after.1 - restored_before.1,
            restored_after.2 - restored_before.2,
            completed_calls() - restored_completed_before,
        );
        r.remove_method(vt, alternate);
        COLLECT.set(false);
        r.canonical().nil
    })
}
