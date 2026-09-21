//! Design 80N's first control slice, using unchanged compiler-produced V2 body
//! frames, ordinary publisher dispatch, real pin ownership and precise GC.
use super::architectural_fit_experiment::Program;
use super::*;
use crate::execution::{
    FrameDescriptorV2Spec, SemanticRegistrationV1, probe_call, probe_control, probe_spec,
};

thread_local! {
    static BODY_STEP: Cell<Option<Method>> = const { Cell::new(None) };
    static YIELDED: Cell<bool> = const { Cell::new(false) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
    static CAPTURE_BODY: Cell<bool> = const { Cell::new(false) };
    static RESULT_SLOT: Cell<usize> = const { Cell::new(0) };
    static BODY_CONTINUED: Cell<usize> = const { Cell::new(0) };
    static CLEANED: Cell<usize> = const { Cell::new(0) };
}

unsafe extern "C" fn budget_handler_step(machine: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| match r.integer_value(unsafe { args.read() }) {
        0 => r.stackless_boundary_action(machine, 0, false).unwrap(),
        1 => {
            assert_eq!(
                r.stackless_boundary_delivery(machine, false).unwrap(),
                r.canonical().true_object
            );
            r.stackless_boundary_delivery(machine, true).unwrap();
            r.stackless_call_site(machine, 1, CallSiteKind::Call)
                .unwrap()
        }
        2 => r.stackless_return_at(machine, 2).unwrap(),
        _ => unreachable!(),
    })
}

#[test]
fn scheduling_owner_exit_retains_compiler_frames_until_transfer_and_dispatches_cleanup_fresh() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let program = Program::compile(false);
    r.register_stackless_resumable(SemanticRegistrationV1::new(
        thunk,
        program.descriptor(81001, program.step),
        2,
        program.layout.entry_receiver_slot as usize,
        program.layout.entry_argument_start as usize,
    ))
    .unwrap();
    r.add_method(
        r.globals().object_vt,
        r.intern(b"budgetBody:input:").unwrap(),
        thunk,
    );
    r.add_method(
        r.globals().object_vt,
        r.intern(b"budgetCleanup").unwrap(),
        old_cleanup,
    );
    CLEANED.set(0);
    let handler = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        call_sites: vec![
            crate::execution::CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 5,
                argument_count: 2,
                result_slot: Some(2),
                continuation_pc: Some(1),
            },
            crate::execution::CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 4,
                argument_start: 0,
                argument_count: 0,
                result_slot: Some(3),
                continuation_pc: Some(2),
            },
        ],
        edges: vec![probe_call(0, 1, 0, 2, 7), probe_call(1, 2, 1, 3, 7)],
        step_method: budget_handler_step,
        ..probe_spec(7, 3)
    })
    .unwrap();
    r.enter(|| {
        let machine = r.create_stackless_machine().unwrap();
        let _root = ScopedOopRootGuard::new(r, machine);
        let receiver = r.canonical().object_prototype;
        assert_eq!(
            r.drive_stackless_budget(
                machine,
                handler,
                &[
                    (0, receiver),
                    (1, r.intern(b"budgetBody:input:").unwrap()),
                    (4, r.intern(b"budgetCleanup").unwrap()),
                    (5, receiver),
                    (6, r.make_integer(14)),
                ],
                1
            ),
            Ok(machine)
        );
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 2);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 1);
        r.gc_collect_from_native_range(0, 0);
        r.add_method(
            r.globals().object_vt,
            r.intern(b"budgetBody:input:").unwrap(),
            old_cleanup,
        );
        r.add_method(
            r.globals().object_vt,
            r.intern(b"budgetCleanup").unwrap(),
            new_cleanup,
        );
        // The entered implementation remains pinned until its body is abandoned.
        assert_eq!(r.continue_stackless_budget(machine, 0), Ok(machine));
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 1);
        let payload = r.make_string_with_finalizer(
            b"cancelled before next body step",
            crate::public_api::string_finalizer,
        );
        r.exit_stackless_runnable_boundary(machine, payload)
            .unwrap();
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 1);
        assert!(
            r.exit_stackless_runnable_boundary(machine, payload)
                .is_err()
        );
        r.gc_collect_from_native_range(0, 0);
        assert_eq!(r.continue_stackless_budget(machine, 1), Ok(machine));
        assert_eq!(CLEANED.get(), 1);
        assert_eq!(r.continue_stackless_budget(machine, 1), Ok(payload));
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
        r.destroy_stackless_machine(machine).unwrap();
    });
}

fn send_index(r: &Runtime, machine: Oop, name: &[u8], index: usize) -> Oop {
    let index = r.make_integer(index as i64);
    r.send_slice(machine, r.intern(name).unwrap(), &[index])
}

unsafe extern "C" fn body_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    if CANCEL.get() && YIELDED.get() {
        with_active(|r| send_index(r, machine, b"exitBoundaryAt:", RESULT_SLOT.get()))
    } else {
        unsafe { BODY_STEP.get().unwrap()(machine, args, count) }
    }
}

unsafe extern "C" fn park_first_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let call = unsafe { crate::execution::machine_call_site(machine, args, count) };
    if YIELDED.replace(true) {
        return call;
    }
    with_active(|r| {
        let result = r.send_slice(call, r.intern(b"resultSlot").unwrap(), &[]);
        RESULT_SLOT.set(r.integer_value(result) as usize);
        let view = r.send_slice(machine, r.intern(b"currentFrameView").unwrap(), &[]);
        let next = r.send_slice(view, r.intern(b"programCounter").unwrap(), &[]);
        let name = if CAPTURE_BODY.get() {
            b"captureAt:continueAt:".as_slice()
        } else {
            b"suspendAt:continueAt:"
        };
        r.send_slice(machine, r.intern(name).unwrap(), &[result, next])
    })
}

unsafe extern "C" fn thunk(_: Oop, _: *const Oop, _: usize) -> Oop {
    panic!("registered body must be driven")
}
unsafe extern "C" fn observe(_: Oop, _: *const Oop, _: usize) -> Oop {
    BODY_CONTINUED.set(BODY_CONTINUED.get() + 1);
    with_active(|r| r.canonical().nil)
}
unsafe extern "C" fn finish(_: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| r.make_integer(2 * r.integer_value(unsafe { args.read() })))
}
unsafe extern "C" fn old_cleanup(_: Oop, _: *const Oop, _: usize) -> Oop {
    panic!("cleanup must use fresh dispatch")
}
unsafe extern "C" fn new_cleanup(_: Oop, _: *const Oop, _: usize) -> Oop {
    CLEANED.set(CLEANED.get() + 1);
    with_active(|r| r.canonical().nil)
}

unsafe extern "C" fn handler_step(machine: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        let pc = r.integer_value(unsafe { args.read() });
        match pc {
            0 => {
                let action = send_index(r, machine, b"boundaryCallSite:", 0);
                assert_eq!(
                    r.stackless_action_kind(action),
                    Some(r.intern(b"boundaryCall").unwrap())
                );
                action
            }
            1 => {
                assert_eq!(
                    r.stackless_boundary_delivery(machine, false).unwrap(),
                    r.canonical().true_object
                );
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                // The transferred result and cleanup receiver live only in H.
                r.gc_collect_from_native_range(0, 0);
                r.stackless_capture_current(machine, 3, 2).unwrap()
            }
            2 => {
                assert_eq!(
                    r.stackless_boundary_delivery(machine, false).unwrap(),
                    r.canonical().true_object
                );
                assert_eq!(
                    r.stackless_boundary_delivery(machine, true).unwrap(),
                    r.canonical().false_object
                );
                // The fixture represents the library taking ownership of the
                // cleanup shield before acknowledging, then doing a fresh send.
                r.stackless_call_site(machine, 2, CallSiteKind::Call)
                    .unwrap()
            }
            3 => r.stackless_suspend_current(machine, 3, 4).unwrap(),
            4 => send_index(r, machine, b"returnAt:", 2),
            _ => unreachable!(),
        }
    })
}

#[test]
fn boundary_control_compiler_body_cancel_normal_capture_gc_and_cleanup_wait() {
    for capture in [false, true] {
        for cancel in [false, true] {
            YIELDED.set(false);
            CANCEL.set(cancel);
            CAPTURE_BODY.set(capture);
            BODY_CONTINUED.set(0);
            CLEANED.set(0);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let program = Program::compile(false);
            BODY_STEP.set(Some(program.step));
            let descriptor = program.descriptor(80001, body_step);
            r.register_stackless_resumable(SemanticRegistrationV1::new(
                thunk,
                descriptor,
                2,
                program.layout.entry_receiver_slot as usize,
                program.layout.entry_argument_start as usize,
            ))
            .unwrap();
            for (name, method) in [
                (b"boundaryBody:input:".as_slice(), thunk as Method),
                (b"fitObserve:alias:", observe),
                (b"fitFinish:alias:", finish),
                (b"boundaryCleanup", old_cleanup),
            ] {
                r.add_method(r.globals().object_vt, r.intern(name).unwrap(), method);
            }
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.call_site, park_first_call);
            let handler = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                call_sites: vec![
                    crate::execution::CallSiteDescriptor {
                        kind: CallSiteKind::Call,
                        receiver_slot: 0,
                        selector_slot: 1,
                        argument_start: 5,
                        argument_count: 2,
                        result_slot: Some(2),
                        continuation_pc: Some(1),
                    },
                    probe_control(3, 2),
                    crate::execution::CallSiteDescriptor {
                        kind: CallSiteKind::Call,
                        receiver_slot: 0,
                        selector_slot: 4,
                        argument_start: 0,
                        argument_count: 0,
                        result_slot: Some(3),
                        continuation_pc: Some(3),
                    },
                    probe_control(3, 4),
                ],
                edges: vec![
                    probe_call(0, 1, 0, 2, 7),
                    probe_call(1, 2, 1, 3, 7),
                    probe_call(2, 3, 2, 3, 7),
                    probe_call(3, 4, 3, 3, 7),
                ],
                step_method: handler_step,
                ..probe_spec(7, 5)
            })
            .unwrap();
            r.enter(|| {
                let machine = r.create_stackless_machine().unwrap();
                let _machine_root = ScopedOopRootGuard::new(r, machine);
                let receiver = r.canonical().object_prototype;
                let paused = r
                    .drive_stackless(
                        machine,
                        handler,
                        &[
                            (0, receiver),
                            (1, r.intern(b"boundaryBody:input:").unwrap()),
                            (4, r.intern(b"boundaryCleanup").unwrap()),
                            (5, receiver),
                            (6, r.make_integer(14)),
                        ],
                    )
                    .unwrap();
                let _paused_root = ScopedOopRootGuard::new(r, paused);
                let capability = r.stackless_machine_capability(machine).unwrap();
                let diagnostics = r.stackless_registration_diagnostics();
                assert_eq!(diagnostics.active_frame_pins, usize::from(!capture));
                assert_eq!(diagnostics.continuation_frame_pins, usize::from(capture));
                r.gc_collect_from_native_range(0, 0);
                let supplied = r.make_integer(22);
                let delivered = if capture {
                    r.restore_stackless_continuation(machine, paused, supplied)
                        .unwrap()
                } else {
                    r.resume_stackless_machine(machine, supplied).unwrap()
                };
                let _delivered_root = ScopedOopRootGuard::new(r, delivered);
                // H itself captured while delivery was pending. Detached
                // metadata must not look like an active delivery or boundary.
                assert_eq!(
                    r.stackless_boundary_delivery(machine, false).unwrap(),
                    r.canonical().false_object
                );
                assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
                assert_eq!(CLEANED.get(), 0);
                r.gc_collect_from_native_range(0, 0);
                r.add_method(
                    r.globals().object_vt,
                    r.intern(b"boundaryCleanup").unwrap(),
                    new_cleanup,
                );
                let waiting_cleanup = r
                    .restore_stackless_continuation(machine, delivered, r.canonical().nil)
                    .unwrap();
                assert_eq!(waiting_cleanup, machine);
                assert_eq!(CLEANED.get(), 1);
                assert_eq!(
                    r.restore_stackless_continuation(machine, delivered, r.canonical().nil),
                    Err(ExecutionError::ContinuationConsumed)
                );
                r.gc_collect_from_native_range(0, 0);
                let answer = r
                    .resume_stackless_machine(machine, r.canonical().nil)
                    .unwrap();
                assert_eq!(r.integer_value(answer), if cancel { 22 } else { 44 });
                assert_eq!(BODY_CONTINUED.get(), usize::from(!cancel));
                assert_eq!(CLEANED.get(), 1);
                assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                assert_eq!(
                    r.stackless_registration_diagnostics()
                        .continuation_frame_pins,
                    0
                );
                assert!(
                    unsafe {
                        (*r.state.get())
                            .execution
                            .preflight_boundary_exit(current_thread_token(), capability)
                    }
                    .is_err()
                );
                r.destroy_stackless_machine(machine).unwrap();
            });
        }
    }
}

unsafe extern "C" fn compiled_boundary_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let is_handler = with_active(|r| {
        let capability = r.stackless_machine_capability(machine).unwrap();
        let arena = unsafe {
            (*r.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .unwrap()
                .arena()
        };
        arena
            .descriptor(arena.current_handle().unwrap())
            .unwrap()
            .identity()
            .generation
            == 80002
    });
    if is_handler {
        unsafe { crate::execution::boundary::machine_boundary_call(machine, args, count) }
    } else {
        unsafe { park_first_call(machine, args, count) }
    }
}

unsafe extern "C" fn cleanup_step(machine: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        if r.integer_value(unsafe { args.read() }) == 0 {
            // The compiled wrapper tail-called this handler: its delivery guard
            // must survive frame replacement, then this independent wait.
            assert_eq!(
                r.stackless_boundary_delivery(machine, false).unwrap(),
                r.canonical().true_object
            );
            r.stackless_boundary_delivery(machine, true).unwrap();
            CLEANED.set(CLEANED.get() + 1);
            r.stackless_suspend_current(machine, 2, 1).unwrap()
        } else {
            r.stackless_return_at(machine, 1).unwrap()
        }
    })
}

unsafe extern "C" fn cleanup_thunk(_: Oop, _: *const Oop, _: usize) -> Oop {
    panic!("cleanup must be resumable")
}

#[test]
fn boundary_control_compiler_caller_preserves_v2_and_tail_handler_delivery() {
    YIELDED.set(false);
    CANCEL.set(true);
    CAPTURE_BODY.set(true);
    CLEANED.set(0);
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let caller = Program::compile(true);
    let body = Program::compile(false);
    BODY_STEP.set(Some(body.step));
    r.register_stackless_resumable(SemanticRegistrationV1::new(
        thunk,
        body.descriptor(80001, body_step),
        2,
        body.layout.entry_receiver_slot as usize,
        body.layout.entry_argument_start as usize,
    ))
    .unwrap();
    r.add_method(
        r.globals().object_vt,
        r.intern(b"fitCallee:input:").unwrap(),
        thunk,
    );
    let cleanup = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        entry_argument_count: 1,
        call_sites: vec![probe_control(2, 1)],
        edges: vec![probe_call(0, 1, 0, 2, 3)],
        step_method: cleanup_step,
        ..probe_spec(3, 2)
    })
    .unwrap();
    r.register_stackless_resumable(SemanticRegistrationV1::new(cleanup_thunk, cleanup, 1, 0, 1))
        .unwrap();
    r.add_method(
        r.globals().object_vt,
        r.intern(b"fitAfter:").unwrap(),
        cleanup_thunk,
    );
    let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
    r.add_method(
        objects.machine_vtable,
        objects.call_site,
        compiled_boundary_call,
    );
    r.enter(|| {
        let machine = r.create_stackless_machine().unwrap();
        let _machine_root = ScopedOopRootGuard::new(r, machine);
        let receiver = r.canonical().object_prototype;
        let captured = r
            .drive_stackless(
                machine,
                caller.descriptor(80002, caller.step),
                &[
                    (caller.layout.entry_receiver_slot as usize, receiver),
                    (caller.layout.entry_argument_start as usize, receiver),
                    (
                        caller.layout.entry_argument_start as usize + 1,
                        r.make_integer(14),
                    ),
                ],
            )
            .unwrap();
        let _captured_root = ScopedOopRootGuard::new(r, captured);
        r.gc_collect_from_native_range(0, 0);
        // Retire the dispatch binding while its old activation is captured.
        r.add_method(
            r.globals().object_vt,
            r.intern(b"fitCallee:input:").unwrap(),
            old_cleanup,
        );
        assert_eq!(
            r.stackless_registration_diagnostics()
                .continuation_frame_pins,
            1
        );
        assert_eq!(
            r.restore_stackless_continuation(machine, captured, r.make_integer(91))
                .unwrap(),
            machine
        );
        assert_eq!(CLEANED.get(), 1);
        r.gc_collect_from_native_range(0, 0);
        let answer = r
            .resume_stackless_machine(machine, r.canonical().nil)
            .unwrap();
        assert_eq!(r.integer_value(answer), 91);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(
            r.stackless_registration_diagnostics()
                .continuation_frame_pins,
            0
        );
        r.destroy_stackless_machine(machine).unwrap();
    });
}
