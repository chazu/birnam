use super::*;
use crate::execution::ExecutionError;
use crate::runtime::CodeGenerationRetirement;

pub(super) unsafe extern "C" fn suspend_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
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

#[test]
fn loop_reuse_capture_suspend_and_native_retirement_preserve_frames_and_identity() {
    let (numeric_program, parent_program) = programs();
    for tails in [false, true] {
        for unboxed in [false, true] {
            for capture in [false, true] {
                for retire in [false, true] {
                    for precise in [false, true] {
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
                            r.gc_set_precise(precise);
                            let fixture = Fixture::load(r);
                            r.add_method(
                                r.canonical().smallint_vt,
                                r.intern(b"blend:").unwrap(),
                                fixture.method(1),
                            );
                            let calc = install(r, &numeric_program, 741930, None);
                            let replacement = install(r, &numeric_program, 741934, None);
                            let _calc = ScopedOopRootGuard::new(r, calc.receiver);
                            let _replacement = ScopedOopRootGuard::new(r, replacement.receiver);
                            let parent = install(r, &parent_program, 741932, Some(calc.receiver));
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
                            let arguments = [parent.receiver, state, a, b];
                            CHANGE.set(Change::None);
                            LIMIT.set(256);
                            ITERATION.set(0);
                            ANSWER.set(core::ptr::null_mut());
                            SUPPLIED.set(core::ptr::null_mut());
                            assert_eq!(
                                r.integer_value(r.enter(|| r.send_slice(
                                    parent.receiver,
                                    parent.selector,
                                    &arguments
                                ))),
                                36
                            );
                            let numeric_owner = {
                                let tier = r.native_tier.borrow();
                                let key = Arc::as_ptr(&calc.descriptor) as usize;
                                Rc::downgrade(
                                    tier.records
                                        .get(&key)
                                        .unwrap()
                                        .numeric_candidate
                                        .as_ref()
                                        .unwrap(),
                                )
                            };
                            let parent_owner = {
                                let tier = r.native_tier.borrow();
                                let key = Arc::as_ptr(&parent.descriptor) as usize;
                                Rc::downgrade(
                                    tier.records.get(&key).unwrap().candidate.as_ref().unwrap(),
                                )
                            };
                            CHANGE.set(if capture {
                                Change::Capture
                            } else {
                                Change::Suspend
                            });
                            LIMIT.set(8);
                            ITERATION.set(0);
                            TRACE.with_borrow_mut(Vec::clear);
                            loop_reuse::PREDICTED_OPERATIONS_REUSED.set(0);
                            fused_numeric::HITS.set(0);
                            unboxed_numeric::COMPLETED.set(0);
                            let machine = r.create_stackless_machine().unwrap();
                            let _machine = ScopedOopRootGuard::new(r, machine);
                            let baseline = r.stackless_registration_diagnostics();
                            let StacklessTarget::Registered { entry } = r
                                .enter(|| {
                                    r.resolve_stackless_target(parent.receiver, parent.selector)
                                })
                                .unwrap()
                            else {
                                panic!("registered loop")
                            };
                            let target = r
                                .materialize_registered_target(entry, parent.receiver, &arguments)
                                .unwrap();
                            let paused = r
                                .enter(|| {
                                    r.drive_stackless_with_resolver(
                                        machine,
                                        target.descriptor,
                                        &target.initial_oops,
                                        Some(entry),
                                        |r, receiver, selector, _| {
                                            r.resolve_stackless_target(receiver, selector)
                                        },
                                    )
                                })
                                .unwrap();
                            let _paused = ScopedOopRootGuard::new(r, paused);
                            assert_eq!(
                                ITERATION.get(),
                                3,
                                "later loop effects must wait for resumption"
                            );
                            assert_eq!(
                                loop_reuse::PREDICTED_OPERATIONS_REUSED.get(),
                                if enabled { 6 } else { 0 }
                            );
                            assert!(
                                fused_numeric::HITS.get() > 0
                                    || unboxed_numeric::COMPLETED.get() > 0
                            );
                            let pins = r.stackless_registration_diagnostics();
                            assert_eq!(
                                pins.active_frame_pins,
                                baseline.active_frame_pins + usize::from(!capture)
                            );
                            assert_eq!(
                                pins.continuation_frame_pins,
                                baseline.continuation_frame_pins + usize::from(capture)
                            );
                            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                            r.add_method(
                                objects.machine_vtable,
                                objects.call_site,
                                crate::execution::machine_call_site,
                            );
                            let parent_method = r
                                .lookup_method(
                                    r.checked_vtable_of(parent.receiver),
                                    parent.selector,
                                )
                                .unwrap();
                            if retire {
                                let replacement_method = r
                                    .lookup_method(
                                        r.checked_vtable_of(replacement.receiver),
                                        replacement.selector,
                                    )
                                    .unwrap();
                                r.add_method(
                                    r.checked_vtable_of(calc.receiver),
                                    calc.selector,
                                    replacement_method.to_method(),
                                );
                                assert_eq!(
                                    r.enter(|| r.request_stackless_code_generation_retirement(
                                        calc.generation
                                    )),
                                    CodeGenerationRetirement::Retired
                                );
                                assert!(
                                    numeric_owner.upgrade().is_none(),
                                    "retirement releases raw caches and native numeric owners"
                                );
                                r.remove_method(
                                    r.checked_vtable_of(parent.receiver),
                                    parent.selector,
                                );
                                assert_eq!(
                                    r.enter(|| r.request_stackless_code_generation_retirement(
                                        parent.generation
                                    )),
                                    CodeGenerationRetirement::Rejected,
                                    "paused frames own the original code"
                                );
                                r.add_method(
                                    r.checked_vtable_of(parent.receiver),
                                    parent.selector,
                                    parent_method.to_method(),
                                );
                            }
                            r.enter(|| r.gc_collect_from_native_range(0, 0));
                            assert_eq!(r.integer_value(ANSWER.get()), 36);
                            let supplied = r.make_integer(36);
                            let _supplied = ScopedOopRootGuard::new(r, supplied);
                            SUPPLIED.set(supplied);
                            let answer = if capture {
                                let answer = r
                                    .enter(|| {
                                        r.restore_stackless_continuation(machine, paused, supplied)
                                    })
                                    .unwrap();
                                assert_eq!(
                                    r.enter(|| r
                                        .restore_stackless_continuation(machine, paused, supplied)),
                                    Err(ExecutionError::ContinuationConsumed)
                                );
                                answer
                            } else {
                                assert_eq!(paused, machine);
                                let answer = r
                                    .enter(|| r.resume_stackless_machine(machine, supplied))
                                    .unwrap();
                                assert_eq!(
                                    r.enter(|| r.resume_stackless_machine(machine, supplied)),
                                    Err(ExecutionError::MachineNotSuspended)
                                );
                                answer
                            };
                            assert_eq!(r.integer_value(answer), 36);
                            assert_ne!(answer, supplied);
                            assert_eq!(ITERATION.get(), 8);
                            assert_eq!(
                                loop_reuse::PREDICTED_OPERATIONS_REUSED.get(),
                                if enabled {
                                    if retire { 6 } else { 15 }
                                } else {
                                    0
                                },
                                "resumption must reestablish loop evidence before reuse"
                            );
                            assert!(
                                parent_owner.upgrade().is_some(),
                                "paused code stayed owned through resume"
                            );
                            let after = r.stackless_registration_diagnostics();
                            assert_eq!(after.active_frame_pins, baseline.active_frame_pins);
                            assert_eq!(
                                after.continuation_frame_pins,
                                baseline.continuation_frame_pins
                            );
                            assert_eq!(after.acquired_entries, baseline.acquired_entries);
                            if retire {
                                r.remove_method(
                                    r.checked_vtable_of(parent.receiver),
                                    parent.selector,
                                );
                                assert_eq!(
                                    r.enter(|| r.request_stackless_code_generation_retirement(
                                        parent.generation
                                    )),
                                    CodeGenerationRetirement::Retired
                                );
                                assert!(
                                    parent_owner.upgrade().is_none(),
                                    "successful caller retirement releases its native candidate"
                                );
                            }
                            traces.push(TRACE.with_borrow(Clone::clone));
                            r.destroy_stackless_machine(machine).unwrap();
                        }
                        assert_eq!(
                            traces[0], traces[1],
                            "identical effect order across control and retirement"
                        );
                    }
                }
            }
        }
    }
}
