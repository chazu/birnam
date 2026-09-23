//! Retained source/control acceptance cases for compiler and Store reconnection.
use super::*;

unsafe extern "C" fn answer(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.make_integer(42))
}

unsafe extern "C" fn native_context_probe(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.send_slice(machine, r.intern(b"controlEligible").unwrap(), &[]))
}

#[test]
fn task_control_eligibility_distinguishes_steps_from_native_callbacks() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    r.enter(|| {
        register(r);
        let machine = r.create_stackless_machine().unwrap();
        let _root = ScopedOopRootGuard::new(r, machine);
        assert_eq!(
            r.drive_stackless(
                machine,
                descriptor(eligible_step, 0, false),
                &[(0, machine)]
            ),
            Ok(r.canonical().true_object)
        );
        r.destroy_stackless_machine(machine).unwrap();

        let machine = r.create_stackless_machine().unwrap();
        let _root = ScopedOopRootGuard::new(r, machine);
        let vtable = unsafe { (*r.state.get()).execution.objects().unwrap().machine_vtable };
        r.add_method(vtable, r.intern(b"value").unwrap(), native_context_probe);
        assert_eq!(
            r.send_slice(machine, r.intern(b"runBlock:").unwrap(), &[machine]),
            r.canonical().false_object
        );
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        r.destroy_stackless_machine(machine).unwrap();
    });
}

#[test]
fn task_control_adapters_return_release_pins_and_preserve_replacements() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    r.enter(|| {
        let vtable = unsafe { (*r.state.get()).execution.objects().unwrap().machine_vtable };
        let park_selector = r.intern(b"park").unwrap();
        // First-time preparation also respects an intentionally supplied
        // binding; it must not quietly install policy over user code.
        r.add_method(vtable, park_selector, answer);
        r.add_method(r.globals().object_vt, r.intern(b"value").unwrap(), answer);
        for _ in 0..2 {
            let machine = r.create_stackless_machine().unwrap();
            let _root = ScopedOopRootGuard::new(r, machine);
            let result = r.send_slice(
                machine,
                r.intern(b"runBlock:").unwrap(),
                &[r.canonical().object_prototype],
            );
            assert_eq!(r.integer_value(result), 42);
            assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(
                unsafe {
                    (*r.state.get())
                        .vtables
                        .direct_lookup(vtable, park_selector)
                },
                Some(MethodAddress::from_method(answer))
            );
            r.destroy_stackless_machine(machine).unwrap();
        }
        let replacement = r.intern(b"protectedBlock:").unwrap();
        r.add_method(vtable, replacement, answer);
        register(r);
        assert_eq!(
            unsafe { (*r.state.get()).vtables.direct_lookup(vtable, replacement) },
            Some(MethodAddress::from_method(answer))
        );
    });
}

#[test]
fn task_control_adapter_rejects_a_different_machine_receiver() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    r.enter(|| {
        let machine = r.create_stackless_machine().unwrap();
        let _machine_root = ScopedOopRootGuard::new(r, machine);
        let other = r.create_stackless_machine().unwrap();
        let _other_root = ScopedOopRootGuard::new(r, other);
        let result = r.drive_stackless(
            machine,
            descriptor(leave_step, 1, false),
            &[(0, other), (1, r.canonical().nil)],
        );
        assert_eq!(result, Err(ExecutionError::InvalidAction));
        // Rejected execution remains inspectable until explicit disposal.
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 1);
        r.destroy_stackless_machine(machine).unwrap();
        // The other Machine is still new, so explicit disposal (which is
        // reserved for suspended/completed/failed execution) rejects it.
        assert_eq!(
            r.destroy_stackless_machine(other),
            Err(MachineError::InvalidLifecycle)
        );
        assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
    });
}
