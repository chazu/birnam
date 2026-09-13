use super::*;
use crate::runtime::native_tier::unboxed_numeric;
use crate::runtime::numeric::tests::Fixture;

pub(super) struct Mode(bool, Option<usize>);
impl Mode {
    pub(super) fn set(enabled: bool, force: Option<usize>) -> Self {
        Self(
            unboxed_numeric::ENABLED.replace(enabled),
            unboxed_numeric::FORCE_AT.replace(force),
        )
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        unboxed_numeric::ENABLED.set(self.0);
        unboxed_numeric::FORCE_AT.set(self.1);
    }
}

fn equal_results() -> Vec<u8> {
    let send = |left, right| Expr::Send {
        receiver: Box::new(Expr::Global(left)),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![Expr::Global(right)],
        source: b"",
    };
    portable::encode_block(
        &Block {
            params: vec![b"a", b"b"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: send(b"a", b"b"),
                },
                Statement::Let {
                    name: b"second",
                    value: send(b"a", b"b"),
                },
                Statement::Expr(send(b"first", b"second")),
            ],
        },
        &[],
    )
    .unwrap()
}

fn continued_chain() -> Vec<u8> {
    let send = |receiver| Expr::Send {
        receiver: Box::new(Expr::Global(receiver)),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![Expr::Global(b"b")],
        source: b"",
    };
    let mut body = vec![Statement::Let {
        name: b"answer",
        value: send(b"a"),
    }];
    for _ in 0..110 {
        body.push(Statement::Let {
            name: b"answer",
            value: Expr::Global(b"answer"),
        });
    }
    body.push(Statement::Expr(send(b"answer")));
    portable::encode_block(
        &Block {
            params: vec![b"a", b"b"],
            body,
        },
        &[],
    )
    .unwrap()
}

fn long_chain(count: usize) -> Vec<u8> {
    let mut body = Vec::new();
    for index in 0..count {
        let send = Expr::Send {
            receiver: Box::new(Expr::Global(if index == 0 { b"a" } else { b"answer" })),
            selector: Cow::Borrowed(b"blend:"),
            args: vec![Expr::Global(b"b")],
            source: b"",
        };
        body.push(if index + 1 == count {
            Statement::Expr(send)
        } else {
            Statement::Let {
                name: b"answer",
                value: send,
            }
        });
    }
    portable::encode_block(
        &Block {
            params: vec![b"a", b"b"],
            body,
        },
        &[],
    )
    .unwrap()
}

#[test]
fn unboxed_numeric_preserves_boundaries_and_late_binding() {
    fused_numeric_tests::numeric_driver_preserves_allocations_boundaries_and_late_binding(true);
}

#[test]
fn unboxed_numeric_driver_saves_results_and_recovers_each_original_boundary() {
    for shape in 0..6 {
        let operations = match shape {
            3 => 2,
            4 => 16,
            5 => 32,
            _ => 3,
        };
        let tail = shape != 0;
        let bytes = match shape {
            2 => equal_results(),
            3 => continued_chain(),
            4 | 5 => long_chain(operations),
            _ => fused_numeric_tests::program(tail),
        };
        let mut baseline = None;
        let cases = core::iter::once((false, None))
            .chain(core::iter::once((true, None)))
            .chain(
                (0..if shape == 3 {
                    3
                } else {
                    operations + usize::from(!tail)
                })
                    .map(|pc| (true, Some(pc))),
            );
        for (enabled, force) in cases {
            let _mode = Mode::set(enabled, None);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            let selector = r.intern(b"blend:").unwrap();
            r.add_method(r.canonical().smallint_vt, selector, native.method(1));
            let block = install(r, &bytes, 741601, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let a = r.make_integer(11);
            let _a = ScopedOopRootGuard::new(r, a);
            let b = r.make_integer(3);
            let _b = ScopedOopRootGuard::new(r, b);
            let expected = match shape {
                2 => 28,
                3 => 17,
                4 | 5 => 11 + 3 * operations as i64,
                _ => 27,
            };
            for _ in 0..128 {
                let answer = r.enter(|| r.send_slice(block.receiver, block.selector, &[a, b]));
                assert_eq!(r.integer_value(answer), expected);
            }
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            unboxed_numeric::FORCE_AT.set(force);
            unboxed_numeric::COMPLETED.set(0);
            unboxed_numeric::MATERIALIZED.set(0);
            let machine = r.create_stackless_machine().unwrap();
            let _machine = ScopedOopRootGuard::new(r, machine);
            let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let answer = r
                .enter(|| {
                    r.drive_stackless(
                        machine,
                        block.descriptor.clone(),
                        &[(0, block.receiver), (1, a), (2, b)],
                    )
                })
                .unwrap();
            assert_eq!(
                r.integer_value(answer),
                expected,
                "shape={shape} force={force:?}"
            );
            assert_ne!(answer, a);
            assert_ne!(answer, b);
            let state = unsafe { &*r.state.get() };
            let charge = state.heap.bytes_since_collection() - before;
            if let Some(baseline) = baseline {
                assert_eq!(charge, baseline);
            } else {
                baseline = Some(charge);
            }
            let completed = unboxed_numeric::COMPLETED.get();
            let materialized = unboxed_numeric::MATERIALIZED.get();
            assert_eq!(
                completed,
                if !enabled {
                    0
                } else if shape == 3 {
                    force.map_or(2, |pc| usize::from(pc > 0))
                } else {
                    force.unwrap_or(operations)
                },
                "shape={shape} force={force:?}"
            );
            let expected_materialized = if !enabled || force == Some(0) || tail && force.is_none() {
                0
            } else if shape == 2 && force == Some(2) {
                2
            } else {
                1
            };
            assert_eq!(
                materialized, expected_materialized,
                "shape={shape} force={force:?}"
            );
            let saved = state.heap.allocation_count() - state.heap.physical_allocation_count();
            assert_eq!(
                saved,
                completed - materialized - usize::from(enabled && tail && force.is_none()),
                "shape={shape} force={force:?}"
            );
            let cap = r.stackless_machine_capability(machine).unwrap();
            assert!(
                state
                    .execution
                    .machine(current_thread_token(), cap)
                    .unwrap()
                    .arena()
                    .current_handle()
                    .is_none()
            );
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        }
    }
}

#[test]
fn unboxed_numeric_budget_preserves_actual_collection_and_input_mutation() {
    fused_numeric_tests::numeric_budget_preserves_actual_collection_and_input_mutation(true);
}

#[test]
fn unboxed_numeric_faults_leave_error_effects_at_their_original_calls() {
    fused_numeric_tests::numeric_faults_leave_error_effects_at_their_original_calls(true);
}
