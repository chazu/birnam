use super::*;
use crate::execution::{
    FrameDescriptorV2Spec, MachineLifecycle, probe_call, probe_continue, probe_control, probe_spec,
};

thread_local! {
    static TURNS: Cell<usize> = const { Cell::new(0) };
    static FINISH: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "C" fn loop_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        TURNS.set(TURNS.get() + 1);
        if FINISH.get() {
            r.stackless_return_at(machine, 0).unwrap()
        } else {
            r.stackless_continue_at(machine, 0).unwrap()
        }
    })
}

fn loop_descriptor() -> std::sync::Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        step_method: loop_step,
        edges: vec![probe_continue(0, 0, 1)],
        ..probe_spec(1, 1)
    })
    .unwrap()
}

fn lifecycle(r: &Runtime, m: Oop) -> MachineLifecycle {
    let cap = r.stackless_machine_capability(m).unwrap();
    unsafe {
        (*r.state.get())
            .execution
            .machine(current_thread_token(), cap)
            .unwrap()
            .lifecycle()
    }
}

unsafe extern "C" fn nested_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        TURNS.set(TURNS.get() + 1);
        assert_eq!(lifecycle(r, machine), MachineLifecycle::Running);
        if TURNS.get() == 3 {
            r.stackless_return_at(machine, 0).unwrap()
        } else {
            r.stackless_continue_at(machine, 0).unwrap()
        }
    })
}

unsafe extern "C" fn nested_thunk(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| unsafe {
        r.enter_registered_resumable(nested_thunk, receiver, args, count)
            .unwrap()
    })
}

unsafe extern "C" fn native_nested(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.send_slice(receiver, r.intern(b"nestedBudgetProbe").unwrap(), &[]))
}

#[test]
fn scheduling_budget_waits_for_native_callback_to_return() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    TURNS.set(0);
    let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        step_method: nested_step,
        edges: vec![probe_continue(0, 0, 1)],
        ..probe_spec(1, 1)
    })
    .unwrap();
    r.register_stackless_resumable(SemanticRegistrationV1::new(
        nested_thunk,
        descriptor,
        0,
        0,
        0,
    ))
    .unwrap();
    r.add_method(
        r.globals().object_vt,
        r.intern(b"nestedBudgetProbe").unwrap(),
        nested_thunk,
    );
    r.add_method(
        r.globals().object_vt,
        r.intern(b"value").unwrap(),
        native_nested,
    );
    r.enter(|| {
        let machine = r.create_stackless_machine().unwrap();
        let _root = ScopedOopRootGuard::new(r, machine);
        let receiver = r.canonical().object_prototype;
        assert_eq!(
            r.send_slice(
                machine,
                r.intern(b"runBlock:budget:").unwrap(),
                &[receiver, r.make_integer(1)]
            ),
            machine
        );
        assert_eq!(
            TURNS.get(),
            3,
            "the entire synchronous callback must finish before stopping"
        );
        assert_eq!(lifecycle(r, machine), MachineLifecycle::Runnable);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(r.continue_stackless_budget(machine, 1), Ok(receiver));
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
        r.destroy_stackless_machine(machine).unwrap();
    });
}

#[test]
fn scheduling_budget_interleaves_without_wait_results_and_keeps_precise_roots() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    TURNS.set(0);
    FINISH.set(false);
    r.enter(|| {
        let a = r.create_stackless_machine().unwrap();
        let _a = ScopedOopRootGuard::new(r, a);
        let b = r.create_stackless_machine().unwrap();
        let _b = ScopedOopRootGuard::new(r, b);
        let value = r.make_string_with_finalizer(
            b"retained by stopped frame",
            crate::public_api::string_finalizer,
        );
        assert_eq!(
            r.drive_stackless_budget(a, loop_descriptor(), &[(0, value)], 0),
            Ok(a)
        );
        assert_eq!(TURNS.get(), 0);
        assert_eq!(lifecycle(r, a), MachineLifecycle::Runnable);
        assert_eq!(
            r.drive_stackless_budget(b, loop_descriptor(), &[(0, value)], 2),
            Ok(b)
        );
        assert_eq!(TURNS.get(), 2);
        for expected in 3..8 {
            let task = if expected % 2 == 0 { b } else { a };
            assert_eq!(r.continue_stackless_budget(task, 1), Ok(task));
            assert_eq!(TURNS.get(), expected);
            r.gc_collect_from_native_range(0, 0);
        }
        // A timeslice is not a pending Call; injecting a result must reject.
        assert_eq!(
            r.resume_stackless_budget(a, r.canonical().nil, 1),
            Err(ExecutionError::MachineNotSuspended)
        );
        assert_eq!(lifecycle(r, a), MachineLifecycle::Runnable);
        FINISH.set(true);
        assert_eq!(r.continue_stackless_budget(a, 1), Ok(value));
        assert_eq!(r.continue_stackless_budget(b, 1), Ok(value));
        assert_eq!(lifecycle(r, a), MachineLifecycle::Completed);
        assert!(r.continue_stackless_budget(a, 1).is_err());
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
        r.destroy_stackless_machine(a).unwrap();
        r.destroy_stackless_machine(b).unwrap();
    });
}

unsafe extern "C" fn wait_step(machine: Oop, args: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        if r.integer_value(unsafe { args.read() }) == 0 {
            r.stackless_suspend_current(machine, 0, 1).unwrap()
        } else {
            r.stackless_return_at(machine, 0).unwrap()
        }
    })
}

#[test]
fn scheduling_budget_distinguishes_wait_and_runnable_resume() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    r.enter(|| {
        let machine = r.create_stackless_machine().unwrap();
        let _root = ScopedOopRootGuard::new(r, machine);
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            step_method: wait_step,
            call_sites: vec![probe_control(0, 1)],
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .unwrap();
        assert_eq!(
            r.drive_stackless_budget(machine, descriptor, &[], 1),
            Ok(machine)
        );
        assert_eq!(lifecycle(r, machine), MachineLifecycle::Suspended);
        assert!(r.continue_stackless_budget(machine, 1).is_err());
        assert!(
            r.exit_stackless_runnable_boundary(machine, r.canonical().nil)
                .is_err()
        );
        assert_eq!(lifecycle(r, machine), MachineLifecycle::Suspended);
        let answer = r.make_integer(42);
        assert_eq!(r.resume_stackless_budget(machine, answer, 0), Ok(machine));
        assert_eq!(lifecycle(r, machine), MachineLifecycle::Runnable);
        assert_eq!(r.continue_stackless_budget(machine, 1), Ok(answer));
        r.destroy_stackless_machine(machine).unwrap();
    });
}
