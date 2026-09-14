//! Observe source-created dead closures only after their native stack unwinds.
use super::native_tier::closure_elision::{completed_calls, counters};
use super::*;
use core::cell::RefCell;

#[derive(Default)]
struct State {
    input: usize,
    child: usize,
    observing: bool,
    base_count: usize,
    base_bytes: usize,
    mark_count: usize,
    mark_bytes: usize,
    marks: usize,
    finalized: usize,
}
thread_local! {
    // Host scalars are intentionally not registered roots. Loading the weak
    // identities for inspection happens only after collection has returned.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

unsafe extern "C" fn mark_observer(_: crate::abi::MarkFn) {
    STATE.with_borrow_mut(|s| {
        if let Some(s) = s {
            with_active(|r| {
                if s.marks == 0 {
                    s.mark_count = r.gc_live_count();
                    s.mark_bytes = r.gc_live_bytes();
                }
                s.marks += 1;
            });
        }
    });
}

unsafe extern "C" fn finalizer(object: Oop) {
    STATE.with_borrow_mut(|s| {
        let s = s
            .as_mut()
            .expect("observing input finalized within its probe");
        assert_eq!(object.addr(), s.input);
        // The object is detached from the heap, but its payload stays valid
        // for this callback. The child's address is checked without dereference.
        assert_eq!(unsafe { (*object.cast::<Oop>()).addr() }, s.child);
        with_active(|r| {
            let heap = unsafe { &(*r.state.get()).heap };
            assert!(
                !heap.has_dead_allocations(),
                "foreign finalizers see a settled journal"
            );
            assert_eq!(heap.physical_allocation_count(), r.gc_live_count());
        });
        s.finalized += 1;
    });
}

pub(super) unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_source_probe_native_roots(|r| {
        assert!(STATE.with_borrow(Option::is_none));
        let (factory, calls, observing) = (a[0], r.integer_value(a[1]), r.integer_value(a[2]) != 0);
        assert!(calls == 3 || calls == 32);
        let capacity = r.integer_value(a[3]) != 0;
        assert!(!capacity || calls == 3);
        let limit = crate::heap::MAX_DEAD_ALLOCATIONS;
        let iterations = if capacity { limit + 1 } else { 1 };
        let selector = r.intern(b"value:").unwrap();
        let nil = r.canonical().nil;
        for _ in 0..128 {
            assert_eq!(r.send_slice(factory, selector, &[nil]), nil);
        }
        r.gc_collect_for_source_probe();
        let input = r.allocate_instance(r.globals().object_vt, core::mem::size_of::<Oop>());
        let _input = ScopedOopRootGuard::new(r, input);
        let child = r.make_integer(999);
        assert!(unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(input, 0, child.addr())
        });
        if observing {
            r.set_finalizer(input, finalizer);
        }
        STATE.replace(Some(State {
            input: input.addr(),
            child: child.addr(),
            observing,
            base_count: r.gc_live_count(),
            base_bytes: r.gc_live_bytes(),
            ..State::default()
        }));
        let before = counters();
        let completed_before = completed_calls();
        let bytes_before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        let mut at_limit_storage = 0;
        for iteration in 0..iterations {
            assert_eq!(r.send_slice(factory, selector, &[input]), input);
            if capacity && iteration + 1 == limit {
                at_limit_storage = unsafe { (*r.state.get()).heap.dead_allocation_storage_bytes() };
            }
        }
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
        let journal_storage = heap.dead_allocation_storage_bytes();
        if capacity {
            assert_eq!(
                journal_storage, at_limit_storage,
                "refused allocation cannot grow the journal"
            );
        }
        let after = counters();
        println!(
            "closure-journal-run calls={calls} iterations={iterations} limit={limit} journal_storage={journal_storage} observing={} bytes={} physical_saved={} attempts={} hits={} materialized={} completed_calls={}",
            usize::from(observing),
            heap.bytes_since_collection() - bytes_before,
            heap.allocation_count() - heap.physical_allocation_count(),
            after.0 - before.0,
            after.1 - before.1,
            after.2 - before.2,
            completed_calls() - completed_before
        );
        nil
    })
}

pub(super) unsafe extern "C" fn inspect(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    with_source_probe_native_roots(|r| {
        let native = r.integer_value(unsafe { *args });
        assert!((0..=2).contains(&native));
        assert!(STATE.with_borrow(Option::is_some));
        r.gc_register_root_provider(mark_observer);
        let first_reclaimed = match native {
            0 => r.gc_collect_for_source_probe(),
            1 => r.gc_collect(),
            2 => collect_with_live_native_root(r),
            _ => unreachable!(),
        };
        let (first_input_alive, first_child_alive, first_finalized) = STATE.with_borrow(|s| {
            let s = s.as_ref().unwrap();
            let heap = unsafe { &(*r.state.get()).heap };
            let input_alive = heap.contains(core::ptr::with_exposed_provenance_mut(s.input));
            let child_alive = heap.contains(core::ptr::with_exposed_provenance_mut(s.child));
            assert!(!input_alive || child_alive);
            assert_eq!(s.finalized, usize::from(s.observing && !input_alive));
            (input_alive, child_alive, s.finalized)
        });
        // Native scanning includes incidental stack words, whose retention is
        // not reproducible even between baseline processes. Retain that first
        // sweep's observations, then check exact reclamation once only the Id
        // and explicit runtime roots apply. The deliberately live native root
        // in mode 2 has returned and no longer protects the private input.
        let reclaimed = first_reclaimed
            + if native == 0 {
                0
            } else {
                r.gc_collect_for_source_probe()
            };
        let s = STATE.take().unwrap();
        let heap = unsafe { &(*r.state.get()).heap };
        assert_eq!(s.marks, if native == 0 { 1 } else { 2 });
        assert_eq!(heap.bytes_since_collection(), 0);
        assert!(!heap.has_dead_allocations());
        assert_eq!(heap.allocation_count(), heap.physical_allocation_count());
        let input_alive = heap.contains(core::ptr::with_exposed_provenance_mut(s.input));
        let child_alive = heap.contains(core::ptr::with_exposed_provenance_mut(s.child));
        assert!(!input_alive && !child_alive);
        assert_eq!(s.finalized, usize::from(s.observing));
        println!(
            "closure-journal-inspect native={native} first_reclaimed={first_reclaimed} first_input_alive={} first_child_alive={} first_finalized={first_finalized} input_alive={} child_alive={} finalized={} reclaimed={reclaimed} mark_count_delta={} mark_bytes_delta={} after_count_delta={} after_bytes_delta={} journal_storage={}",
            usize::from(first_input_alive),
            usize::from(first_child_alive),
            usize::from(input_alive),
            usize::from(child_alive),
            s.finalized,
            s.mark_count as isize - s.base_count as isize,
            s.mark_bytes as isize - s.base_bytes as isize,
            r.gc_live_count() as isize - s.base_count as isize,
            r.gc_live_bytes() as isize - s.base_bytes as isize,
            heap.dead_allocation_storage_bytes()
        );
        r.canonical().nil
    })
}

#[inline(never)]
fn collect_with_live_native_root(r: &Runtime) -> usize {
    let input = STATE.with_borrow(|s| s.as_ref().unwrap().input);
    // This array deliberately tests native-root discovery; it is not optimizer
    // scratch. Exposing its address and using it after GC keeps actual stack
    // storage live across the normal native scan and register spill.
    let roots = [core::ptr::with_exposed_provenance_mut::<u8>(input)];
    std::hint::black_box(&roots);
    let reclaimed = r.gc_collect();
    assert!(unsafe {
        (*r.state.get())
            .heap
            .contains(std::hint::black_box(roots[0]).cast())
    });
    assert_eq!(STATE.with_borrow(|s| s.as_ref().unwrap().finalized), 0);
    reclaimed
}

pub(super) unsafe extern "C" fn restored(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    with_source_probe_native_roots(|r| {
        let factory = unsafe { *args };
        let calls = r.integer_value(unsafe { *args.add(1) });
        assert!(calls == 3 || calls == 32);
        let selector = r.intern(b"value:").unwrap();
        let before = counters();
        let completed_before = completed_calls();
        let nil = r.canonical().nil;
        assert_eq!(r.send_slice(factory, selector, &[nil]), nil);
        let after = counters();
        println!(
            "closure-journal-restored hits={} materialized={} completed_calls={}",
            after.1 - before.1,
            after.2 - before.2,
            completed_calls() - completed_before
        );
        nil
    })
}
