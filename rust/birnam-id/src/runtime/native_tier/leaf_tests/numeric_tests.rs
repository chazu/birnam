use super::*;
use crate::runtime::numeric;
use crate::runtime::numeric::tests::Fixture;

thread_local! { static OBSERVATIONS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn observing_lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

struct Mode(bool);
impl Drop for Mode {
    fn drop(&mut self) {
        numeric::ENABLED.set(self.0);
    }
}

#[test]
fn production_numeric_calls_use_current_targets_and_preserve_observing_lookup() {
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"input")),
                        selector: Cow::Borrowed(b"blend:"),
                        args: vec![Expr::Global(b"input")],
                        source: b"input blend: input",
                    },
                },
                Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"first")),
                    selector: Cow::Borrowed(b"blend:"),
                    args: vec![Expr::Global(b"input")],
                    source: b"first blend: input",
                }),
            ],
        },
        &[],
    )
    .unwrap();
    let mut evidence = Vec::new();
    for observing in [false, true] {
        for enabled in [false, true] {
            let _mode = Mode(numeric::ENABLED.replace(enabled));
            numeric::HITS.set(0);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            let blend = r.intern(b"blend:").unwrap();
            r.add_method(r.canonical().smallint_vt, blend, native.method(1));
            let block = install(r, &bytes, 741500, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let input = r.make_integer(11);
            let _input = ScopedOopRootGuard::new(r, input);
            if observing {
                r.add_method(r.globals().vtable_vt, r.globals().lookup, observing_lookup);
            }
            let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[input]));
            for _ in 0..128 {
                assert_eq!(r.integer_value(invoke()), 33);
            }
            OBSERVATIONS.set(0);
            numeric::HITS.set(0);
            numeric::CHAIN_HITS.set(0);
            let before = r.gc_live_bytes();
            for _ in 0..16 {
                assert_eq!(r.integer_value(invoke()), 33);
            }
            let allocation_bytes = r.gc_live_bytes() - before;
            assert_eq!(numeric::HITS.get() > 0, enabled);
            assert_eq!(
                numeric::CHAIN_HITS.get(),
                if enabled && !observing { 32 } else { 0 }
            );
            assert_eq!(OBSERVATIONS.get() > 0, observing);
            evidence.push((allocation_bytes, OBSERVATIONS.get()));
            // A familiar or unfamiliar selector says nothing about its meaning.
            // Rebinding the same site to subtraction changes the next answer.
            r.add_method(r.canonical().smallint_vt, blend, native.method(2));
            assert_eq!(r.integer_value(invoke()), -11);
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert_eq!(r.integer_value(invoke()), -11);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        }
    }
    assert_eq!(evidence[0], evidence[1], "canonical allocation parity");
    assert_eq!(
        evidence[2], evidence[3],
        "custom lookup effects and allocation parity"
    );
}

#[derive(Clone, Copy)]
enum Change {
    Operand,
    Operator,
    Error,
    Collect,
    Declaration,
}
thread_local! {
    static CHANGE: Cell<(usize, Change)> = const { Cell::new((usize::MAX, Change::Operand)) };
    static REPLACEMENT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static INPUT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static SUBTRACT: Cell<Option<Method>> = const { Cell::new(None) };
    static DECLARATION: Cell<Option<unsafe extern "C" fn(u32)>> = const { Cell::new(None) };
    static TRACE: core::cell::RefCell<Vec<(usize, i64, i64)>> = const { core::cell::RefCell::new(Vec::new()) };
    static CHAIN_OWNER: core::cell::RefCell<std::rc::Weak<numeric::CompiledChain>> = const { core::cell::RefCell::new(std::rc::Weak::new()) };
}

unsafe extern "C" fn chain_collection(mark: crate::abi::MarkFn) {
    with_active(|r| {
        let input = INPUT.get();
        assert!(unsafe { (*r.state.get()).heap.write_payload_word(input, 0, 10) });
        // Active predictions own their metadata/declarations independently of
        // both caches. Cache clearing must not free an active owner or leave a
        // dangling native declaration for subsequent validation.
        *r.numeric.borrow_mut() = Default::default();
        *r.native_tier.borrow_mut() = Default::default();
        assert_eq!(
            CHAIN_OWNER.with_borrow(|owner| owner.upgrade().is_some()),
            numeric::ENABLED.get() && CHANGE.get().0 > 0
        );
        unsafe { mark(input) };
    });
}

fn before_publication(machine: Oop, operand: Oop) {
    with_active(|r| {
        let _operand = ScopedOopRootGuard::new(r, operand);
        let site = r.integer_value(operand) as usize;
        let (cut, change) = CHANGE.get();
        if cut == site {
            match change {
                Change::Operand => {
                    let cap = r.stackless_machine_capability(machine).unwrap();
                    let state = unsafe { &mut *r.state.get() };
                    let arena = state
                        .execution
                        .machine_mut(current_thread_token(), cap)
                        .unwrap()
                        .arena_mut();
                    let frame = arena.current_handle().unwrap();
                    let slot = arena
                        .descriptor(frame)
                        .unwrap()
                        .call_site(site)
                        .unwrap()
                        .receiver_slot;
                    arena.write_oop(frame, slot, REPLACEMENT.get()).unwrap();
                }
                Change::Operator => {
                    r.add_method(
                        r.canonical().smallint_vt,
                        r.intern(b"blend:").unwrap(),
                        SUBTRACT.get().unwrap(),
                    );
                }
                Change::Error => {
                    r.export(b"ErrorVt", r.canonical().smallint_vt).unwrap();
                }
                Change::Collect => {
                    r.gc_collect_from_native_range(0, 0);
                }
                Change::Declaration => unsafe { DECLARATION.get().unwrap()(0) },
            }
        }
        let cap = r.stackless_machine_capability(machine).unwrap();
        let state = unsafe { &*r.state.get() };
        let arena = state
            .execution
            .machine(current_thread_token(), cap)
            .unwrap()
            .arena();
        let frame = arena.current_handle().unwrap();
        let call = arena.descriptor(frame).unwrap().call_site(site).unwrap();
        let left = arena.read_oop(frame, call.receiver_slot).unwrap();
        let right = arena.read_oop(frame, call.argument_start).unwrap();
        TRACE.with_borrow_mut(|trace| {
            trace.push((site, r.integer_value(left), r.integer_value(right)))
        });
    });
}
unsafe extern "C" fn observing_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    before_publication(machine, unsafe { *args });
    unsafe { crate::execution::machine_call_site(machine, args, count) }
}
unsafe extern "C" fn observing_tail(machine: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    before_publication(machine, unsafe { *args });
    unsafe { crate::execution::machine_tail_call_site(machine, args, count) }
}

#[test]
fn production_raw_chains_revalidate_after_changes_at_every_operation() {
    let send = |left| Expr::Send {
        receiver: Box::new(Expr::Global(left)),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![Expr::Global(b"a")],
        source: b"",
    };
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"a"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: send(b"a"),
                },
                Statement::Let {
                    name: b"second",
                    value: send(b"first"),
                },
                Statement::Expr(send(b"second")),
            ],
        },
        &[],
    )
    .unwrap();
    for change in [
        Change::Operand,
        Change::Operator,
        Change::Error,
        Change::Collect,
        Change::Declaration,
    ] {
        for cut in 0..3 {
            let mut evidence = Vec::new();
            for enabled in [false, true] {
                let _mode = Mode(numeric::ENABLED.replace(enabled));
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
                let block = install(r, &bytes, 741501, None);
                let _block = ScopedOopRootGuard::new(r, block.receiver);
                let input = r.make_integer(3);
                let _input = ScopedOopRootGuard::new(r, input);
                let replacement = r.make_integer(10);
                let _replacement = ScopedOopRootGuard::new(r, replacement);
                let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[input]));
                for _ in 0..128 {
                    assert_eq!(r.integer_value(invoke()), 12);
                }
                // Warm the replacement declaration so the test distinguishes
                // operator mismatch from the unrelated discovery hotness gate.
                for _ in 0..65 {
                    r.enter(|| r.try_numeric_call(native.method(2), input, &[input]));
                }
                let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                r.add_method(objects.machine_vtable, objects.call_site, observing_call);
                r.add_method(
                    objects.machine_vtable,
                    objects.tail_call_site,
                    observing_tail,
                );
                CHANGE.set((cut, change));
                INPUT.set(input);
                REPLACEMENT.set(replacement);
                SUBTRACT.set(Some(native.method(2)));
                DECLARATION.set(Some(native.declaration_toggle()));
                TRACE.with_borrow_mut(Vec::clear);
                if matches!(change, Change::Collect) {
                    let key = Arc::as_ptr(&block.descriptor) as usize;
                    CHAIN_OWNER.with_borrow_mut(|owner| {
                        *owner = r
                            .native_tier
                            .borrow()
                            .records
                            .get(&key)
                            .and_then(|record| record.numeric_candidate.as_ref())
                            .map(Rc::downgrade)
                            .unwrap_or_default();
                    });
                    r.gc_register_root_provider(chain_collection);
                }
                numeric::CHAIN_HITS.set(0);
                let before = r.gc_live_bytes();
                let native_before = native.calls();
                let result = invoke();
                if matches!(change, Change::Declaration) {
                    assert_eq!(
                        native.calls() - native_before,
                        if enabled { 3 - cut } else { 3 },
                        "withdrawn declarations must execute the original native calls"
                    );
                }
                let answer = r.integer_value(result);
                let expected = match change {
                    Change::Operand => [19, 16, 13][cut],
                    Change::Operator => [-6, 0, 6][cut],
                    Change::Error => 3,
                    Change::Collect => [40, 26, 19][cut],
                    Change::Declaration => 12,
                };
                assert_eq!(answer, expected);
                assert_eq!(numeric::CHAIN_HITS.get(), if enabled { cut } else { 0 });
                assert_eq!(result == input, matches!(change, Change::Error));
                let allocation = if matches!(change, Change::Collect) {
                    unsafe { (*r.state.get()).heap.bytes_since_collection() }
                } else {
                    r.gc_live_bytes() - before
                };
                evidence.push((answer, TRACE.with_borrow(Clone::clone), allocation));
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                if matches!(change, Change::Collect) {
                    assert!(
                        CHAIN_OWNER.with_borrow(|owner| owner.upgrade().is_none()),
                        "evicted metadata and native pins are released after the drive"
                    );
                }
                CHANGE.set((usize::MAX, Change::Operand));
                if matches!(change, Change::Declaration) {
                    unsafe { native.declaration_toggle()(1) };
                    numeric::CHAIN_HITS.set(0);
                    assert_eq!(r.integer_value(invoke()), 12);
                    assert_eq!(
                        numeric::CHAIN_HITS.get(),
                        if enabled { 3 } else { 0 },
                        "restoring a declaration can reuse its guarded cached plan"
                    );
                }
            }
            assert_eq!(
                evidence[0], evidence[1],
                "values, ordered observation and allocation parity at {cut}"
            );
        }
    }
}

#[test]
#[ignore = "diagnostic paired production-chain timing; qualification is external"]
fn production_numeric_chain_timing() {
    struct ChainMode(bool);
    impl Drop for ChainMode {
        fn drop(&mut self) {
            numeric::CHAIN_ENABLED.set(self.0);
        }
    }
    let batches = std::env::var("BENCH_NUMERIC_BATCHES")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((1..=500).contains(&batches));
    let operator = std::env::var("BENCH_NUMERIC_OPERATOR").unwrap_or_else(|_| "add".to_owned());
    assert!(matches!(operator.as_str(), "add" | "divide"));
    let varying = std::env::var("BENCH_NUMERIC_VARYING").is_ok_and(|value| value == "1");
    for length in [3, 16] {
        let mut body = Vec::new();
        for i in 0..length {
            body.push(Statement::Let {
                name: b"v",
                value: Expr::Send {
                    receiver: Box::new(Expr::Global(if i == 0 { b"a" } else { b"v" })),
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
        for pair in 0..4 {
            for enabled in if pair % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let _mode = ChainMode(numeric::CHAIN_ENABLED.replace(enabled));
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                let native = Fixture::load(r);
                r.add_method(
                    r.canonical().smallint_vt,
                    r.intern(b"blend:").unwrap(),
                    native.method(if operator == "divide" { 4 } else { 1 }),
                );
                let block = install(r, &bytes, 741502, None);
                let _block = ScopedOopRootGuard::new(r, block.receiver);
                let input = r.make_integer(1);
                let _input = ScopedOopRootGuard::new(r, input);
                let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[input]));
                let expected = |value| {
                    if operator == "divide" {
                        i64::from(value == 1)
                    } else {
                        value * i64::from(length + 1)
                    }
                };
                for _ in 0..256 {
                    assert_eq!(r.integer_value(invoke()), expected(1));
                }
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                numeric::CHAIN_HITS.set(0);
                let mut ns = 0;
                let mut allocations = 0;
                for _ in 0..batches {
                    let before = r.gc_live_bytes();
                    let start = std::time::Instant::now();
                    for iteration in 0..2000 {
                        let value = if varying { 1 + iteration % 32 } else { 1 };
                        assert!(unsafe {
                            (*r.state.get())
                                .heap
                                .write_payload_word(input, 0, value as usize)
                        });
                        assert_eq!(r.integer_value(invoke()), expected(value));
                    }
                    ns += start.elapsed().as_nanos();
                    allocations += r.gc_live_bytes() - before;
                    // Keep each measured batch below the collection threshold.
                    // Collection is outside the measured arithmetic interval;
                    // application/automatic-GC measurements remain separate.
                    r.enter(|| r.gc_collect_from_native_range(0, 0));
                }
                let hits = numeric::CHAIN_HITS.get();
                let calls = batches * 2000;
                assert_eq!(hits, if enabled { length as usize * calls } else { 0 });
                println!(
                    "numeric-chain length={length} pair={pair} enabled={enabled} calls={calls} ns={ns} allocation_bytes={allocations} hits={hits} operator={operator} varying={varying} collection=outside-batch"
                );
            }
        }
    }
}
