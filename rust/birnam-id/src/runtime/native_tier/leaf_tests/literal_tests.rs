//! Canonical compiled execution independently checks literal allocation order.
use super::*;

struct LiteralMode;
impl Drop for LiteralMode {
    fn drop(&mut self) {
        LITERAL_CALLEES.set(false);
    }
}

#[test]
fn literal_callee_preserves_fresh_identity_allocation_trace_and_recovery() {
    let _mode = LiteralMode;
    for literal in [i64::MIN, -1, 0, 42, i64::MAX] {
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![
                    Statement::Expr(Expr::SmallInt(123)), // Unused allocation stays observable to GC.
                    Statement::Let {
                        name: b"local",
                        value: Expr::SmallInt(literal),
                    },
                    Statement::Let {
                        name: b"alias",
                        value: Expr::Global(b"local"),
                    },
                    Statement::Expr(Expr::Global(b"alias")),
                ],
            },
            &[],
        )
        .unwrap();
        let mut baseline = None;
        for optimized in [false, true] {
            LITERAL_CALLEES.set(optimized);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let input = r.make_integer(literal);
            let _input = ScopedOopRootGuard::new(r, input);
            let leaf = install(r, &bytes, 740100, None);
            let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
            assert_eq!(
                r.native_tier
                    .borrow()
                    .records
                    .get(&(Arc::as_ptr(&leaf.descriptor) as usize))
                    .is_some_and(|record| record.leaf.is_some()),
                optimized
            );
            let caller = install(r, &caller(), 740200, None);
            let _caller = ScopedOopRootGuard::new(r, caller.receiver);
            r.native_tier
                .borrow_mut()
                .records
                .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
            let drive = || {
                r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]))
            };
            for _ in 0..80 {
                assert_eq!(r.integer_value(drive()), literal);
            }
            INLINE_HITS.set(0);
            INLINE_RECOVERIES.set(0);
            alias_tests::BOXES.set(Some(Vec::new()));
            let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let first = drive();
            let _first = ScopedOopRootGuard::new(r, first);
            let second = drive();
            let _second = ScopedOopRootGuard::new(r, second);
            assert_ne!(
                first, second,
                "equal literals from different invocations are fresh"
            );
            assert_ne!(first, input);
            assert_eq!(r.integer_value(first), literal);
            assert_eq!(r.integer_value(second), literal);
            let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
            let trace = alias_tests::BOXES.take().unwrap();
            assert_eq!(INLINE_HITS.get(), if optimized { 2 } else { 0 });
            if optimized {
                assert_eq!(baseline.as_ref(), Some(&(bytes, trace)));
            } else {
                baseline = Some((bytes, trace));
            }

            // A changed Return meaning observes a physical callee after recovery.
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.return_at, return_override);
            RETURNS.set(0);
            assert_eq!(r.integer_value(drive()), literal);
            assert_eq!(RETURNS.get(), 2);
            assert_eq!(INLINE_RECOVERIES.get(), usize::from(optimized));
            r.add_method(
                objects.machine_vtable,
                objects.return_at,
                crate::execution::machine_return_at,
            );

            // The literal stream is longer than the old entry-only budget.
            r.add_method(
                objects.machine_vtable,
                objects.call_site,
                alias_tests::alias_boundary_call,
            );
            let anchor = 0usize;
            r.gc_set_stack_bottom(core::ptr::from_ref(&anchor).addr());
            r.gc_register_root_provider(gc_observe);
            GC_DEPTH.set(0);
            BOUNDARY.set(1);
            assert_eq!(r.integer_value(drive()), literal);
            r.gc_set_auto_requested(false);
            assert_eq!(GC_DEPTH.get(), 2);
            assert_eq!(INLINE_RECOVERIES.get(), 2 * usize::from(optimized));
            assert_eq!(
                r.integer_value(first),
                literal,
                "earlier rooted result survives collection"
            );
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
        }
    }
}
