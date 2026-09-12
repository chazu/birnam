//! Design 60P: V2 control through ordinary Machine messages and the real driver.
use super::architectural_fit_experiment::Program;
use super::*;
use crate::execution::{
    EdgeTransitionKind, FrameDescriptorV2Spec, FrameEdgeSpec, SlotCopy, probe_control,
};

thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
    static YIELDED: Cell<bool> = const { Cell::new(false) };
    static PUBLICATIONS: Cell<usize> = const { Cell::new(0) };
}

fn publish(r: &Runtime, machine: Oop, result: usize, next: usize) -> Oop {
    let capability = r.stackless_machine_capability(machine).unwrap();
    let owner = current_thread_token();
    let before = unsafe {
        (*r.state.get())
            .execution
            .machine(owner, capability)
            .unwrap()
            .arena()
            .recovery_experiment_snapshot()
    };
    assert!(
        r.stackless_suspend_current(machine, result, usize::MAX)
            .is_err()
    );
    assert!(
        r.stackless_capture_current(machine, usize::MAX, next)
            .is_err()
    );
    assert_eq!(
        unsafe {
            (*r.state.get())
                .execution
                .machine(owner, capability)
                .unwrap()
                .arena()
                .recovery_experiment_snapshot()
        },
        before
    );
    let slot = r.make_integer(result as i64);
    let pc = r.make_integer(next as i64);
    let selector = if CAPTURE.get() {
        b"captureAt:continueAt:".as_slice()
    } else {
        b"suspendAt:continueAt:"
    };
    let action = r.send_slice(machine, r.intern(selector).unwrap(), &[slot, pc]);
    PUBLICATIONS.set(PUBLICATIONS.get() + 1);
    // A valid slot is not enough: the final action must identify the exact
    // pending result destination. Reject a tampered action before transfer.
    unsafe {
        let state = &mut *r.state.get();
        let word = if CAPTURE.get() { 6 } else { 4 };
        assert!(state.heap.write_payload_word(action, word, result ^ 1));
        assert!(
            state
                .execution
                .decode_action(&state.heap, owner, capability, action)
                .is_err()
        );
        assert!(state.heap.write_payload_word(action, word, result));
        state
            .execution
            .decode_action(&state.heap, owner, capability, action)
            .unwrap();
    }
    action
}

unsafe extern "C" fn yielding_call_site(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let call = unsafe { crate::execution::machine_call_site(machine, args, count) };
    if YIELDED.replace(true) {
        return call;
    }
    with_active(|r| {
        // This override consumes ordinary Id reflection on the already-staged
        // compiler CallAction; no compiler selector special case is involved.
        let result = r.send_slice(call, r.intern(b"resultSlot").unwrap(), &[]);
        let view = r.send_slice(machine, r.intern(b"currentFrameView").unwrap(), &[]);
        let pc = r.send_slice(view, r.intern(b"programCounter").unwrap(), &[]);
        publish(
            r,
            machine,
            r.integer_value(result) as usize,
            r.integer_value(pc) as usize,
        )
    })
}

unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    receiver
}
unsafe extern "C" fn observe(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    assert_eq!(unsafe { args.read() }, unsafe { args.add(1).read() });
    with_active(|r| r.canonical().nil)
}
unsafe extern "C" fn finish(_: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| r.make_integer(2 * r.integer_value(unsafe { args.read() })))
}

fn resume(r: &Runtime, machine: Oop, paused: Oop, value: Oop) -> Oop {
    if CAPTURE.get() {
        let wrong_machine = r.create_stackless_machine().unwrap();
        assert_eq!(
            r.restore_stackless_continuation(wrong_machine, paused, value),
            Err(ExecutionError::ContinuationWrongMachine)
        );
        let wrong_capability = r.stackless_machine_capability(wrong_machine).unwrap();
        unsafe {
            (*r.state.get())
                .execution
                .machine_mut(current_thread_token(), wrong_capability)
                .unwrap()
                .fail(current_thread_token())
                .unwrap();
        }
        r.destroy_stackless_machine(wrong_machine).unwrap();
        // Failed admission did not consume or disturb the saved segment.
        let answer = r
            .restore_stackless_continuation(machine, paused, value)
            .unwrap();
        assert_eq!(
            r.restore_stackless_continuation(machine, paused, value),
            Err(ExecutionError::ContinuationConsumed)
        );
        answer
    } else {
        assert_eq!(paused, machine);
        let answer = r.resume_stackless_machine(machine, value).unwrap();
        assert_eq!(
            r.resume_stackless_machine(machine, value),
            Err(ExecutionError::MachineNotSuspended)
        );
        answer
    }
}

#[test]
fn compiler_frames_suspend_and_capture_through_ordinary_machine_sends() {
    for capture in [false, true] {
        CAPTURE.set(capture);
        YIELDED.set(false);
        PUBLICATIONS.set(0);
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let program = Program::compile(false);
        let descriptor = program.descriptor(601601, program.step);
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(
            objects.machine_vtable,
            objects.call_site,
            yielding_call_site,
        );
        for (name, method) in [
            (b"fitIdentity".as_slice(), identity as Method),
            (b"fitObserve:alias:", observe),
            (b"fitFinish:alias:", finish),
        ] {
            r.add_method(r.globals().object_vt, r.intern(name).unwrap(), method);
        }
        r.enter(|| {
            let machine = r.create_stackless_machine().unwrap();
            let _machine_root = ScopedOopRootGuard::new(r, machine);
            let input = r.make_integer(14);
            let receiver = r.canonical().object_prototype;
            let paused = r.drive_stackless(machine, descriptor, &[
                (program.layout.entry_receiver_slot as usize, receiver),
                (program.layout.entry_argument_start as usize, receiver),
                (program.layout.entry_argument_start as usize + 1, input),
            ]).unwrap();
            let _paused_root = ScopedOopRootGuard::new(r, paused);
            r.gc_collect_from_native_range(0, 0);
            assert_eq!(r.integer_value(input), 14, "predecessor roots survive while awaiting the Call result");
            let supplied = r.make_integer(22);
            let answer = resume(r, machine, paused, supplied);
            assert_eq!(r.integer_value(answer), 44);
            assert_eq!(PUBLICATIONS.get(), 1);
            assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
            r.destroy_stackless_machine(machine).unwrap();
            println!("V2_CONTROL {{\"case\":\"compiler\",\"capture\":{capture},\"answer\":44,\"ordinary_publications\":1,\"precise_gc\":true}}");
        });
    }
}

unsafe extern "C" fn copy_step(machine: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        if r.integer_value(unsafe { args.read() }) == 0 {
            return publish(r, machine, 3, 1);
        }
        assert_eq!(r.integer_value(r.stackless_oop_at(machine, 0).unwrap()), 20);
        assert_eq!(r.integer_value(r.stackless_oop_at(machine, 1).unwrap()), 10);
        assert_eq!(r.stackless_oop_at(machine, 2).unwrap(), r.canonical().nil);
        assert_eq!(r.stackless_oop_at(machine, 4).unwrap(), r.canonical().nil);
        r.gc_collect_from_native_range(0, 0);
        let result = r.stackless_oop_at(machine, 3).unwrap();
        assert_eq!(
            r.integer_value(result),
            99,
            "successor roots retain the supplied result"
        );
        let index = r.make_integer(3);
        r.send_slice(machine, r.intern(b"returnAt:").unwrap(), &[index])
    })
}

#[test]
fn paused_v2_edges_commit_cyclic_copies_clears_and_root_changes_once() {
    for capture in [false, true] {
        CAPTURE.set(capture);
        PUBLICATIONS.set(0);
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            root_bitmap_words: vec![0b00111, 0b01011],
            call_sites: vec![probe_control(3, 1)],
            edges: vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Call,
                source_pc: 0,
                target_pc: 1,
                call_site_index: Some(0),
                result_slot: Some(3),
                copies: vec![
                    SlotCopy {
                        source: 1,
                        destination: 0,
                    },
                    SlotCopy {
                        source: 0,
                        destination: 1,
                    },
                ],
                clear_bitmap_words: vec![0b10100],
            }],
            step_method: copy_step,
            ..crate::execution::probe_spec(5, 2)
        })
        .unwrap();
        r.enter(|| {
            let machine = r.create_stackless_machine().unwrap();
            let _machine_root = ScopedOopRootGuard::new(r, machine);
            let a = r.make_integer(10); let b = r.make_integer(20); let c = r.make_integer(30); let garbage = r.make_integer(40);
            let paused = r.drive_stackless(machine, descriptor, &[(0, a), (1, b), (2, c), (4, garbage)]).unwrap();
            let _paused_root = ScopedOopRootGuard::new(r, paused);
            r.gc_collect_from_native_range(0, 0);
            assert!(!r.checked_vtable_of(a).is_null()); assert!(!r.checked_vtable_of(b).is_null()); assert!(!r.checked_vtable_of(c).is_null());
            assert!(r.checked_vtable_of(garbage).is_null(), "untraced slot must not retain its dead value");
            let supplied = r.make_integer(99);
            let answer = resume(r, machine, paused, supplied);
            assert_eq!(r.integer_value(answer), 99);
            r.destroy_stackless_machine(machine).unwrap();
            println!("V2_CONTROL {{\"case\":\"cyclic-copies\",\"capture\":{capture},\"answer\":99,\"ordinary_publications\":1,\"precise_gc\":true}}");
        });
    }
}
