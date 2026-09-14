//! Test-floor bridge for actual installed numeric source and recovery counters.
//! No probe or additional ABI entry is present in production builds.
use super::native_tier::unboxed_numeric::{COMPLETED, MATERIALIZED};
use super::*;

mod control;
mod loop_measure;
mod measure;
pub(super) use control::{active as control_active, after_action};

impl Runtime {
    pub(crate) fn install_unboxed_numeric_probe(&self) {
        let vt = self.create_vtable(self.globals().object_vt);
        self.add_method(
            vt,
            self.intern(b"checkNativeRoots").unwrap(),
            check_native_roots,
        );
        self.add_method(vt, self.intern(b"run:input:other:expected:").unwrap(), run);
        self.add_method(
            vt,
            self.intern(b"control:input:other:expected:at:capture:")
                .unwrap(),
            control::run,
        );
        self.add_method(
            vt,
            self.intern(b"prepare:captures:").unwrap(),
            measure::prepare,
        );
        self.add_method(
            vt,
            self.intern(b"measure:input:other:operations:shape:batches:")
                .unwrap(),
            measure::run,
        );
        self.add_method(vt, self.intern(b"loopReused").unwrap(), loop_reused);
        self.add_method(
            vt,
            self.intern(b"measureLoop:numeric:operations:shape:batches:")
                .unwrap(),
            loop_measure::run,
        );
        let object = self.allocate_instance(vt, 0);
        self.export(b"UnboxedNumericProbe", object).unwrap();
    }
}

unsafe extern "C" fn check_native_roots(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_source_probe_native_roots(|r| {
        let inner = r.make_integer(1234567);
        r.gc_collect_for_source_probe();
        assert!(
            !unsafe { (*r.state.get()).heap.contains(inner) },
            "probe scratch must not become native roots"
        );
        println!("source-probe-roots inner_dead=1");
        r.canonical().nil
    })
}

unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_source_probe_native_roots(|r| {
        let (block, input, other) = (a[0], a[1], a[2]);
        let expected = r.integer_value(a[3]);
        let _block = ScopedOopRootGuard::new(r, block);
        let _input = ScopedOopRootGuard::new(r, input);
        let _other = ScopedOopRootGuard::new(r, other);
        let selector = r.intern(b"value:value:").unwrap();
        let invoke = || {
            let answer = r.send_slice(block, selector, &[input, other]);
            assert_eq!(r.integer_value(answer), expected);
            assert_ne!(answer, input);
            assert_ne!(answer, other);
            answer
        };
        // Keep warmup below the journal limit even for 32-operation regions.
        for _ in 0..16 {
            for _ in 0..8 {
                invoke();
            }
            r.gc_collect_for_source_probe();
        }
        let completed_before = COMPLETED.get();
        let materialized_before = MATERIALIZED.get();
        let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        let count_before = r.gc_live_count();
        let bytes_before = r.gc_live_bytes();
        let mut answer = r.canonical().nil;
        for _ in 0..8 {
            answer = invoke();
        }
        let root = ScopedOopRootGuard::new(r, answer);
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
        let bytes = heap.bytes_since_collection() - before;
        let physical_saved = heap.allocation_count() - heap.physical_allocation_count();
        let counted_indices = heap.counted_frame_index_count();
        let logical_allocations = heap.allocation_count() - count_before;
        let completed = COMPLETED.get() - completed_before;
        let materialized = MATERIALIZED.get() - materialized_before;
        println!(
            "unboxed-numeric invocations=8 bytes={bytes} physical_saved={physical_saved} counted_indices={counted_indices} logical_allocations={logical_allocations} live_before={count_before} live_bytes_before={bytes_before} completed={completed} materialized={materialized} answer={expected}"
        );
        let reclaimed_objects = r.gc_collect_for_source_probe();
        let heap = unsafe { &(*r.state.get()).heap };
        println!(
            "unboxed-numeric-gc reclaimed_objects={reclaimed_objects} live_objects={} live_bytes={} remaining_bytes={}",
            heap.allocation_count(),
            heap.allocated_bytes(),
            heap.bytes_since_collection()
        );
        assert_eq!(r.integer_value(answer), expected);
        drop(root);
        r.canonical().nil
    })
}

unsafe extern "C" fn loop_reused(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_source_probe_native_roots(|r| {
        r.make_integer(super::numeric::loop_reuse::PREDICTED_OPERATIONS_REUSED.get() as i64)
    })
}
