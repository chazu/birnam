use super::*;
use crate::runtime::numeric::{self, tests::Fixture};

thread_local! { static ENTRY_LOOKUPS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn observed_lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    ENTRY_LOOKUPS.set(ENTRY_LOOKUPS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

#[test]
fn numeric_entry_preflight_is_pure_current_and_requires_a_warm_callee() {
    for precise in [false, true] {
        for tail in [false, true] {
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            r.gc_set_precise(precise);
            let native = Fixture::load(r);
            let blend = r.intern(b"blend:").unwrap();
            r.add_method(r.canonical().smallint_vt, blend, native.method(1));
            let bytes = super::fused_numeric_tests::program(tail);
            let block = install(r, &bytes, 741601, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let a = r.make_integer(11);
            let _a = ScopedOopRootGuard::new(r, a);
            let b = r.make_integer(3);
            let _b = ScopedOopRootGuard::new(r, b);
            let machine = r.create_stackless_machine().unwrap();
            let _machine = ScopedOopRootGuard::new(r, machine);
            let initial = [(0, r.canonical().nil), (1, a), (2, b)];
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none()
            );
            for _ in 0..128 {
                let drive = r.create_stackless_machine().unwrap();
                let _drive = ScopedOopRootGuard::new(r, drive);
                let answer = r
                    .enter(|| {
                        r.drive_stackless(
                            drive,
                            block.descriptor.clone(),
                            &[(0, block.receiver), (1, a), (2, b)],
                        )
                    })
                    .unwrap();
                assert_eq!(r.integer_value(answer), 27);
            }
            let before = (
                r.gc_live_bytes(),
                numeric::HITS.get(),
                super::super::fused_numeric::HITS.get(),
            );
            let ticket = r
                .prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                .expect("prepared numeric entry must predict without a callee Frame");
            assert_eq!(ticket.fusion_result(0, 0), Some(14));
            assert_eq!(ticket.fusion_result(1, 1), Some(16));
            assert_eq!(ticket.fusion_result(2, 2), Some(27));
            assert_eq!(
                before,
                (
                    r.gc_live_bytes(),
                    numeric::HITS.get(),
                    super::super::fused_numeric::HITS.get()
                ),
                "preflight neither allocates Birnam objects nor executes source methods"
            );
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial[..2])
                    .is_none()
            );
            let duplicate = [(1, a), (1, b), (2, b)];
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &duplicate)
                    .is_none()
            );
            let invalid = [(block.descriptor.slot_count(), a), (1, a), (2, b)];
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &invalid)
                    .is_none()
            );
            let floating = r.make_float(11.0);
            let _floating = ScopedOopRootGuard::new(r, floating);
            assert!(
                r.prepare_numeric_entry_prediction(
                    machine,
                    &block.descriptor,
                    &[(1, floating), (2, b)]
                )
                .is_none()
            );
            let maximum = r.make_integer(i64::MAX);
            let _maximum = ScopedOopRootGuard::new(r, maximum);
            assert!(
                r.prepare_numeric_entry_prediction(
                    machine,
                    &block.descriptor,
                    &[(1, maximum), (2, b)]
                )
                .is_none()
            );
            r.add_method(r.canonical().smallint_vt, blend, native.method(2));
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none()
            );
            r.add_method(r.canonical().smallint_vt, blend, native.method(1));
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_some(),
                "restoration is immediate, with no negative cache"
            );
            unsafe { native.declaration_toggle()(0) };
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none()
            );
            unsafe { native.declaration_toggle()(1) };
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_some()
            );
            let changed = r.make_integer(13);
            let _changed = ScopedOopRootGuard::new(r, changed);
            let other = r
                .prepare_numeric_entry_prediction(
                    machine,
                    &block.descriptor,
                    &[(1, changed), (2, b)],
                )
                .unwrap();
            assert_eq!(other.fusion_result(2, 2), Some(31));
            assert_eq!(
                ticket.fusion_result(2, 2),
                Some(27),
                "simultaneous predictions own independent buffers"
            );
            let error_symbol = r.intern(b"ErrorVt").unwrap();
            let original_error = unsafe { (*r.state.get()).environment.import(error_symbol) };
            r.export(b"ErrorVt", r.canonical().smallint_vt).unwrap();
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none()
            );
            r.export(b"ErrorVt", original_error).unwrap();
            ENTRY_LOOKUPS.set(0);
            r.add_method(r.globals().vtable_vt, r.globals().lookup, observed_lookup);
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none()
            );
            assert_eq!(
                ENTRY_LOOKUPS.get(),
                0,
                "preflight refuses an observing lookup without executing it"
            );
            r.add_method(
                r.globals().vtable_vt,
                r.globals().lookup,
                crate::dispatch::vtable_lookup_method,
            );
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_some()
            );
            // Withdraw the live Method before requesting ordinary retirement.
            // A rooted receiver/vtable must not force retirement of its method.
            assert!(
                r.remove_method(r.checked_vtable_of(block.receiver), block.selector)
                    .is_some()
            );
            assert!(r.enter(|| r.retire_stackless_code_generation(block.generation)));
            assert!(
                r.prepare_numeric_entry_prediction(machine, &block.descriptor, &initial)
                    .is_none(),
                "a retained descriptor cannot authorize a retired callee"
            );
            assert_eq!(
                ticket.fusion_result(2, 2),
                Some(27),
                "an old prediction owns only its pure numeric storage, not callee authority"
            );
        }
    }
}
