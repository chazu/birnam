use super::*;
use crate::execution::{FrameViewError, tail_entry};
use std::cell::RefCell;

struct Mode(bool);
impl Drop for Mode {
    fn drop(&mut self) {
        tail_entry::ENABLED.set(self.0);
    }
}
#[derive(Clone, Copy, Debug)]
enum Change {
    None,
    Capture,
    Lookup,
    Rebind,
    Collect,
    CaptureControl,
    SuspendControl,
}
thread_local! {
    static REMAINING: Cell<usize> = const { Cell::new(0) };
    static ITERATION: Cell<usize> = const { Cell::new(0) };
    static CHANGE: Cell<Change> = const { Cell::new(Change::None) };
    static STOP: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static REPLACEMENT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static CAPTURE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static LAST_VIEW: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static TRACE: RefCell<Vec<(usize, i64, i64, i64, u64)>> = const { RefCell::new(Vec::new()) };
    static LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    LOOKUP_COUNT.set(LOOKUP_COUNT.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn finish(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    unsafe { *args.add(2) }
}
unsafe extern "C" fn next(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_active(|r| {
        let machine = MACHINE.get();
        if !LAST_VIEW.get().is_null() {
            let state = unsafe { &*r.state.get() };
            assert!(matches!(
                state.execution.frame_view_snapshot(
                    &state.heap,
                    current_thread_token(),
                    LAST_VIEW.get()
                ),
                Err(FrameViewError::StaleFrameView)
            ));
        }
        let view = r.stackless_current_frame_view(machine).unwrap();
        LAST_VIEW.set(view);
        r.export(b"LastLoopView", view).unwrap();
        let snapshot = unsafe {
            let state = &*r.state.get();
            state
                .execution
                .frame_view_snapshot(&state.heap, current_thread_token(), view)
                .unwrap()
        };
        TRACE.with_borrow_mut(|trace| {
            trace.push((
                REMAINING.get(),
                r.integer_value(args[1]),
                r.integer_value(args[2]),
                r.integer_value(args[3]),
                snapshot.identity.generation,
            ))
        });
        if ITERATION.get() == 2 {
            match CHANGE.get() {
                Change::Capture => {
                    assert!(unsafe {
                        (*r.state.get())
                            .heap
                            .write_payload_word(args[0], 1, CAPTURE.get().addr())
                    });
                }
                Change::Lookup => {
                    r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup);
                }
                Change::Rebind => {
                    let selector = r.intern(b"value:value:value:value:").unwrap();
                    let method = r
                        .lookup_method(r.checked_vtable_of(REPLACEMENT.get()), selector)
                        .unwrap();
                    r.add_method(r.checked_vtable_of(args[0]), selector, method.to_method());
                }
                Change::CaptureControl | Change::SuspendControl => {
                    let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                    r.add_method(
                        objects.machine_vtable,
                        objects.call_site,
                        if matches!(CHANGE.get(), Change::CaptureControl) {
                            capture_call
                        } else {
                            suspend_call
                        },
                    );
                }
                Change::Collect => {
                    r.gc_collect_from_native_range(0, 0);
                }
                Change::None => {}
            }
        }
        ITERATION.set(ITERATION.get() + 1);
        if REMAINING.get() == 0 {
            STOP.get()
        } else {
            REMAINING.set(REMAINING.get() - 1);
            args[0]
        }
    })
}
fn program() -> Vec<u8> {
    portable::encode_block(
        &Block {
            params: vec![b"again", b"state", b"left", b"right"],
            body: vec![
                Statement::Let {
                    name: b"target",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"state")),
                        selector: Cow::Borrowed(b"next:saved:left:right:"),
                        args: vec![
                            Expr::Global(b"again"),
                            Expr::Global(b"saved"),
                            Expr::Global(b"left"),
                            Expr::Global(b"right"),
                        ],
                        source: b"",
                    },
                },
                Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"target")),
                    selector: Cow::Borrowed(b"value:value:value:value:"),
                    args: vec![
                        Expr::Global(b"again"),
                        Expr::Global(b"state"),
                        Expr::Global(b"right"),
                        Expr::Global(b"left"),
                    ],
                    source: b"",
                }),
            ],
        },
        &[b"saved".to_vec()],
    )
    .unwrap()
}

#[test]
fn tail_loop_entry_plan_preserves_observers_capture_changes_and_rebinding() {
    for iterations in [0, 1, 128] {
        for change in [
            Change::None,
            Change::Capture,
            Change::Lookup,
            Change::Rebind,
            Change::Collect,
            Change::CaptureControl,
            Change::SuspendControl,
        ] {
            let mut evidence = Vec::new();
            for enabled in [false, true] {
                let _mode = Mode(tail_entry::ENABLED.replace(enabled));
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                let a = r.make_integer(7);
                let _a = ScopedOopRootGuard::new(r, a);
                let b = r.make_integer(9);
                let _b = ScopedOopRootGuard::new(r, b);
                let block = install(r, &program(), 741710, Some(a));
                let _block = ScopedOopRootGuard::new(r, block.receiver);
                let replacement = install(r, &program(), 741712, Some(a));
                let _replacement = ScopedOopRootGuard::new(r, replacement.receiver);
                let control_vt = r.create_vtable(r.globals().object_vt);
                r.add_method(
                    control_vt,
                    r.intern(b"next:saved:left:right:").unwrap(),
                    next,
                );
                let control = r.allocate_instance(control_vt, 0);
                let _control = ScopedOopRootGuard::new(r, control);
                let stop_vt = r.create_vtable(r.globals().object_vt);
                r.add_method(stop_vt, block.selector, finish);
                let stop = r.allocate_instance(stop_vt, 0);
                let _stop = ScopedOopRootGuard::new(r, stop);
                // Equal-valued arguments remain distinct and rotate by identity.
                let left = r.make_integer(11);
                let _left = ScopedOopRootGuard::new(r, left);
                let right = r.make_integer(11);
                let _right = ScopedOopRootGuard::new(r, right);
                assert_ne!(left, right);
                REMAINING.set(iterations);
                ITERATION.set(0);
                CHANGE.set(change);
                STOP.set(stop);
                REPLACEMENT.set(replacement.receiver);
                CAPTURE.set(b);
                LAST_VIEW.set(core::ptr::null_mut());
                TRACE.with_borrow_mut(Vec::clear);
                LOOKUP_COUNT.set(0);
                tail_entry::HITS.set(0);
                let machine = r.create_stackless_machine().unwrap();
                let _machine = ScopedOopRootGuard::new(r, machine);
                let binding = block.descriptor.v2_entry_binding().unwrap();
                let mut initial = vec![(binding.receiver_slot, block.receiver)];
                initial.extend(
                    [block.receiver, control, left, right]
                        .into_iter()
                        .enumerate()
                        .map(|(i, value)| (binding.argument_start + i, value)),
                );
                initial.push((block.descriptor.entry_capture_slots()[0], a));
                let before = r.gc_live_bytes();
                let mut result = r
                    .enter(|| r.drive_stackless(machine, block.descriptor.clone(), &initial))
                    .unwrap();
                let interrupted = iterations > 2
                    && matches!(change, Change::CaptureControl | Change::SuspendControl);
                if interrupted {
                    assert_eq!(
                        ITERATION.get(),
                        3,
                        "no later loop effect runs across suspension or capture"
                    );
                    assert_eq!(tail_entry::HITS.get(), if enabled { 3 } else { 0 });
                    let pins = r.stackless_registration_diagnostics();
                    assert_eq!(
                        pins.active_frame_pins,
                        usize::from(matches!(change, Change::SuspendControl))
                    );
                    assert_eq!(
                        pins.continuation_frame_pins,
                        usize::from(matches!(change, Change::CaptureControl))
                    );
                    let _result = ScopedOopRootGuard::new(r, result);
                    let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                    r.add_method(
                        objects.machine_vtable,
                        objects.call_site,
                        crate::execution::machine_call_site,
                    );
                    r.enter(|| r.gc_collect_from_native_range(0, 0));
                    result = if matches!(change, Change::CaptureControl) {
                        r.enter(|| {
                            r.restore_stackless_continuation(machine, result, block.receiver)
                        })
                        .unwrap()
                    } else {
                        assert_eq!(result, machine);
                        r.enter(|| r.resume_stackless_machine(machine, block.receiver))
                            .unwrap()
                    };
                }
                let turns = iterations + usize::from(interrupted);
                assert_eq!(result, if turns % 2 == 0 { right } else { left });
                assert_eq!(tail_entry::HITS.get(), if enabled { turns } else { 0 });
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
                let trace = TRACE.with_borrow(Clone::clone);
                assert_eq!(trace.len(), iterations + 1);
                if iterations > 2 && matches!(change, Change::Capture) {
                    assert_eq!(trace[3].1, 9);
                }
                if iterations > 2 && matches!(change, Change::Rebind) {
                    assert_eq!(trace[3].4, 741712);
                }
                let capability = r.stackless_machine_capability(machine).unwrap();
                let diagnostics = unsafe {
                    (*r.state.get())
                        .execution
                        .machine_diagnostics(current_thread_token(), capability)
                }
                .unwrap();
                // Collection compares retained bytes rather than a subtraction
                // that can underflow when baseline garbage is reclaimed.
                let bytes = if matches!(change, Change::Collect) || interrupted {
                    r.gc_live_bytes()
                } else {
                    r.gc_live_bytes() - before
                };
                evidence.push((trace, LOOKUP_COUNT.get(), bytes, diagnostics));
            }
            assert_eq!(
                evidence[0], evidence[1],
                "{change:?} iterations={iterations}"
            );
        }
    }
}

unsafe extern "C" fn suspend_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| {
        let cap = r.stackless_machine_capability(machine).unwrap();
        let (result, pc) = unsafe {
            let state = &*r.state.get();
            let arena = state
                .execution
                .machine(current_thread_token(), cap)
                .unwrap()
                .arena();
            (
                state.heap.read_payload_word(action, 8).unwrap(),
                arena
                    .program_counter(arena.current_handle().unwrap())
                    .unwrap(),
            )
        };
        r.stackless_suspend_current(machine, result, pc).unwrap()
    })
}

unsafe extern "C" fn next_timed(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 4);
    if REMAINING.get() == 0 {
        STOP.get()
    } else {
        REMAINING.set(REMAINING.get() - 1);
        unsafe { *args }
    }
}

#[test]
#[ignore = "paired loop timing; host qualification is external"]
fn tail_loop_entry_timing() {
    let batches = std::env::var("BENCH_TAIL_BATCHES")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((1..=500).contains(&batches));
    for pair in 0..4 {
        for enabled in if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let _mode = Mode(tail_entry::ENABLED.replace(enabled));
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let saved = r.make_integer(7);
            let _saved = ScopedOopRootGuard::new(r, saved);
            let block = install(r, &program(), 741720, Some(saved));
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let control_vt = r.create_vtable(r.globals().object_vt);
            r.add_method(
                control_vt,
                r.intern(b"next:saved:left:right:").unwrap(),
                next_timed,
            );
            let control = r.allocate_instance(control_vt, 0);
            let _control = ScopedOopRootGuard::new(r, control);
            let stop_vt = r.create_vtable(r.globals().object_vt);
            r.add_method(stop_vt, block.selector, finish);
            let stop = r.allocate_instance(stop_vt, 0);
            let _stop = ScopedOopRootGuard::new(r, stop);
            STOP.set(stop);
            let left = r.make_integer(11);
            let _left = ScopedOopRootGuard::new(r, left);
            let right = r.make_integer(11);
            let _right = ScopedOopRootGuard::new(r, right);
            let invoke = |iterations| {
                REMAINING.set(iterations);
                r.enter(|| {
                    r.send_slice(
                        block.receiver,
                        block.selector,
                        &[block.receiver, control, left, right],
                    )
                })
            };
            let metadata_before = block.descriptor.owned_byte_size();
            let prepare_start = std::time::Instant::now();
            assert_eq!(invoke(128), right);
            let cold_ns = prepare_start.elapsed().as_nanos();
            let metadata_bytes = block.descriptor.owned_byte_size() - metadata_before;
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            let mut ns = 0;
            let mut allocated = 0;
            tail_entry::HITS.set(0);
            for _ in 0..batches {
                let before = r.gc_live_bytes();
                let start = std::time::Instant::now();
                assert_eq!(invoke(1000), right);
                ns += start.elapsed().as_nanos();
                allocated += r.gc_live_bytes() - before;
                assert!(
                    unsafe { (*r.state.get()).heap.bytes_since_collection() }
                        < crate::gc::AUTO_COLLECTION_THRESHOLD
                );
                r.enter(|| r.gc_collect_from_native_range(0, 0));
            }
            let transitions = batches * 1000;
            let hits = tail_entry::HITS.get();
            assert_eq!(hits, if enabled { transitions } else { 0 });
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            println!(
                "tail-entry pair={pair} enabled={enabled} transitions={transitions} ns={ns} allocation_bytes={allocated} hits={hits} cold_ns={cold_ns} metadata_bytes={metadata_bytes} collection=outside-batch"
            );
        }
    }
}
