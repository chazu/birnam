//! Block adapters for the existing checked Machine operations. Task outcomes,
//! cancellation, shields and wait ownership remain ordinary Birnam objects.
//! Protected calls and waits use descriptor V2 frames. The native start/resume
//! entries return to their caller when that Machine parks.
use super::*;
use crate::execution::{
    CallSiteDescriptor, DescriptorIdentity, EdgeTransitionKind, FrameDescriptorV2Spec,
    FrameEdgeSpec,
};
use std::sync::Arc;

pub(super) fn install(r: &Runtime, vtable: Oop) {
    for (name, method) in [
        (b"prepareBlockControl".as_slice(), prepare as Method),
        (b"runBlock:".as_slice(), run as Method),
        (b"resumeWith:", resume as Method),
    ] {
        r.add_method(vtable, r.intern(name).expect("static selector"), method);
    }
}

// Slots: receiver Machine, block/exit value, #value selector, returned value.
// The only edge retains all four Oops and initializes the last slot. These are
// ordinary descriptor V2 frames, prepared by the source control unit at startup.
fn descriptor(step: Method, arity: usize, returns: bool) -> Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: step as usize as u64,
            generation: 0,
            source_map: 0,
        },
        program_counter_count: if returns { 2 } else { 1 },
        driver_argument_count: 1,
        slot_count: 4,
        entry_receiver_slot: 0,
        entry_argument_start: 1,
        entry_argument_count: arity,
        entry_capture_slots: vec![],
        implementation_slot: None,
        root_bitmap_words: vec![15; if returns { 2 } else { 1 }],
        call_sites: if returns {
            vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 1,
                selector_slot: 2,
                argument_start: 0,
                argument_count: 0,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }]
        } else {
            vec![]
        },
        edges: if returns {
            vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Call,
                source_pc: 0,
                target_pc: 1,
                call_site_index: Some(0),
                result_slot: Some(3),
                copies: vec![],
                clear_bitmap_words: vec![0],
            }]
        } else {
            vec![]
        },
        step_method: step,
    })
    .expect("block adapter descriptor")
}

fn register(r: &Runtime) {
    for (name, thunk, step, arity, returns) in [
        (
            b"controlEligible".as_slice(),
            eligible as Method,
            eligible_step as Method,
            0,
            false,
        ),
        (
            b"protectedBlock:".as_slice(),
            protected as Method,
            protected_step as Method,
            1,
            true,
        ),
        (
            b"leaveBoundary:".as_slice(),
            leave as Method,
            leave_step as Method,
            1,
            false,
        ),
        (
            b"park".as_slice(),
            park as Method,
            park_step as Method,
            0,
            true,
        ),
    ] {
        // The registration owns process-lifetime code; repeated runBlock: does
        // not replace registrations or existing user method bindings.
        if !unsafe {
            (*r.state.get())
                .execution
                .has_resumable_thunk(MethodAddress::from_method(thunk))
        } {
            r.register_stackless_resumable(SemanticRegistrationV1::new(
                thunk,
                descriptor(step, arity, returns),
                arity,
                0,
                1,
            ))
            .expect("block adapter registration");
            let vtable = unsafe { (*r.state.get()).execution.objects().unwrap().machine_vtable };
            let selector = r.intern(name).expect("static selector");
            if unsafe {
                (*r.state.get())
                    .vtables
                    .direct_lookup(vtable, selector)
                    .is_none()
            } {
                r.add_method(vtable, selector, thunk);
            }
        }
    }
}

unsafe extern "C" fn prepare(receiver: Oop, _: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        if count != 0 {
            return rejected(r, ExecutionError::InvalidAction);
        }
        register(r);
        receiver
    })
}

unsafe extern "C" fn run(machine: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        if count != 1 || args.is_null() {
            return rejected(r, ExecutionError::InvalidAction);
        }
        if let Err(error) = r.stackless_machine_capability(machine) {
            return rejected(r, error.into());
        }
        register(r);
        let block = unsafe { args.read() };
        r.drive_stackless(
            machine,
            descriptor(run_step, 1, true),
            &[
                (0, machine),
                (1, block),
                (2, r.intern(b"value").expect("static selector")),
            ],
        )
        .unwrap_or_else(|error| rejected(r, error))
    })
}

unsafe extern "C" fn resume(machine: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        if count != 1 || args.is_null() {
            return rejected(r, ExecutionError::InvalidAction);
        }
        r.resume_stackless_machine(machine, unsafe { args.read() })
            .unwrap_or_else(|error| rejected(r, error))
    })
}

// Native entry is used only when invoked across an ordinary entry barrier.
// The existing driver then rejects any attempted park/exit across that barrier.
macro_rules! entry {
    ($name:ident, $operation:literal) => {
        unsafe extern "C" fn $name(receiver: Oop, args: *const Oop, count: usize) -> Oop {
            with_active(|r| unsafe {
                r.enter_registered_resumable($name, receiver, args, count)
                    .unwrap_or_else(|error| rejected_in(r, $operation, error))
            })
        }
    };
}
// These entries must be semantically distinct, not just refer to their own
// function addresses: release LTO can merge self-referencing identical bodies.
// Operation-specific rejection diagnostics both identify the failed operation
// and preserve the separate entry identities required by the registry.
entry!(protected, "protectedBlock:");
entry!(leave, "leaveBoundary:");
entry!(park, "park");
entry!(eligible, "controlEligible");

unsafe extern "C" fn eligible_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        if r.stackless_oop_at(machine, 0).ok() != Some(machine) {
            return r.canonical().nil;
        }
        let capability = r
            .stackless_machine_capability(machine)
            .expect("active Machine");
        let eligible = unsafe {
            (*r.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .expect("active owner")
                .control_eligible()
        };
        let value = if eligible {
            r.canonical().true_object
        } else {
            r.canonical().false_object
        };
        r.stackless_oop_at_put(machine, 3, value)
            .expect("adapter result slot");
        publish(r, machine, b"returnAt:", 3)
    })
}

fn rejected(r: &Runtime, error: ExecutionError) -> Oop {
    rejected_in(r, "Machine", error)
}

fn rejected_in(r: &Runtime, operation: &str, error: ExecutionError) -> Oop {
    let Ok(factory) = r.import(b"Error") else {
        return r.canonical().nil;
    };
    let kind = r.intern(b"machineControlRejected").expect("static symbol");
    let message = r.make_string_with_finalizer(
        format!("{operation}: {error:?}").as_bytes(),
        crate::public_api::string_finalizer,
    );
    let _message_root = ScopedOopRootGuard::new(r, message);
    r.send_slice(
        factory,
        r.intern(b"kind:message:").expect("static selector"),
        &[kind, message],
    )
}

fn publish(r: &Runtime, machine: Oop, name: &[u8], index: usize) -> Oop {
    let index = r.make_integer(index as i64);
    r.send_slice(machine, r.intern(name).expect("static selector"), &[index])
}

fn invoke_step(machine: Oop, args: *const Oop, count: usize, boundary: bool) -> Oop {
    with_active(|r| {
        if count != 1 || args.is_null() || r.stackless_oop_at(machine, 0).ok() != Some(machine) {
            return r.canonical().nil;
        }
        if r.integer_value(unsafe { args.read() }) == 0 {
            r.stackless_oop_at_put(machine, 2, r.intern(b"value").expect("static selector"))
                .expect("adapter selector slot");
            publish(
                r,
                machine,
                if boundary {
                    b"boundaryCallSite:"
                } else {
                    b"callSite:"
                },
                0,
            )
        } else {
            publish(r, machine, b"returnAt:", 3)
        }
    })
}
unsafe extern "C" fn run_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    invoke_step(machine, args, count, false)
}
unsafe extern "C" fn protected_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    invoke_step(machine, args, count, true)
}
unsafe extern "C" fn leave_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        if r.stackless_oop_at(machine, 0).ok() != Some(machine) {
            return r.canonical().nil;
        }
        publish(r, machine, b"exitBoundaryAt:", 1)
    })
}
unsafe extern "C" fn park_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        if count != 1 || args.is_null() || r.stackless_oop_at(machine, 0).ok() != Some(machine) {
            return r.canonical().nil;
        }
        if r.integer_value(unsafe { args.read() }) == 0 {
            let slot = r.make_integer(3);
            let pc = r.make_integer(1);
            r.send_slice(
                machine,
                r.intern(b"suspendAt:continueAt:").expect("static selector"),
                &[slot, pc],
            )
        } else {
            publish(r, machine, b"returnAt:", 3)
        }
    })
}

#[cfg(test)]
mod tests {
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
}
