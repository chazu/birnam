//! Test-floor bridge for real source-created closures and host-owned Machines.
//! Absent from production builds; uses the ordinary driver and recovery path.
use super::*;
use core::cell::RefCell;

#[derive(Default)]
struct Probe {
    machine: Oop,
    closure: Oop,
    input: Oop,
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
        unsafe { super::closure_api_probe::attach(api) };
        let vt = self.create_vtable(self.globals().object_vt);
        for (name, method) in [
            (b"warm:input:".as_slice(), warm as Method),
            (b"run:input:capture:".as_slice(), run as Method),
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
        let object = self.allocate_instance(vt, 0);
        self.export(b"ClosureControlProbe", object).unwrap();
    }
}

unsafe extern "C" fn warm(probe: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    let (block, input) = unsafe { (*args, *args.add(1)) };
    with_active(|r| {
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
    with_active(|r| r.canonical().nil)
}

unsafe extern "C" fn observe(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert!((2..=3).contains(&count));
    let closure = unsafe { *args };
    assert_eq!(closure, unsafe { *args.add(1) });
    with_active(|r| {
        let Some((machine, expected, input, observations)) = PROBE.with_borrow(|p| {
            p.as_ref()
                .map(|p| (p.machine, p.closure, p.input, p.observations))
        }) else {
            return closure;
        };
        if observations != 0 {
            assert_eq!(closure, expected, "resume must preserve recovered identity");
            assert_eq!(count, 3);
            assert_eq!(
                unsafe { *args.add(2) },
                input,
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
        r.gc_collect_from_native_range(0, 0);
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

unsafe extern "C" fn pause_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| {
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
    assert_eq!(count, 3);
    let (block, input, mode) = unsafe { (*args, *args.add(1), *args.add(2)) };
    with_active(|r| {
        assert!(PROBE.with_borrow(Option::is_none));
        let capture = mode == r.canonical().true_object;
        let baseline = r.stackless_registration_diagnostics();
        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
        PROBE.replace(Some(Probe {
            machine,
            input,
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
        let expected_calls = if std::env::var_os("BIRNAM_DEV_CLOSURE_ELISION")
            .is_some_and(|v| v == "1")
            && !std::env::var_os("BIRNAM_DEV_CLOSURE_FORCE_RECOVERY").is_some_and(|v| v == "1")
        {
            3
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
        r.gc_collect_from_native_range(0, 0);
        let identity = PROBE.with_borrow(|p| p.as_ref().unwrap().closure);
        assert!(unsafe { (*r.state.get()).heap.contains(identity) });
        let answer = if capture {
            let wrong = r.create_stackless_machine().unwrap();
            let _wrong = ScopedOopRootGuard::new(r, wrong);
            assert_eq!(
                r.restore_stackless_continuation(wrong, paused, input),
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
                .restore_stackless_continuation(machine, paused, input)
                .unwrap();
            assert_eq!(
                r.restore_stackless_continuation(machine, paused, input),
                Err(ExecutionError::ContinuationConsumed)
            );
            answer
        } else {
            assert_eq!(paused, machine);
            let answer = r.resume_stackless_machine(machine, input).unwrap();
            assert_eq!(
                r.resume_stackless_machine(machine, input),
                Err(ExecutionError::MachineNotSuspended)
            );
            answer
        };
        let _answer = ScopedOopRootGuard::new(r, answer);
        assert_eq!(answer, identity);
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
        assert_eq!(
            r.send_slice(answer, r.intern(b"value").unwrap(), &[]),
            input
        );
        r.destroy_stackless_machine(machine).unwrap();
        println!(
            "closure-control: capture={capture} observations=2 aliases={:?} pins_restored=true",
            state.aliases
        );
        answer
    })
}
