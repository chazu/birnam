use super::*;
use crate::runtime::native_tier::fused_numeric;
use crate::runtime::numeric;
use numeric::tests::Fixture;

struct Mode(bool, bool);
impl Mode {
    fn set(enabled: bool) -> Self {
        Self(
            fused_numeric::ENABLED.replace(enabled),
            numeric::CHAIN_ENABLED.replace(false),
        )
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        fused_numeric::ENABLED.set(self.0);
        numeric::CHAIN_ENABLED.set(self.1);
    }
}

pub(super) fn program(tail: bool) -> Vec<u8> {
    let send = |left, right| Expr::Send {
        receiver: Box::new(Expr::Global(left)),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![right],
        source: b"",
    };
    let mut body = vec![
        Statement::Let {
            name: b"first",
            value: send(b"a", Expr::Global(b"b")),
        },
        Statement::Let {
            name: b"alias",
            value: Expr::Global(b"first"),
        },
        Statement::Let {
            name: b"second",
            value: send(b"alias", Expr::SmallInt(2)),
        },
    ];
    let last = send(b"second", Expr::Global(b"a"));
    if tail {
        body.push(Statement::Expr(last));
    } else {
        body.push(Statement::Let {
            name: b"last",
            value: last,
        });
        body.push(Statement::Expr(Expr::Global(b"last")));
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

#[derive(Clone, Copy, Debug)]
enum Case {
    Canonical,
    Control,
    TailControl,
    ReturnControl,
    ContinueControl,
    Epoch,
    Lookup,
    Operator,
    Error,
    Declaration,
    OopLayout,
    Trace,
    JournalFull,
}
unsafe extern "C" fn inert_trace(_: Oop, _: crate::abi::MarkFn) {}

thread_local! { static OBSERVATIONS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn call_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::execution::machine_call_site(machine, args, count) }
}
unsafe extern "C" fn tail_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::execution::machine_tail_call_site(machine, args, count) }
}
unsafe extern "C" fn return_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::execution::machine_return_at(machine, args, count) }
}
unsafe extern "C" fn continue_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::execution::machine_continue_at(machine, args, count) }
}
unsafe extern "C" fn lookup_observer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

#[test]
fn fused_numeric_driver_preserves_allocations_boundaries_and_late_binding() {
    numeric_driver_preserves_allocations_boundaries_and_late_binding(false);
}

unsafe extern "C" fn counting_switch_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    if OBSERVATIONS.get() == 2 {
        // PC 0 contains entry/global setup and is not a native copy region.
        // Change after PC 1 so admitted regions execute on both sides.
        regions::COUNTED_INDICES.set(true);
    }
    unsafe { crate::execution::machine_call_site(machine, args, count) }
}

#[test]
fn fused_numeric_counting_control_is_scoped_to_nonobserving_execution() {
    struct CountingMode(bool);
    impl Drop for CountingMode {
        fn drop(&mut self) {
            regions::COUNTED_INDICES.set(self.0);
        }
    }
    let _counting = CountingMode(regions::COUNTED_INDICES.replace(false));
    for precise in [false, true] {
        for tail in [false, true] {
            let _mode = Mode::set(true);
            let _raw = super::unboxed_numeric_tests::Mode::set(false, None);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            r.gc_set_precise(precise);
            let native = Fixture::load(r);
            r.add_method(
                r.canonical().smallint_vt,
                r.intern(b"blend:").unwrap(),
                native.method(1),
            );
            let mut body = Vec::new();
            for operation in 0..16 {
                let value = Expr::Send {
                    receiver: Box::new(Expr::Global(if operation == 0 { b"a" } else { b"v" })),
                    selector: Cow::Borrowed(b"blend:"),
                    args: vec![Expr::Global(b"b")],
                    source: b"",
                };
                body.push(if tail && operation == 15 {
                    Statement::Expr(value)
                } else {
                    Statement::Let { name: b"v", value }
                });
            }
            if !tail {
                body.push(Statement::Expr(Expr::Global(b"v")));
            }
            let bytes = portable::encode_block(
                &Block {
                    params: vec![b"a", b"b"],
                    body,
                },
                &[],
            )
            .unwrap();
            let block = install(r, &bytes, 741507, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let a = r.make_integer(11);
            let _a = ScopedOopRootGuard::new(r, a);
            let b = r.make_integer(3);
            let _b = ScopedOopRootGuard::new(r, b);
            let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[a, b]));
            for _ in 0..128 {
                assert_eq!(r.integer_value(invoke()), 59);
            }
            let snapshot = || {
                let heap = unsafe { &(*r.state.get()).heap };
                (
                    heap.allocation_count(),
                    heap.physical_allocation_count(),
                    r.gc_live_bytes(),
                )
            };
            let reset = || {
                regions::COUNTED_CONTROL_READS.set(0);
                regions::COUNTED_BOXES.set(0);
                regions::HITS.set(0);
                fused_numeric::HITS.set(0);
            };
            let mut canonical_charge = None;
            let mut all_counted = 0;
            // One already-hot Runtime observes each change on the next call.
            // The unfused path retains a fresh read at every admitted PC.
            for (fused, counted) in [(false, false), (true, false), (true, true), (true, false)] {
                let _mode = Mode::set(fused);
                regions::COUNTED_INDICES.set(counted);
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                reset();
                let before = snapshot();
                let answer = invoke();
                let after = snapshot();
                assert_eq!(r.integer_value(answer), 59);
                assert_ne!(answer, a);
                assert_ne!(answer, b);
                let charge = (after.0 - before.0, after.2 - before.2);
                assert_eq!(*canonical_charge.get_or_insert(charge), charge);
                assert!(
                    regions::HITS.get() > 1,
                    "precise={precise} tail={tail} fused={fused} counted={counted} region_hits={} numeric_hits={} reads={}",
                    regions::HITS.get(),
                    fused_numeric::HITS.get(),
                    regions::COUNTED_CONTROL_READS.get()
                );
                assert_eq!(fused_numeric::HITS.get(), if fused { 16 } else { 0 });
                assert_eq!(
                    regions::COUNTED_CONTROL_READS.get(),
                    if fused { 1 } else { regions::HITS.get() }
                );
                let saved = regions::COUNTED_BOXES.get();
                assert_eq!(saved > 0, counted);
                assert_eq!(charge.0 - (after.1 - before.1), saved);
                if counted {
                    all_counted = saved;
                }
                let _answer = ScopedOopRootGuard::new(r, answer);
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                assert_eq!(r.integer_value(answer), 59);
            }

            // A user-defined publication method creates an observation boundary.
            // Its switch mutation must affect the subsequent PC in this call.
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(
                objects.machine_vtable,
                objects.call_site,
                counting_switch_observer,
            );
            regions::COUNTED_INDICES.set(false);
            OBSERVATIONS.set(0);
            reset();
            assert_eq!(r.integer_value(invoke()), 59);
            assert_eq!(fused_numeric::HITS.get(), 0);
            assert!(OBSERVATIONS.get() >= 2);
            assert!(regions::COUNTED_CONTROL_READS.get() > 1);
            assert_eq!(regions::COUNTED_CONTROL_READS.get(), regions::HITS.get());
            assert!(regions::COUNTED_BOXES.get() > 0);
            assert!(regions::COUNTED_BOXES.get() < all_counted);
        }
    }
}

pub(super) fn numeric_driver_preserves_allocations_boundaries_and_late_binding(unboxed: bool) {
    for tail in [false, true] {
        let bytes = program(tail);
        for case in [
            Case::Canonical,
            Case::Control,
            Case::TailControl,
            Case::ReturnControl,
            Case::ContinueControl,
            Case::Epoch,
            Case::Lookup,
            Case::Operator,
            Case::Error,
            Case::Declaration,
        ]
        .into_iter()
        .chain(
            unboxed
                .then_some([Case::OopLayout, Case::Trace, Case::JournalFull])
                .into_iter()
                .flatten(),
        ) {
            let mut evidence = Vec::new();
            for enabled in [false, true] {
                let _mode = Mode::set(enabled && !unboxed);
                let _raw_mode = super::unboxed_numeric_tests::Mode::set(enabled && unboxed, None);
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                let native = Fixture::load(r);
                let blend = r.intern(b"blend:").unwrap();
                r.add_method(r.canonical().smallint_vt, blend, native.method(1));
                let block = install(r, &bytes, 741503, None);
                let _block = ScopedOopRootGuard::new(r, block.receiver);
                let a = r.make_integer(11);
                let _a = ScopedOopRootGuard::new(r, a);
                let b = r.make_integer(3);
                let _b = ScopedOopRootGuard::new(r, b);
                let logical = resumable::lower_portable_block(&bytes).unwrap();
                let layout = resumable::color_slots(&logical).unwrap();
                let boxes = birnam_codegen::numeric_chain::Template::derive(&logical, &layout)
                    .unwrap()
                    .body_boxes()
                    + 1;
                let invoke = || {
                    let machine = r.create_stackless_machine().unwrap();
                    let _machine = ScopedOopRootGuard::new(r, machine);
                    let error_symbol_exists =
                        unsafe { (*r.state.get()).symbols.existing(b"ErrorVt").is_some() };
                    let before = r.gc_live_bytes();
                    let answer = r
                        .enter(|| {
                            r.drive_stackless(
                                machine,
                                block.descriptor.clone(),
                                &[(0, block.receiver), (1, a), (2, b)],
                            )
                        })
                        .unwrap();
                    if matches!(case, Case::Canonical) && error_symbol_exists {
                        assert_eq!(
                            r.gc_live_bytes() - before,
                            boxes * crate::heap::Heap::allocation_total_size(8).unwrap(),
                            "the whole-body allocation budget includes every actual box"
                        );
                    }
                    let cap = r.stackless_machine_capability(machine).unwrap();
                    let diagnostics = unsafe {
                        (*r.state.get())
                            .execution
                            .machine_diagnostics(current_thread_token(), cap)
                    }
                    .unwrap();
                    (
                        answer,
                        diagnostics.current_synchronous_boundary_depth,
                        diagnostics.high_water_synchronous_boundary_depth,
                    )
                };
                for _ in 0..128 {
                    assert_eq!(r.integer_value(invoke().0), 27);
                }
                let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                match case {
                    Case::Canonical => {}
                    Case::Control => {
                        r.add_method(objects.machine_vtable, objects.call_site, call_observer);
                    }
                    Case::TailControl => {
                        r.add_method(
                            objects.machine_vtable,
                            objects.tail_call_site,
                            tail_observer,
                        );
                    }
                    Case::ReturnControl => {
                        r.add_method(objects.machine_vtable, objects.return_at, return_observer);
                    }
                    Case::ContinueControl => {
                        r.add_method(
                            objects.machine_vtable,
                            objects.continue_at,
                            continue_observer,
                        );
                    }
                    Case::Epoch => unsafe {
                        (*r.state.get()).vtables.exhaust_mutation_epoch_for_test();
                    },
                    Case::Lookup => {
                        r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_observer);
                    }
                    Case::Operator => {
                        r.add_method(r.canonical().smallint_vt, blend, native.method(2));
                    }
                    Case::Error => {
                        r.export(b"ErrorVt", r.canonical().smallint_vt).unwrap();
                    }
                    Case::Declaration => unsafe { native.declaration_toggle()(0) },
                    Case::OopLayout => r.set_layout(
                        r.canonical().smallint_vt,
                        core::ptr::null(),
                        0,
                        crate::abi::SLOT_OOP,
                    ),
                    Case::Trace => r.set_trace_hook(r.canonical().smallint_vt, inert_trace),
                    Case::JournalFull => {
                        r.enter(|| r.gc_collect_from_native_range(0, 0));
                        let heap = unsafe { &mut (*r.state.get()).heap };
                        let snapshot = heap
                            .prepare_virtual_bytes(r.canonical().smallint_vt, &1i64.to_ne_bytes())
                            .unwrap();
                        for _ in 0..crate::heap::MAX_DEAD_ALLOCATIONS {
                            assert!(heap.record_virtual_clone(snapshot.clone()).is_some());
                        }
                        assert!(!heap.admits_virtual_allocations(1));
                    }
                }
                super::super::unboxed_numeric::COMPLETED.set(0);
                fused_numeric::HITS.set(0);
                OBSERVATIONS.set(0);
                let before = r.gc_live_bytes();
                let mut outcomes = Vec::new();
                for _ in 0..32 {
                    let (answer, depth, high_water) = invoke();
                    let expected = match case {
                        Case::Operator => -5,
                        Case::Error => 11,
                        _ => 27,
                    };
                    assert_eq!(r.integer_value(answer), expected);
                    assert_eq!(answer == a, matches!(case, Case::Error));
                    outcomes.push((r.integer_value(answer), depth, high_water));
                }
                assert_eq!(
                    if unboxed {
                        super::super::unboxed_numeric::COMPLETED.get()
                    } else {
                        fused_numeric::HITS.get()
                    },
                    if enabled && matches!(case, Case::Canonical) {
                        96
                    } else {
                        0
                    },
                    "{case:?} tail={tail}"
                );
                evidence.push((r.gc_live_bytes() - before, OBSERVATIONS.get(), outcomes));
                // The ordinary registered entry uses the same fused path and
                // releases its real generation/frame pin after completion.
                let answer = r.enter(|| r.send_slice(block.receiver, block.selector, &[a, b]));
                assert_eq!(
                    r.integer_value(answer),
                    match case {
                        Case::Operator => -5,
                        Case::Error => 11,
                        _ => 27,
                    }
                );
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            }
            assert_eq!(evidence[0], evidence[1], "{case:?} tail={tail}");
        }
    }
}

thread_local! {
    static GC_INPUT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static COLLECTIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn collecting_mutation(mark: crate::abi::MarkFn) {
    COLLECTIONS.set(COLLECTIONS.get() + 1);
    with_active(|r| {
        assert!(unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(GC_INPUT.get(), 0, 1000)
        });
        unsafe { mark(GC_INPUT.get()) };
    });
}

#[test]
fn fused_numeric_budget_preserves_actual_collection_and_input_mutation() {
    numeric_budget_preserves_actual_collection_and_input_mutation(false);
}

pub(super) fn numeric_budget_preserves_actual_collection_and_input_mutation(unboxed: bool) {
    let bytes = program(false);
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let boxes = birnam_codegen::numeric_chain::Template::derive(&logical, &layout)
        .unwrap()
        .body_boxes();
    let box_size = crate::heap::Heap::allocation_total_size(8).unwrap();
    for gap in [boxes + 2, boxes + 1, boxes, boxes / 2, 1] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(enabled && !unboxed);
            let _raw_mode = super::unboxed_numeric_tests::Mode::set(enabled && unboxed, None);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            r.add_method(
                r.canonical().smallint_vt,
                r.intern(b"blend:").unwrap(),
                native.method(1),
            );
            let block = install(r, &bytes, 741504, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let a = r.make_integer(11);
            let _a = ScopedOopRootGuard::new(r, a);
            let b = r.make_integer(3);
            let _b = ScopedOopRootGuard::new(r, b);
            for _ in 0..128 {
                assert_eq!(
                    r.enter(|| r.integer_value(r.send_slice(
                        block.receiver,
                        block.selector,
                        &[a, b]
                    ))),
                    27
                );
            }
            let machine = r.create_stackless_machine().unwrap();
            let _machine = ScopedOopRootGuard::new(r, machine);
            GC_INPUT.set(a);
            COLLECTIONS.set(0);
            r.gc_register_root_provider(collecting_mutation);
            let current = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let target = crate::gc::AUTO_COLLECTION_THRESHOLD - gap * box_size;
            r.allocate_instance(
                r.globals().object_vt,
                target - current - crate::heap::Heap::allocation_total_size(0).unwrap(),
            );
            assert_eq!(
                unsafe { (*r.state.get()).heap.bytes_since_collection() },
                target
            );
            fused_numeric::HITS.set(0);
            super::super::unboxed_numeric::COMPLETED.set(0);
            let anchor = [0usize; 32];
            r.gc_set_stack_bottom(anchor.as_ptr().addr() + core::mem::size_of_val(&anchor));
            let result = r
                .enter(|| {
                    r.drive_stackless(
                        machine,
                        block.descriptor.clone(),
                        &[(0, block.receiver), (1, a), (2, b)],
                    )
                })
                .unwrap();
            r.gc_set_stack_bottom(0);
            assert_eq!(
                if unboxed {
                    super::super::unboxed_numeric::COMPLETED.get()
                } else {
                    fused_numeric::HITS.get()
                },
                if enabled && gap > boxes + 1 { 3 } else { 0 }
            );
            assert_eq!(COLLECTIONS.get(), usize::from(gap <= boxes));
            evidence.push((
                r.integer_value(result),
                r.integer_value(a),
                COLLECTIONS.get(),
                unsafe { (*r.state.get()).heap.bytes_since_collection() },
            ));
        }
        assert_eq!(evidence[0], evidence[1], "collection boundary gap={gap}");
    }
}

unsafe extern "C" fn propagate_error(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    receiver
}

#[test]
fn fused_numeric_faults_leave_error_effects_at_their_original_calls() {
    numeric_faults_leave_error_effects_at_their_original_calls(false);
}

pub(super) fn numeric_faults_leave_error_effects_at_their_original_calls(unboxed: bool) {
    for (operator, left, right) in [
        (1, i64::MAX, 3),
        (2, i64::MIN, 3),
        (3, i64::MAX, 3),
        (4, i64::MIN, -1),
        (4, 11, 0),
        (5, 11, 0),
    ] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(enabled && !unboxed);
            let _raw_mode = super::unboxed_numeric_tests::Mode::set(enabled && unboxed, None);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            let blend = r.intern(b"blend:").unwrap();
            r.add_method(r.canonical().smallint_vt, blend, native.method(operator));
            r.add_method(
                r.checked_vtable_of(r.canonical().nil),
                blend,
                propagate_error,
            );
            let block = install(r, &program(true), 741505, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let a = r.make_integer(11);
            let _a = ScopedOopRootGuard::new(r, a);
            let b = r.make_integer(3);
            let _b = ScopedOopRootGuard::new(r, b);
            let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[a, b]));
            for _ in 0..128 {
                assert_ne!(invoke(), r.canonical().nil);
            }
            assert!(unsafe {
                (*r.state.get())
                    .heap
                    .write_payload_word(a, 0, left as usize)
            });
            assert!(unsafe {
                (*r.state.get())
                    .heap
                    .write_payload_word(b, 0, right as usize)
            });
            fused_numeric::HITS.set(0);
            super::super::unboxed_numeric::COMPLETED.set(0);
            OBSERVATIONS.set(0);
            let before = r.gc_live_bytes();
            assert_eq!(invoke(), r.canonical().nil);
            assert_eq!(
                if unboxed {
                    super::super::unboxed_numeric::COMPLETED.get()
                } else {
                    fused_numeric::HITS.get()
                },
                0
            );
            assert_eq!(OBSERVATIONS.get(), 2);
            evidence.push(r.gc_live_bytes() - before);
        }
        assert_eq!(evidence[0], evidence[1]);
    }
}

#[test]
fn fused_numeric_preparation_waits_for_hot_chain_starts() {
    for length in [3usize, 16] {
        let mut body = Vec::new();
        for index in 0..length {
            body.push(Statement::Let {
                name: b"v",
                value: Expr::Send {
                    receiver: Box::new(Expr::Global(if index == 0 { b"a" } else { b"v" })),
                    selector: Cow::Borrowed(b"blend:"),
                    args: vec![Expr::Global(b"a")],
                    source: b"",
                },
            });
        }
        body.push(Statement::Expr(Expr::Global(b"v")));
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"a"],
                body,
            },
            &[],
        )
        .unwrap();
        let mut allocation_totals = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(enabled);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            r.add_method(
                r.canonical().smallint_vt,
                r.intern(b"blend:").unwrap(),
                native.method(1),
            );
            let block = install(r, &bytes, 741506, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let input = r.make_integer(1);
            let _input = ScopedOopRootGuard::new(r, input);
            let key = Arc::as_ptr(&block.descriptor) as usize;
            let mut first_fused_call = 0;
            let mut compilation_call_ns = 0;
            let mut cold_calls_ns = 0;
            let mut prepared = false;
            let before = r.gc_live_bytes();
            for call in 1..=128 {
                fused_numeric::HITS.set(0);
                let start = std::time::Instant::now();
                let answer = r.enter(|| r.send_slice(block.receiver, block.selector, &[input]));
                let elapsed = start.elapsed().as_nanos();
                cold_calls_ns += elapsed;
                assert_eq!(r.integer_value(answer), length as i64 + 1);
                let registry = r.native_tier.borrow();
                let record = registry.records.get(&key).unwrap();
                let compiled = record.numeric_candidate.is_some();
                if compiled && !prepared {
                    assert!(enabled && call >= HOT_ENTRIES);
                    assert_eq!(
                        record.numeric_starts, HOT_ENTRIES,
                        "long chains must not count individual PCs toward preparation"
                    );
                    compilation_call_ns = elapsed;
                }
                if !compiled {
                    assert_eq!(fused_numeric::HITS.get(), 0);
                }
                if fused_numeric::HITS.get() != 0 {
                    assert_eq!(fused_numeric::HITS.get(), length);
                    if first_fused_call == 0 {
                        first_fused_call = call;
                    }
                }
                prepared = compiled;
            }
            assert_eq!(prepared, enabled);
            assert_eq!(first_fused_call != 0, enabled);
            let allocation_bytes = r.gc_live_bytes() - before;
            allocation_totals.push(allocation_bytes);
            println!(
                "numeric-preparation length={length} enabled={enabled} calls=128 first_fused_call={first_fused_call} compilation_call_ns={compilation_call_ns} cold_calls_ns={cold_calls_ns} allocation_bytes={allocation_bytes} qualification=diagnostic"
            );
        }
        assert_eq!(allocation_totals[0], allocation_totals[1]);
    }
}
