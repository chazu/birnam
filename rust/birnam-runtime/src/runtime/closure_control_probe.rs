//! Test-floor bridge for real source-created closures and host-owned Machines.
//! Absent from production builds; uses the ordinary driver and recovery path.
use super::*;
use core::cell::RefCell;

#[derive(Default)]
struct Probe {
    machine: Oop,
    closure: Oop,
    input: Oop,
    supplied: Oop,
    literal_mode: bool,
    literal_results: Vec<Oop>,
    literal_aliases: Vec<Vec<Vec<usize>>>,
    capture: bool,
    observations: usize,
    aliases: Vec<Vec<usize>>,
}
thread_local! {
    // Intentionally untraced: only the actual Frame/Continuation may retain the
    // recovered closure during collection. The fixture also uses precise GC.
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

impl Runtime {
    pub(crate) fn install_closure_control_probe(&self, api: *mut crate::abi::IdApi) {
        self.enter(|| unsafe { super::closure_api_probe::attach(api) });
        let vt = self.create_vtable(self.globals().object_vt);
        for (name, method) in [
            (b"warm:input:".as_slice(), warm as Method),
            (
                b"actionState:input:".as_slice(),
                super::closure_action_probe::run as Method,
            ),
            (b"run:input:capture:".as_slice(), run as Method),
            (
                b"runLiterals:input:capture:".as_slice(),
                run_literals as Method,
            ),
            (
                b"inspectLiterals:second:third:alias:closure:closureAlias:".as_slice(),
                observe_literals as Method,
            ),
            (
                b"inspectLiterals:second:third:alias:closure:closureAlias:supplied:".as_slice(),
                observe_literals as Method,
            ),
            (b"inspect:alias:".as_slice(), observe as Method),
            (b"inspect:alias:supplied:".as_slice(), observe as Method),
            (b"checkpoint".as_slice(), checkpoint as Method),
            (
                b"prepare:captures:".as_slice(),
                super::closure_timing_probe::prepare as Method,
            ),
            (
                b"measure:input:shape:batches:".as_slice(),
                super::closure_timing_probe::measure as Method,
            ),
            (
                b"sequence:input:other:calls:escape:".as_slice(),
                super::closure_timing_probe::sequence as Method,
            ),
            (
                b"sequenceJournal:calls:observe:capacity:".as_slice(),
                super::closure_journal_probe::run as Method,
            ),
            (
                b"inspectJournal:".as_slice(),
                super::closure_journal_probe::inspect as Method,
            ),
            (
                b"journalRestored:calls:".as_slice(),
                super::closure_journal_probe::restored as Method,
            ),
            (
                b"sequenceBudget:gap:escape:".as_slice(),
                super::closure_budget_probe::check as Method,
            ),
            (
                b"sequenceMutation:input:other:position:collect:".as_slice(),
                super::closure_sequence_probe::check as Method,
            ),
            (
                b"checkApi:input:operation:".as_slice(),
                super::closure_api_probe::check as Method,
            ),
        ] {
            self.add_method(vt, self.intern(name).unwrap(), method);
        }
        #[cfg(feature = "allocation-metrics")]
        super::consumer_lifetime_probe::install(self, vt);
        let object = self.allocate_instance(vt, 0);
        self.export(b"ClosureControlProbe", object).unwrap();
    }
}

unsafe extern "C" fn warm(probe: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    let (block, input) = unsafe { (*args, *args.add(1)) };
    with_source_probe_native_roots(|r| {
        let selector = r.intern(b"value:value:").unwrap();
        for _ in 0..128 {
            let answer = r.send_slice(block, selector, &[input, probe]);
            assert_ne!(answer, r.canonical().nil);
        }
        r.canonical().nil
    })
}

unsafe extern "C" fn checkpoint(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    assert!(
        PROBE.with_borrow(Option::is_none),
        "paused Call must await supplied result"
    );
    with_source_probe_native_roots(|r| r.canonical().nil)
}

unsafe extern "C" fn observe(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert!((2..=3).contains(&count));
    let closure = unsafe { *args };
    assert_eq!(closure, unsafe { *args.add(1) });
    with_source_probe_native_roots(|r| {
        let Some((machine, expected, input, supplied, observations)) = PROBE.with_borrow(|p| {
            p.as_ref()
                .map(|p| (p.machine, p.closure, p.input, p.supplied, p.observations))
        }) else {
            return closure;
        };
        if observations != 0 {
            assert_eq!(closure, expected, "resume must preserve recovered identity");
            assert_eq!(count, 3);
            assert_eq!(
                unsafe { *args.add(2) },
                supplied,
                "resume supplies the pending Call's result"
            );
        }
        let cap = r.stackless_machine_capability(machine).unwrap();
        let aliases = || unsafe {
            let arena = (*r.state.get())
                .execution
                .machine(current_thread_token(), cap)
                .unwrap()
                .arena();
            let frame = arena.current_handle().unwrap();
            (0..arena.descriptor(frame).unwrap().slot_count())
                .filter(|&slot| arena.read_oop(frame, slot).unwrap() == closure)
                .collect::<Vec<_>>()
        };
        let before = aliases();
        assert!(before.len() >= 2);
        r.gc_collect_for_source_probe();
        assert_eq!(aliases(), before);
        assert_eq!(
            unsafe { (*r.state.get()).heap.read_payload_word(closure, 1) },
            Some(input.addr())
        );
        PROBE.with_borrow_mut(|p| {
            let p = p.as_mut().unwrap();
            p.closure = closure;
            p.observations += 1;
            p.aliases.push(before);
        });
        if observations == 0 {
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.call_site, pause_call);
        }
        closure
    })
}

// Only real source Frames/Continuations retain these result Oops during GC.
unsafe extern "C" fn observe_literals(probe: Oop, args: *const Oop, count: usize) -> Oop {
    assert!((6..=7).contains(&count));
    let values = unsafe { core::slice::from_raw_parts(args, count) };
    assert_eq!(values[0], values[3]);
    assert_ne!(values[0], values[1]);
    assert_ne!(values[0], values[2]);
    assert_ne!(values[1], values[2]);
    with_source_probe_native_roots(|r| {
        for &value in &values[..3] {
            assert_eq!(r.integer_value(value), 42);
        }
        let active = PROBE.with_borrow(|p| p.as_ref().map(|p| (p.machine, p.observations)));
        if let Some((machine, observations)) = active {
            PROBE.with_borrow_mut(|p| {
                let p = p.as_mut().unwrap();
                assert!(p.literal_mode);
                if observations == 0 {
                    p.literal_results = values[..3].to_vec();
                } else {
                    assert_eq!(values[..3], p.literal_results);
                    assert_eq!(count, 7);
                    assert_eq!(values[6], p.supplied);
                    assert!(
                        !p.literal_results.contains(&values[6]),
                        "supplied equal-valued result remains distinct"
                    );
                    assert_eq!(r.integer_value(values[6]), 42);
                }
            });
            let cap = r.stackless_machine_capability(machine).unwrap();
            let aliases = || unsafe {
                let arena = (*r.state.get())
                    .execution
                    .machine(current_thread_token(), cap)
                    .unwrap()
                    .arena();
                let frame = arena.current_handle().unwrap();
                values[..3]
                    .iter()
                    .map(|value| {
                        (0..arena.descriptor(frame).unwrap().slot_count())
                            .filter(|&slot| arena.read_oop(frame, slot).unwrap() == *value)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            let before = aliases();
            assert!(before.iter().all(|slots| !slots.is_empty()));
            assert!(
                before[0].len() >= 2,
                "first result and its source alias stay live"
            );
            r.gc_collect_for_source_probe();
            assert_eq!(aliases(), before);
            for &value in &values[..3] {
                assert!(unsafe { (*r.state.get()).heap.contains(value) });
                assert_eq!(r.integer_value(value), 42);
            }
            PROBE.with_borrow_mut(|p| p.as_mut().unwrap().literal_aliases.push(before));
        }
    });
    // The closure pair (and optional supplied result) is contiguous in the
    // original call window; no native-stack scratch roots are introduced.
    unsafe { observe(probe, args.add(4), count - 4) }
}

unsafe extern "C" fn pause_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_source_probe_native_roots(|r| {
        let (expected, capture) = PROBE.with_borrow(|p| {
            let p = p.as_ref().unwrap();
            (p.machine, p.capture)
        });
        assert_eq!(machine, expected);
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(
            objects.machine_vtable,
            objects.call_site,
            crate::execution::machine_call_site,
        );
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
        if capture {
            r.stackless_capture_current(machine, result, pc).unwrap()
        } else {
            r.stackless_suspend_current(machine, result, pc).unwrap()
        }
    })
}

unsafe extern "C" fn run(probe: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { run_control(probe, args, count, false) }
}
unsafe extern "C" fn run_literals(probe: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { run_control(probe, args, count, true) }
}
unsafe fn run_control(probe: Oop, args: *const Oop, count: usize, literal_mode: bool) -> Oop {
    assert_eq!(count, 3);
    let (block, input, mode) = unsafe { (*args, *args.add(1), *args.add(2)) };
    with_source_probe_native_roots(|r| {
        assert!(PROBE.with_borrow(Option::is_none));
        let capture = mode == r.canonical().true_object;
        let baseline = r.stackless_registration_diagnostics();
        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
        PROBE.replace(Some(Probe {
            machine,
            input,
            supplied: input,
            literal_mode,
            capture,
            ..Probe::default()
        }));
        let StacklessTarget::Registered { entry } = r
            .resolve_stackless_target(block, r.intern(b"value:value:").unwrap())
            .unwrap()
        else {
            panic!("fixture must use an actual registered source Block");
        };
        let target = r
            .materialize_registered_target(entry, block, &[input, probe])
            .unwrap();
        let calls_before = super::native_tier::closure_elision::completed_calls();
        let counted_before = super::native_tier::counted_indices_recorded();
        let paused = r
            .drive_stackless_with_resolver(
                machine,
                target.descriptor,
                &target.initial_oops,
                Some(entry),
                |runtime, receiver, selector, _| {
                    runtime.resolve_stackless_target(receiver, selector)
                },
            )
            .unwrap();
        let counted_at_pause =
            super::native_tier::counted_indices_recorded().wrapping_sub(counted_before);
        let expected_calls = if std::env::var_os("BIRNAM_DEV_CLOSURE_ELISION")
            .is_some_and(|v| v == "1")
            && !std::env::var_os("BIRNAM_DEV_CLOSURE_FORCE_RECOVERY").is_some_and(|v| v == "1")
            && (!literal_mode
                || std::env::var_os("BIRNAM_DEV_LITERAL_CALLEES").is_some_and(|v| v == "1"))
        {
            std::env::var("BIRNAM_DEV_CLOSURE_FORCE_AT")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(3)
                .min(3)
        } else {
            0
        };
        assert_eq!(
            super::native_tier::closure_elision::completed_calls() - calls_before,
            expected_calls
        );
        let _paused = ScopedOopRootGuard::new(r, paused);
        assert_eq!(PROBE.with_borrow(|p| p.as_ref().unwrap().observations), 1);
        let pins = r.stackless_registration_diagnostics();
        assert_eq!(
            pins.active_frame_pins,
            baseline.active_frame_pins + usize::from(!capture)
        );
        assert_eq!(
            pins.continuation_frame_pins,
            baseline.continuation_frame_pins + usize::from(capture)
        );
        let codegen = r.import(b"Codegen").unwrap();
        r.send_slice(codegen, r.intern(b"evictMaterializations").unwrap(), &[]);
        r.gc_collect_for_source_probe();
        let identity = PROBE.with_borrow(|p| p.as_ref().unwrap().closure);
        assert!(unsafe { (*r.state.get()).heap.contains(identity) });
        let supplied = if literal_mode {
            PROBE.with_borrow(|p| {
                let p = p.as_ref().unwrap();
                assert_eq!(p.literal_results.len(), 3);
                for &value in &p.literal_results {
                    assert!(unsafe { (*r.state.get()).heap.contains(value) });
                    assert_eq!(r.integer_value(value), 42);
                }
            });
            r.make_integer(42)
        } else {
            input
        };
        let _supplied = ScopedOopRootGuard::new(r, supplied);
        PROBE.with_borrow_mut(|p| p.as_mut().unwrap().supplied = supplied);
        let answer = if capture {
            let wrong = r.create_stackless_machine().unwrap();
            let _wrong = ScopedOopRootGuard::new(r, wrong);
            assert_eq!(
                r.restore_stackless_continuation(wrong, paused, supplied),
                Err(ExecutionError::ContinuationWrongMachine)
            );
            let cap = r.stackless_machine_capability(wrong).unwrap();
            unsafe {
                (*r.state.get())
                    .execution
                    .machine_mut(current_thread_token(), cap)
                    .unwrap()
                    .fail(current_thread_token())
                    .unwrap();
            }
            r.destroy_stackless_machine(wrong).unwrap();
            let answer = r
                .restore_stackless_continuation(machine, paused, supplied)
                .unwrap();
            assert_eq!(
                r.restore_stackless_continuation(machine, paused, supplied),
                Err(ExecutionError::ContinuationConsumed)
            );
            answer
        } else {
            assert_eq!(paused, machine);
            let answer = r.resume_stackless_machine(machine, supplied).unwrap();
            assert_eq!(
                r.resume_stackless_machine(machine, supplied),
                Err(ExecutionError::MachineNotSuspended)
            );
            answer
        };
        let _answer = ScopedOopRootGuard::new(r, answer);
        let counted_after_resume =
            super::native_tier::counted_indices_recorded().wrapping_sub(counted_before);
        assert_eq!(
            answer,
            if literal_mode {
                PROBE.with_borrow(|p| p.as_ref().unwrap().literal_results[0])
            } else {
                identity
            }
        );
        let completed = r.stackless_registration_diagnostics();
        assert_eq!(completed.active_frame_pins, baseline.active_frame_pins);
        assert_eq!(
            completed.continuation_frame_pins,
            baseline.continuation_frame_pins
        );
        let state = PROBE.take().unwrap();
        assert_eq!(
            state.observations, 2,
            "restore cannot replay or skip the observers"
        );
        let _closure = ScopedOopRootGuard::new(r, identity);
        let returned = r.send_slice(identity, r.intern(b"value").unwrap(), &[]);
        if literal_mode {
            assert_eq!(state.literal_aliases.len(), 2);
            assert_eq!(r.integer_value(returned), 42);
            assert!(!state.literal_results.contains(&returned));
            assert_ne!(returned, supplied);
            println!(
                "closure-literal-control: calls={expected_calls} fresh_results=3 supplied_distinct=true aliases={:?}",
                state.literal_aliases
            );
        } else {
            assert_eq!(returned, input);
        }
        r.destroy_stackless_machine(machine).unwrap();
        if std::env::var_os("BIRNAM_TEST_COUNTED_CLOSURE_CONTROL").is_some_and(|v| v == "1") {
            println!(
                "closure-counted-control: before_pause={counted_at_pause} after_resume={counted_after_resume}"
            );
        }
        println!(
            "closure-control: capture={capture} observations=2 aliases={:?} pins_restored=true",
            state.aliases
        );
        answer
    })
}
