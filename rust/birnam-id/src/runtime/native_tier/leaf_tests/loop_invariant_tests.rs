use super::super::{fused_numeric, unboxed_numeric};
use super::*;
use crate::execution::tail_entry;
use crate::runtime::numeric::{self, loop_reuse, tests::Fixture};

#[derive(Clone, Copy, PartialEq)]
enum Change {
    None,
    Input,
    Operator,
    Lookup,
    Collect,
    Declaration,
    OperatorRestore,
    Capture,
    Suspend,
}
thread_local! {
    static SUPPLIED: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static ITERATION: Cell<usize> = const { Cell::new(0) };
    static LIMIT: Cell<usize> = const { Cell::new(0) };
    static CHANGE: Cell<Change> = const { Cell::new(Change::None) };
    static STOP: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static ANSWER: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static LOOKUPS: Cell<usize> = const { Cell::new(0) };
    static DECLARATION: Cell<Option<unsafe extern "C" fn(u32)>> = const { Cell::new(None) };
    static ADD: Cell<Option<Method>> = const { Cell::new(None) };
    static SUBTRACT: Cell<Option<Method>> = const { Cell::new(None) };
    static TRACE: std::cell::RefCell<Vec<i64>> = const { std::cell::RefCell::new(Vec::new()) };
}
unsafe extern "C" fn observed_lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    LOOKUPS.set(LOOKUPS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn next(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let n = ITERATION.get();
        let expected = match CHANGE.get() {
            Change::Input if n > 2 => 44,
            Change::Operator if n > 2 => -14,
            Change::OperatorRestore if (3..=4).contains(&n) => -14,
            _ => 36,
        };
        assert_eq!(r.integer_value(args[1]), expected);
        if n == 3 && matches!(CHANGE.get(), Change::Capture | Change::Suspend) {
            assert_eq!(
                args[1],
                SUPPLIED.get(),
                "resume must preserve supplied result identity"
            );
        }
        assert_ne!(
            args[1],
            ANSWER.get(),
            "previous result remains rooted and distinct"
        );
        ANSWER.set(args[1]);
        r.export(b"LoopPreviousResult", args[1]).unwrap();
        TRACE.with_borrow_mut(|trace| trace.push(expected));
        if n == 2 {
            match CHANGE.get() {
                Change::Input => {
                    assert!(unsafe { (*r.state.get()).heap.write_payload_word(args[3], 0, 11) })
                }
                Change::Operator | Change::OperatorRestore => {
                    r.add_method(
                        r.canonical().smallint_vt,
                        r.intern(b"blend:").unwrap(),
                        SUBTRACT.get().unwrap(),
                    );
                }
                Change::Lookup => {
                    r.add_method(r.globals().vtable_vt, r.globals().lookup, observed_lookup);
                }
                Change::Collect => {
                    r.gc_collect_from_native_range(0, 0);
                }
                Change::Declaration => unsafe { DECLARATION.get().unwrap()(0) },
                Change::Capture | Change::Suspend => {
                    let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                    r.add_method(
                        objects.machine_vtable,
                        objects.call_site,
                        if CHANGE.get() == Change::Capture {
                            capture_call
                        } else {
                            control::suspend_call
                        },
                    );
                }
                Change::None => {}
            }
        }
        if n == 4 {
            match CHANGE.get() {
                Change::Declaration => unsafe { DECLARATION.get().unwrap()(1) },
                Change::OperatorRestore => {
                    r.add_method(
                        r.canonical().smallint_vt,
                        r.intern(b"blend:").unwrap(),
                        ADD.get().unwrap(),
                    );
                }
                _ => {}
            }
        }
        ITERATION.set(n + 1);
        if n + 1 == LIMIT.get() {
            STOP.get()
        } else {
            args[0]
        }
    })
}
unsafe extern "C" fn finish(_: Oop, _: *const Oop, _: usize) -> Oop {
    ANSWER.get()
}

fn programs() -> (Vec<u8>, Vec<u8>) {
    let send = |receiver, selector, args| Expr::Send {
        receiver: Box::new(Expr::Global(receiver)),
        selector: Cow::Borrowed(selector),
        args,
        source: b"",
    };
    let numeric = portable::encode_block(
        &Block {
            params: vec![b"a", b"b"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: send(b"a", b"blend:", vec![Expr::Global(b"a")]),
                },
                Statement::Let {
                    name: b"second",
                    value: send(b"first", b"blend:", vec![Expr::Global(b"b")]),
                },
                Statement::Expr(send(b"second", b"blend:", vec![Expr::Global(b"a")])),
            ],
        },
        &[],
    )
    .unwrap();
    let parent = portable::encode_block(
        &Block {
            params: vec![b"again", b"state", b"a", b"b"],
            body: vec![
                Statement::Let {
                    name: b"result",
                    value: send(
                        b"calc",
                        b"value:value:",
                        vec![Expr::Global(b"a"), Expr::Global(b"b")],
                    ),
                },
                Statement::Let {
                    name: b"target",
                    value: send(
                        b"state",
                        b"next:result:a:b:",
                        vec![
                            Expr::Global(b"again"),
                            Expr::Global(b"result"),
                            Expr::Global(b"a"),
                            Expr::Global(b"b"),
                        ],
                    ),
                },
                Statement::Expr(send(
                    b"target",
                    b"value:value:value:value:",
                    vec![
                        Expr::Global(b"again"),
                        Expr::Global(b"state"),
                        Expr::Global(b"a"),
                        Expr::Global(b"b"),
                    ],
                )),
            ],
        },
        &[b"calc".to_vec()],
    )
    .unwrap();
    (numeric, parent)
}

struct Mode(bool, bool, bool, bool);
impl Drop for Mode {
    fn drop(&mut self) {
        loop_reuse::ENABLED.set(self.0);
        tail_entry::ENABLED.set(self.1);
        unboxed_numeric::ENABLED.set(self.2);
        numeric::CHAIN_ENABLED.set(self.3);
    }
}

#[test]
fn loop_invariant_numeric_work_preserves_inputs_methods_lookup_gc_and_fresh_results() {
    let (numeric_program, parent_program) = programs();
    for tails in [false, true] {
        for unboxed in [false, true] {
            for change in [
                Change::None,
                Change::Input,
                Change::Operator,
                Change::Lookup,
                Change::Collect,
                Change::Declaration,
                Change::OperatorRestore,
            ] {
                let mut traces = Vec::new();
                for enabled in [false, true] {
                    let _mode = Mode(
                        loop_reuse::ENABLED.replace(enabled),
                        tail_entry::ENABLED.replace(tails),
                        unboxed_numeric::ENABLED.replace(unboxed),
                        numeric::CHAIN_ENABLED.replace(false),
                    );
                    let bootstrap = crate::dispatch::build_bootstrap();
                    let r = &bootstrap.runtime;
                    r.publish_canonical_exports();
                    r.install_execution_objects();
                    let fixture = Fixture::load(r);
                    r.add_method(
                        r.canonical().smallint_vt,
                        r.intern(b"blend:").unwrap(),
                        fixture.method(1),
                    );
                    SUBTRACT.set(Some(fixture.method(2)));
                    ADD.set(Some(fixture.method(1)));
                    DECLARATION.set(Some(fixture.declaration_toggle()));
                    let calc = install(r, &numeric_program, 741920, None);
                    let _calc = ScopedOopRootGuard::new(r, calc.receiver);
                    let parent = install(r, &parent_program, 741922, Some(calc.receiver));
                    let _parent = ScopedOopRootGuard::new(r, parent.receiver);
                    let vt = r.create_vtable(r.globals().object_vt);
                    r.add_method(vt, r.intern(b"next:result:a:b:").unwrap(), next);
                    let state = r.allocate_instance(vt, 0);
                    let _state = ScopedOopRootGuard::new(r, state);
                    let stop_vt = r.create_vtable(r.globals().object_vt);
                    r.add_method(stop_vt, parent.selector, finish);
                    let stop = r.allocate_instance(stop_vt, 0);
                    let _stop = ScopedOopRootGuard::new(r, stop);
                    STOP.set(stop);
                    let a = r.make_integer(11);
                    let b = r.make_integer(3);
                    let _a = ScopedOopRootGuard::new(r, a);
                    let _b = ScopedOopRootGuard::new(r, b);
                    let invoke = || {
                        r.enter(|| {
                            r.send_slice(
                                parent.receiver,
                                parent.selector,
                                &[parent.receiver, state, a, b],
                            )
                        })
                    };
                    CHANGE.set(Change::None);
                    LIMIT.set(256);
                    ITERATION.set(0);
                    ANSWER.set(core::ptr::null_mut());
                    assert_eq!(r.integer_value(invoke()), 36);
                    CHANGE.set(change);
                    LIMIT.set(8);
                    ITERATION.set(0);
                    TRACE.with_borrow_mut(Vec::clear);
                    LOOKUPS.set(0);
                    loop_reuse::PREDICTED_OPERATIONS_REUSED.set(0);
                    fused_numeric::HITS.set(0);
                    unboxed_numeric::COMPLETED.set(0);
                    let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
                    invoke();
                    let charged = unsafe { (*r.state.get()).heap.bytes_since_collection() };
                    assert_eq!(ITERATION.get(), 8);
                    if change == Change::None || change == Change::Input {
                        assert_eq!(
                            loop_reuse::PREDICTED_OPERATIONS_REUSED.get(),
                            if enabled {
                                if change == Change::None { 21 } else { 19 }
                            } else {
                                0
                            }
                        );
                        assert!(
                            fused_numeric::HITS.get() > 0 || unboxed_numeric::COMPLETED.get() > 0
                        );
                    }
                    traces.push((
                        TRACE.with_borrow(Clone::clone),
                        LOOKUPS.get(),
                        if change == Change::Collect {
                            0
                        } else {
                            charged - before
                        },
                    ));
                    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                }
                assert_eq!(
                    traces[0], traces[1],
                    "values, effects and allocation charges"
                );
            }
        }
    }
}

mod control;
