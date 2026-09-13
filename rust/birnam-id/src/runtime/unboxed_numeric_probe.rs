//! Test-floor bridge for actual installed numeric source and recovery counters.
//! No probe or additional ABI entry is present in production builds.
use super::native_tier::unboxed_numeric::{COMPLETED, MATERIALIZED};
use super::*;

mod control;
mod measure;
pub(super) use control::{active as control_active, after_action};

impl Runtime {
    pub(crate) fn install_unboxed_numeric_probe(&self) {
        let vt = self.create_vtable(self.globals().object_vt);
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
        let object = self.allocate_instance(vt, 0);
        self.export(b"UnboxedNumericProbe", object).unwrap();
    }
}

unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
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
            r.gc_collect_from_native_range(0, 0);
        }
        let completed_before = COMPLETED.get();
        let materialized_before = MATERIALIZED.get();
        let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        let mut answer = r.canonical().nil;
        for _ in 0..8 {
            answer = invoke();
        }
        let root = ScopedOopRootGuard::new(r, answer);
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD);
        let bytes = heap.bytes_since_collection() - before;
        let physical_saved = heap.allocation_count() - heap.physical_allocation_count();
        let completed = COMPLETED.get() - completed_before;
        let materialized = MATERIALIZED.get() - materialized_before;
        println!(
            "unboxed-numeric invocations=8 bytes={bytes} physical_saved={physical_saved} completed={completed} materialized={materialized} answer={expected}"
        );
        r.gc_collect_from_native_range(0, 0);
        assert_eq!(r.integer_value(answer), expected);
        drop(root);
        r.canonical().nil
    })
}
