//! Diagnostic-floor comparison immediately after actual numeric callee returns.
//! The metadata holds scalars only and introduces no Id roots or callbacks.
use super::*;
use core::cell::RefCell;

struct Probe {
    machine: MachineCapability,
    descriptor: usize,
    baseline: crate::execution::RegistrationDiagnostics,
    events: Vec<Vec<Vec<usize>>>,
}
thread_local! {
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

pub(super) fn after_return(runtime: &Runtime, owner: u64, capability: MachineCapability) {
    PROBE.with_borrow_mut(|active| {
        let Some(probe) = active.as_mut().filter(|p| p.machine == capability) else {
            return;
        };
        // Read-only owner-thread borrow; no Id allocation or generated call.
        let state = unsafe { &*runtime.state.get() };
        let arena = state.execution.machine(owner, capability).unwrap().arena();
        let Some(frame) = arena.current_handle() else {
            return;
        };
        if arena.descriptor(frame).unwrap() as *const FrameDescriptor as usize != probe.descriptor {
            return;
        }
        let pins = state.execution.registration_diagnostics();
        assert_eq!(pins.active_frame_pins, probe.baseline.active_frame_pins + 1);
        assert_eq!(
            pins.continuation_frame_pins,
            probe.baseline.continuation_frame_pins
        );
        let mut event = state
            .execution
            .snapshot_action_words(&state.heap, owner, capability);
        event.push(vec![
            arena.program_counter(frame).unwrap(),
            arena.descriptor(frame).unwrap().slot_count(),
            pins.active_frame_pins - probe.baseline.active_frame_pins,
            pins.continuation_frame_pins - probe.baseline.continuation_frame_pins,
        ]);
        // Acquired entry leases differ during the optimized prefix because all
        // selected calls are admitted together. Final balance is checked below.
        probe.events.push(event);
    });
}

pub(super) unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    let (factory, input) = unsafe { (*args, *args.add(1)) };
    with_source_probe_native_roots(|runtime| {
        assert!(PROBE.with_borrow(Option::is_none));
        let selector = runtime.intern(b"value:").unwrap();
        for _ in 0..128 {
            let answer = runtime.send_slice(factory, selector, &[input]);
            assert_eq!(runtime.integer_value(answer), 42);
        }
        runtime.gc_collect_for_source_probe();
        let machine = runtime.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(runtime, machine);
        let capability = runtime.stackless_machine_capability(machine).unwrap();
        let baseline = runtime.stackless_registration_diagnostics();
        let StacklessTarget::Registered { entry } =
            runtime.resolve_stackless_target(factory, selector).unwrap()
        else {
            panic!("action fixture must use a real registered source factory");
        };
        let target = runtime
            .materialize_registered_target(entry, factory, &[input])
            .unwrap();
        PROBE.replace(Some(Probe {
            machine: capability,
            descriptor: std::sync::Arc::as_ptr(&target.descriptor) as usize,
            baseline,
            events: Vec::new(),
        }));
        let calls_before = super::native_tier::closure_elision::completed_calls();
        let answer = runtime.drive_stackless_with_resolver(
            machine,
            target.descriptor,
            &target.initial_oops,
            Some(entry),
            |r, receiver, selector, _| r.resolve_stackless_target(receiver, selector),
        );
        let probe = PROBE.replace(None).unwrap();
        let answer = answer.unwrap();
        let _answer = ScopedOopRootGuard::new(runtime, answer);
        assert_eq!(runtime.integer_value(answer), 42);
        assert_eq!(
            probe.events.len(),
            3,
            "each callee must finish a physical Frame return"
        );
        let pins = runtime.stackless_registration_diagnostics();
        assert_eq!(pins.active_frame_pins, baseline.active_frame_pins);
        assert_eq!(
            pins.continuation_frame_pins,
            baseline.continuation_frame_pins
        );
        assert_eq!(pins.acquired_entries, baseline.acquired_entries);
        runtime.gc_collect_for_source_probe();
        assert_eq!(runtime.integer_value(answer), 42);
        println!("closure-actions: {:?}", probe.events);
        println!(
            "closure-actions-completed: {}",
            super::native_tier::closure_elision::completed_calls() - calls_before
        );
        answer
    })
}
