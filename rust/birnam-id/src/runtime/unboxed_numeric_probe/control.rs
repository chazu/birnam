//! Forced-recovery observation after all native optimizer scratch has unwound.
//! The test bridge publishes ordinary V2 control actions; production has no hook.
use super::*;
use std::cell::RefCell;

struct Control {
    machine: Oop,
    pc: usize,
    capture: bool,
    observed: bool,
    // Host Vec of raw addresses is deliberately not a root provider.
    integers: Vec<(usize, usize, i64)>,
}
thread_local! { static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) }; }

pub(in crate::runtime) fn active(machine: Oop) -> bool {
    CONTROL.with_borrow(|c| c.as_ref().is_some_and(|c| c.machine == machine))
}

fn integer_slots(r: &Runtime, machine: Oop) -> Vec<(usize, usize, i64)> {
    let owner = current_thread_token();
    let cap = r.stackless_machine_capability(machine).unwrap();
    let state = unsafe { &*r.state.get() };
    let arena = state.execution.machine(owner, cap).unwrap().arena();
    let frame = arena.current_handle().unwrap();
    (0..arena.descriptor(frame).unwrap().slot_count())
        .filter_map(|slot| {
            let value = arena.read_oop(frame, slot).unwrap();
            (Vtables::checked_vtable_of(&state.heap, value) == r.canonical().smallint_vt)
                .then(|| (slot, value.addr(), r.integer_value(value)))
        })
        .collect()
}

pub(in crate::runtime) fn after_action(r: &Runtime, machine: Oop, action: Oop) -> Oop {
    let Some((pc, capture)) = CONTROL.with_borrow(|c| {
        c.as_ref()
            .filter(|c| !c.observed && c.machine == machine)
            .map(|c| (c.pc, c.capture))
    }) else {
        return action;
    };
    let owner = current_thread_token();
    let cap = r.stackless_machine_capability(machine).unwrap();
    let (result_slot, next_pc) = {
        let state = unsafe { &*r.state.get() };
        let arena = state.execution.machine(owner, cap).unwrap().arena();
        if arena
            .root_state_program_counter(arena.current_handle().unwrap())
            .unwrap()
            != pc
        {
            return action;
        }
        let DecodedAction::Call { result_slot, .. } = state
            .execution
            .decode_action(&state.heap, owner, cap, action)
            .unwrap()
        else {
            panic!("control fixture requires an original pending Call");
        };
        (
            result_slot,
            arena
                .program_counter(arena.current_handle().unwrap())
                .unwrap(),
        )
    };
    let before = integer_slots(r, machine);
    assert!(!before.is_empty());
    // Only the published Frame and explicit caller roots retain these Oops.
    r.gc_collect_for_source_probe();
    assert_eq!(integer_slots(r, machine), before);
    CONTROL.with_borrow_mut(|c| {
        let c = c.as_mut().unwrap();
        c.observed = true;
        c.integers = before;
    });
    if capture {
        r.stackless_capture_current(machine, result_slot, next_pc)
            .unwrap()
    } else {
        r.stackless_suspend_current(machine, result_slot, next_pc)
            .unwrap()
    }
}

pub(super) unsafe extern "C" fn run(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 6);
    let args = unsafe { core::slice::from_raw_parts(args, count) };
    with_source_probe_native_roots(|r| {
        let (block, input, other) = (args[0], args[1], args[2]);
        let expected = r.integer_value(args[3]);
        let pc = r.integer_value(args[4]) as usize;
        let capture = args[5] == r.canonical().true_object;
        let _block = ScopedOopRootGuard::new(r, block);
        let _input = ScopedOopRootGuard::new(r, input);
        let _other = ScopedOopRootGuard::new(r, other);
        let selector = r.intern(b"value:value:").unwrap();
        assert!(CONTROL.with_borrow(Option::is_none));
        for _ in 0..16 {
            for _ in 0..8 {
                let answer = r.send_slice(block, selector, &[input, other]);
                assert_eq!(r.integer_value(answer), expected);
            }
            r.gc_collect_for_source_probe();
        }
        let baseline = r.stackless_registration_diagnostics();
        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
        let StacklessTarget::Registered { entry } =
            r.resolve_stackless_target(block, selector).unwrap()
        else {
            panic!("control fixture must use installed registered source");
        };
        let target = r
            .materialize_registered_target(entry, block, &[input, other])
            .unwrap();
        CONTROL.replace(Some(Control {
            machine,
            pc,
            capture,
            observed: false,
            integers: Vec::new(),
        }));
        let completed_before = COMPLETED.get();
        let materialized_before = MATERIALIZED.get();
        let paused = r
            .drive_stackless_with_resolver(
                machine,
                target.descriptor,
                &target.initial_oops,
                Some(entry),
                |r, receiver, selector, _| r.resolve_stackless_target(receiver, selector),
            )
            .unwrap();
        let _paused = ScopedOopRootGuard::new(r, paused);
        assert!(CONTROL.with_borrow(|c| c.as_ref().unwrap().observed));
        let completed = COMPLETED.get() - completed_before;
        let materialized = MATERIALIZED.get() - materialized_before;
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
        CONTROL.with_borrow(|c| {
            let heap = unsafe { &(*r.state.get()).heap };
            for &(_, address, value) in &c.as_ref().unwrap().integers {
                let oop = core::ptr::with_exposed_provenance_mut(address);
                assert!(
                    heap.contains(oop),
                    "published numeric identity must survive pause"
                );
                assert_eq!(r.integer_value(oop), value);
            }
        });
        let supplied = r.make_integer(733);
        let _supplied = ScopedOopRootGuard::new(r, supplied);
        let answer = if capture {
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
        let answer_value = r.integer_value(answer);
        let pins = r.stackless_registration_diagnostics();
        assert_eq!(pins.active_frame_pins, baseline.active_frame_pins);
        assert_eq!(
            pins.continuation_frame_pins,
            baseline.continuation_frame_pins
        );
        let control = CONTROL.take().unwrap();
        let normalized = control
            .integers
            .iter()
            .map(|&(slot, address, value)| {
                let first = control
                    .integers
                    .iter()
                    .position(|&(_, a, _)| a == address)
                    .unwrap();
                (slot, first, value)
            })
            .collect::<Vec<_>>();
        r.destroy_stackless_machine(machine).unwrap();
        println!(
            "unboxed-control pc={pc} capture={} completed={completed} materialized={materialized} answer={answer_value} supplied_identity={} pins_restored=1 aliases={normalized:?}",
            usize::from(capture),
            usize::from(answer == supplied)
        );
        r.canonical().nil
    })
}
