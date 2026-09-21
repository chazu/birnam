//! Design 74F. A bounded, test-only tier switch in the real Runtime driver.
//! No native exports, persisted metadata, or dynamic-code ownership are added.

use super::*;
use crate::execution::{
    DescriptorIdentity, EdgeTransitionKind, FrameDescriptorV2Spec, FrameEdgeSpec, FrameViewError,
};
use core::cell::RefCell;
use std::sync::Arc;

/// Only the statically linked fixture below can be selected. The descriptor
/// stays alive by Arc; executable lifetime is the test binary's lifetime.
pub(super) struct Trial {
    descriptor: Arc<FrameDescriptor>,
    epoch: u64,
    pcs: [bool; 2],
    disabled: bool,
    optimized_entries: usize,
    deoptimizations: usize,
    fallback_entries: usize,
}

struct TrialScope<'a>(&'a Runtime);

impl<'a> TrialScope<'a> {
    fn install(runtime: &'a Runtime, descriptor: Arc<FrameDescriptor>) -> Self {
        assert_eq!(descriptor.program_counter_count(), 2);
        assert_eq!(descriptor.driver_argument_count(), 1);
        assert_eq!(
            MethodAddress::from_method(descriptor.step_method()),
            MethodAddress::from_method(baseline_step)
        );
        assert!(runtime.deopt_trial.borrow().is_none());
        // SAFETY: owner-thread read, no foreign code in this scope.
        let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
        *runtime.deopt_trial.borrow_mut() = Some(Trial {
            descriptor,
            epoch,
            pcs: [true, true],
            disabled: false,
            optimized_entries: 0,
            deoptimizations: 0,
            fallback_entries: 0,
        });
        Self(runtime)
    }
}

impl Drop for TrialScope<'_> {
    fn drop(&mut self) {
        self.0.deopt_trial.borrow_mut().take();
    }
}

pub(super) fn select(
    runtime: &Runtime,
    capability: MachineCapability,
    owner: u64,
    machine: Oop,
    baseline: Method,
) -> Result<Method, ExecutionError> {
    let mut trial = runtime.deopt_trial.borrow_mut();
    let Some(trial) = trial.as_mut() else {
        return Ok(baseline);
    };
    // SAFETY: a short read after the driver committed the previous action;
    // nothing here allocates Birnam objects or invokes a Method.
    let state = unsafe { &*runtime.state.get() };
    let arena = state.execution.machine(owner, capability)?.arena();
    let frame = arena
        .current_handle()
        .ok_or(crate::execution::FrameError::StaleHandle)?;
    let descriptor = arena.descriptor(frame)?;
    // Numeric identities alone must never authorize replacement code.
    if !core::ptr::eq(descriptor, Arc::as_ptr(&trial.descriptor)) {
        return Ok(baseline);
    }
    let pc = arena.program_counter(frame)?;
    let epoch = state.vtables.mutation_epoch();
    let eligible = !trial.disabled
        && epoch != u64::MAX
        && epoch == trial.epoch
        && trial.pcs.get(pc).copied().unwrap_or(false)
        && runtime.guarded_slot_access_eligible(machine);
    if eligible {
        trial.optimized_entries += 1;
        Ok(optimized_step)
    } else {
        if !trial.disabled && trial.optimized_entries != 0 {
            trial.deoptimizations += 1;
        }
        trial.disabled = true;
        trial.fallback_entries += 1;
        // No PC, slot, descriptor, pending action, or registration pin changes.
        Ok(baseline)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Canonical,
    BeforeEntry,
    RestoreAfterMutation,
    CustomLookup,
    MachineOverride,
    ReplaceActive,
    PreciseGc,
    NestedEntry,
    MissingRecoveryPc,
    DescriptorCollision,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::BeforeEntry => "before-entry",
            Self::RestoreAfterMutation => "mutation-restored",
            Self::CustomLookup => "custom-lookup",
            Self::MachineOverride => "machine-override",
            Self::ReplaceActive => "replace-active",
            Self::PreciseGc => "precise-gc",
            Self::NestedEntry => "nested-entry",
            Self::MissingRecoveryPc => "missing-recovery-pc",
            Self::DescriptorCollision => "descriptor-collision",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Visit {
    frame: [usize; 3],
    identity: DescriptorIdentity,
    pc: usize,
    optimized: bool,
}

struct Probe {
    case: Case,
    effects: Vec<&'static str>,
    visits: Vec<Visit>,
    reads: usize,
    override_reads: usize,
    lookup_calls: usize,
    view: Oop,
    view_holder: Oop,
    carried: Oop,
    next_answer: Option<i64>,
    nested_answer: Option<i64>,
}

impl Probe {
    fn new(case: Case) -> Self {
        Self {
            case,
            effects: Vec::new(),
            visits: Vec::new(),
            reads: 0,
            override_reads: 0,
            lookup_calls: 0,
            view: core::ptr::null_mut(),
            view_holder: core::ptr::null_mut(),
            carried: core::ptr::null_mut(),
            next_answer: None,
            nested_answer: None,
        }
    }
}

thread_local! {
    // Observation only: raw Oops here are deliberately NOT GC roots.
    static PROBE: RefCell<Probe> = RefCell::new(Probe::new(Case::Canonical));
}

fn selector(runtime: &Runtime, name: &[u8]) -> Oop {
    runtime.intern(name).unwrap()
}

fn send_index(runtime: &Runtime, machine: Oop, name: &[u8], index: i64) -> Oop {
    let selector = selector(runtime, name);
    let operand = runtime.make_integer(index);
    let _root = ScopedOopRootGuard::new(runtime, operand);
    runtime.send_slice(machine, selector, &[operand])
}

fn store(runtime: &Runtime, machine: Oop, index: i64, value: Oop) {
    // The fixture is native code: root a produced value before interning or
    // boxing. This is not a new generated-code rooting convention.
    let _root = ScopedOopRootGuard::new(runtime, value);
    let selector = selector(runtime, b"oopAt:put:");
    let operand = runtime.make_integer(index);
    let _operand_root = ScopedOopRootGuard::new(runtime, operand);
    assert_eq!(
        runtime.send_slice(machine, selector, &[operand, value]),
        value
    );
}

fn double_read(runtime: &Runtime, machine: Oop, slot: i64, optimized: bool) -> i64 {
    let selector = selector(runtime, b"oopAt:");
    let operand = runtime.make_integer(slot);
    let _root = ScopedOopRootGuard::new(runtime, operand);
    // Allocation above could invalidate entry assumptions. Recheck before the
    // region, then leave no allocation or callback between its two reads.
    // A miss here executes baseline reads; tier recovery follows at the next
    // driver boundary. This is an operation guard, not mid-entry deoptimization.
    let eliminate = optimized && runtime.guarded_slot_access_eligible(machine);
    let left = runtime.send_slice(machine, selector, &[operand]);
    let right = if eliminate {
        left
    } else {
        runtime.send_slice(machine, selector, &[operand])
    };
    PROBE.with_borrow_mut(|probe| probe.reads += if eliminate { 1 } else { 2 });
    runtime.integer_value(left) + runtime.integer_value(right)
}

unsafe extern "C" fn baseline_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    // SAFETY: same Method ABI as the descriptor selected by the real driver.
    unsafe { step(machine, args, count, false) }
}

unsafe extern "C" fn optimized_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    // SAFETY: same Method ABI and canonical Frame layout as baseline_step.
    unsafe { step(machine, args, count, true) }
}

unsafe fn step(machine: Oop, args: *const Oop, count: usize, optimized: bool) -> Oop {
    with_active(|runtime| {
        assert_eq!(count, 1);
        assert!(!args.is_null());
        // SAFETY: the driver supplies the one resume-token Oop.
        let pc = runtime.integer_value(unsafe { args.read() }) as usize;
        let capability = runtime.stackless_machine_capability(machine).unwrap();
        let visit = unsafe {
            let state = &*runtime.state.get();
            let arena = state
                .execution
                .machine(current_thread_token(), capability)
                .unwrap()
                .arena();
            let frame = arena.current_handle().unwrap();
            assert_eq!(arena.program_counter(frame).unwrap(), pc);
            Visit {
                frame: frame.words().unwrap(),
                identity: arena.descriptor(frame).unwrap().identity(),
                pc,
                optimized,
            }
        };
        PROBE.with_borrow_mut(|probe| probe.visits.push(visit));
        match pc {
            0 => {
                PROBE.with_borrow_mut(|probe| probe.effects.push("body-before"));
                let carried = runtime.make_integer(double_read(runtime, machine, 1, optimized));
                let _carried_root = ScopedOopRootGuard::new(runtime, carried);
                store(runtime, machine, 2, carried);
                store(runtime, machine, 3, carried);
                PROBE.with_borrow_mut(|probe| probe.carried = carried);
                // Drop the temporary root before the callback: the Frame must
                // carry the value and its alias through precise collection.
                drop(_carried_root);
                let effect = selector(runtime, b"deoptEffect:");
                let receiver = runtime.canonical().object_prototype;
                runtime.send_slice(receiver, effect, &[machine]);
                send_index(runtime, machine, b"continueAt:", 1)
            }
            1 => {
                PROBE.with_borrow_mut(|probe| probe.effects.push("body-after"));
                let carried = PROBE.with_borrow(|probe| probe.carried);
                assert_eq!(runtime.stackless_oop_at(machine, 2).unwrap(), carried);
                assert_eq!(runtime.stackless_oop_at(machine, 3).unwrap(), carried);
                assert!(runtime.is_integer(carried));
                let view = PROBE.with_borrow(|probe| probe.view);
                assert_eq!(
                    runtime
                        .integer_value(runtime.stackless_frame_view_index_word(view, 0).unwrap()),
                    1
                );
                assert_eq!(runtime.stackless_frame_view_oop(view, 2).unwrap(), carried);
                let result = runtime.make_integer(double_read(runtime, machine, 2, optimized));
                store(runtime, machine, 4, result);
                send_index(runtime, machine, b"returnAt:", 4)
            }
            _ => panic!("unexpected PC {pc}"),
        }
    })
}

unsafe extern "C" fn original_thunk(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    // Distinct native identities are required by registration. Optimized Rust
    // can otherwise fold these structurally identical self-referring thunks.
    std::hint::black_box(74061u64);
    with_active(|runtime| {
        // SAFETY: forwarding the original Method buffer contract.
        unsafe {
            runtime.enter_registered_resumable_or_fatal(original_thunk, receiver, args, count)
        }
    })
}

unsafe extern "C" fn replacement_thunk(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    std::hint::black_box(74062u64);
    with_active(|runtime| {
        // SAFETY: forwarding the original Method buffer contract.
        unsafe {
            runtime.enter_registered_resumable_or_fatal(replacement_thunk, receiver, args, count)
        }
    })
}

unsafe extern "C" fn replacement_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| {
        store(runtime, machine, 4, runtime.make_integer(999));
        send_index(runtime, machine, b"returnAt:", 4)
    })
}

unsafe extern "C" fn nested_thunk(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    std::hint::black_box(74063u64);
    with_active(|runtime| {
        // SAFETY: forwarding the original Method buffer contract.
        unsafe { runtime.enter_registered_resumable_or_fatal(nested_thunk, receiver, args, count) }
    })
}

unsafe extern "C" fn nested_step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| {
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .active_frame_pins,
            2
        );
        PROBE.with_borrow_mut(|probe| probe.effects.push("nested-entry"));
        send_index(runtime, machine, b"returnAt:", 1)
    })
}

unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    PROBE.with_borrow_mut(|probe| probe.lookup_calls += 1);
    // SAFETY: count/buffer forwarded to the unchanged native lookup primitive.
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

unsafe extern "C" fn read_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    PROBE.with_borrow_mut(|probe| probe.override_reads += 1);
    // SAFETY: a native override avoids metacircular Birnam recursion and
    // forwards the exact buffer to the ordinary primitive once.
    unsafe { crate::execution::machine_oop_at(receiver, args, count) }
}

fn bump_epoch(runtime: &Runtime) {
    let vt = runtime.globals().object_vt;
    let name = selector(runtime, b"deoptEpochProbe:");
    let _ = runtime.add_method(vt, name, nested_thunk);
    runtime.remove_method(vt, name).unwrap();
}

unsafe extern "C" fn effect_callback(_: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| {
        assert_eq!(count, 1);
        // SAFETY: effect: is sent one Machine argument.
        let machine = unsafe { args.read() };
        PROBE.with_borrow_mut(|probe| probe.effects.push("effect"));
        let view = runtime.stackless_current_frame_view(machine).unwrap();
        // The retained view is independently rooted; its lifetime is observed
        // after return, when its generational Frame reference must be stale.
        let holder = PROBE.with_borrow(|probe| probe.view_holder);
        // SAFETY: a rooted, one-Oop holder was allocated before freezing the
        // epoch; retaining the view does not itself mutate a dictionary.
        assert!(unsafe {
            (&mut *runtime.state.get())
                .heap
                .write_payload_word(holder, 0, view.addr())
        });
        PROBE.with_borrow_mut(|probe| probe.view = view);
        let case = PROBE.with_borrow(|probe| probe.case);
        match case {
            Case::RestoreAfterMutation | Case::PreciseGc => bump_epoch(runtime),
            Case::CustomLookup => {
                let globals = runtime.globals();
                let _ = runtime.add_method(globals.vtable_vt, globals.lookup, lookup_override);
            }
            Case::MachineOverride => {
                let objects = unsafe { (*runtime.state.get()).execution.objects() }.unwrap();
                let _ = runtime.add_method(objects.machine_vtable, objects.oop_at, read_override);
            }
            Case::ReplaceActive => {
                let name = selector(runtime, b"deoptRun:");
                let _ = runtime.add_method(runtime.globals().object_vt, name, replacement_thunk);
                assert_eq!(
                    runtime
                        .stackless_registration_diagnostics()
                        .active_frame_pins,
                    1
                );
            }
            Case::NestedEntry => {
                let nested = selector(runtime, b"deoptNested:");
                let argument = runtime.stackless_oop_at(machine, 2).unwrap();
                let result =
                    runtime.send_slice(runtime.canonical().object_prototype, nested, &[argument]);
                assert_eq!(result, argument);
                PROBE.with_borrow_mut(|probe| {
                    probe.effects.push("nested-return");
                    probe.nested_answer = Some(runtime.integer_value(result));
                });
                bump_epoch(runtime);
            }
            _ => {}
        }
        if case == Case::PreciseGc || case == Case::ReplaceActive {
            runtime.gc_set_precise(true);
            let garbage = runtime.make_integer(123456);
            // No native range: only actual Runtime roots protect the carried
            // object. The dead control allocation must disappear.
            let _ = runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(runtime.checked_vtable_of(garbage), core::ptr::null_mut());
            let carried = PROBE.with_borrow(|probe| probe.carried);
            assert_eq!(runtime.integer_value(carried), 14);
            assert_eq!(runtime.stackless_oop_at(machine, 3).unwrap(), carried);
        }
        runtime.canonical().nil
    })
}

fn descriptor(step_method: Method, generation: u64) -> Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 7406,
            generation,
            source_map: 7406,
        },
        program_counter_count: 2,
        driver_argument_count: 1,
        slot_count: 5,
        entry_receiver_slot: 0,
        entry_argument_start: 1,
        entry_argument_count: 1,
        entry_capture_slots: Vec::new(),
        implementation_slot: None,
        root_bitmap_words: vec![0b11111, 0b11111],
        call_sites: Vec::new(),
        edges: vec![FrameEdgeSpec {
            kind: EdgeTransitionKind::Continue,
            source_pc: 0,
            target_pc: 1,
            call_site_index: None,
            result_slot: None,
            copies: Vec::new(),
            clear_bitmap_words: vec![0],
        }],
        step_method,
    })
    .unwrap()
}

#[derive(Debug, Eq, PartialEq)]
struct Outcome {
    answer: i64,
    effects: Vec<&'static str>,
    override_reads: usize,
    lookup_calls: usize,
    next_answer: Option<i64>,
    nested_answer: Option<i64>,
}

fn run_case(case: Case, candidate: bool) -> (Outcome, usize, usize, usize) {
    run_case_repeated(case, candidate, 1)
}

fn run_case_repeated(
    case: Case,
    candidate: bool,
    repetitions: usize,
) -> (Outcome, usize, usize, usize) {
    let bootstrap = crate::dispatch::build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    PROBE.with_borrow_mut(|probe| *probe = Probe::new(case));
    let holder_vt = runtime.create_vtable(runtime.globals().object_vt);
    runtime.set_layout(holder_vt, [SLOT_OOP].as_ptr(), 1, crate::abi::SLOT_RAW);
    let holder = runtime.allocate_instance(holder_vt, size_of::<usize>());
    let _holder_root = ScopedOopRootGuard::new(runtime, holder);
    PROBE.with_borrow_mut(|probe| probe.view_holder = holder);
    let original = descriptor(baseline_step, 1);
    runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(
            original_thunk,
            original.clone(),
            1,
            0,
            1,
        ))
        .unwrap();
    runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(
            replacement_thunk,
            descriptor(replacement_step, 2),
            1,
            0,
            1,
        ))
        .unwrap();
    runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(
            nested_thunk,
            descriptor(nested_step, 3),
            1,
            0,
            1,
        ))
        .unwrap();
    let receiver = runtime.canonical().object_prototype;
    let run = selector(runtime, b"deoptRun:");
    for (name, method) in [
        (run, original_thunk as Method),
        (selector(runtime, b"deoptEffect:"), effect_callback),
        (selector(runtime, b"deoptNested:"), nested_thunk),
    ] {
        let _ = runtime.add_method(runtime.globals().object_vt, name, method);
    }
    // Pre-intern fixture selectors before freezing the conservative epoch.
    for name in [
        &b"oopAt:"[..],
        b"oopAt:put:",
        b"continueAt:",
        b"returnAt:",
        b"deoptEpochProbe:",
        b"DeoptExperimentView",
    ] {
        let _ = selector(runtime, name);
    }
    let retained_descriptors = Arc::strong_count(&original);
    let mut last = None;
    for _ in 0..repetitions {
        PROBE.with_borrow_mut(|probe| {
            *probe = Probe::new(case);
            probe.view_holder = holder;
        });
        let trial_descriptor = if case == Case::DescriptorCollision {
            descriptor(baseline_step, 1)
        } else {
            original.clone()
        };
        let scope = candidate.then(|| TrialScope::install(runtime, trial_descriptor));
        if case == Case::BeforeEntry {
            bump_epoch(runtime);
        }
        if candidate && case == Case::MissingRecoveryPc {
            runtime.deopt_trial.borrow_mut().as_mut().unwrap().pcs[1] = false;
        }
        let input = runtime.make_integer(7);
        let _input_root = ScopedOopRootGuard::new(runtime, input);
        PROBE.with_borrow_mut(|probe| probe.effects.push("argument"));
        let answer = runtime.enter(|| runtime.send_slice(receiver, run, &[input]));
        assert_eq!(runtime.integer_value(answer), 28);
        if case == Case::ReplaceActive {
            let next = runtime.enter(|| runtime.send_slice(receiver, run, &[input]));
            assert_eq!(runtime.integer_value(next), 999);
            PROBE.with_borrow_mut(|probe| probe.next_answer = Some(999));
        }
        let (entries, deopts, fallbacks) =
            runtime
                .deopt_trial
                .borrow()
                .as_ref()
                .map_or((0, 0, 0), |trial| {
                    (
                        trial.optimized_entries,
                        trial.deoptimizations,
                        trial.fallback_entries,
                    )
                });
        let reads = PROBE.with_borrow(|probe| probe.reads);
        let outcome = PROBE.with_borrow(|probe| {
            let expected_effects = if case == Case::NestedEntry {
                vec![
                    "argument",
                    "body-before",
                    "effect",
                    "nested-entry",
                    "nested-return",
                    "body-after",
                ]
            } else {
                vec!["argument", "body-before", "effect", "body-after"]
            };
            assert_eq!(
                probe.effects, expected_effects,
                "each effect happens once in order"
            );
            if case == Case::MachineOverride {
                assert_eq!(
                    probe.override_reads, 2,
                    "recovery honors both ordinary reads"
                );
            }
            if case == Case::CustomLookup {
                assert!(probe.lookup_calls > 0, "custom lookup actually executes");
            }
            assert_eq!(probe.visits.len(), 2);
            let first = &probe.visits[0];
            let last = &probe.visits[1];
            assert_eq!((first.pc, last.pc), (0, 1));
            assert_eq!(
                first.frame, last.frame,
                "same generational Frame survives recovery"
            );
            assert_eq!(first.identity, last.identity);
            assert_eq!(
                last.identity,
                original.identity(),
                "active body is not replaced"
            );
            assert_eq!(
                runtime.stackless_frame_view_oop(probe.view, 2),
                Err(FrameViewError::StaleFrameView)
            );
            assert_eq!(
                probe.visits.iter().filter(|visit| visit.optimized).count(),
                entries
            );
            Outcome {
                answer: 28,
                effects: probe.effects.clone(),
                override_reads: probe.override_reads,
                lookup_calls: probe.lookup_calls,
                next_answer: probe.next_answer,
                nested_answer: probe.nested_answer,
            }
        });
        let registrations = runtime.stackless_registration_diagnostics();
        assert_eq!(registrations.active_frame_pins, 0);
        assert_eq!(registrations.acquired_entries, 0);
        assert_eq!(registrations.continuation_frame_pins, 0);
        assert_eq!(runtime.stackless_runtime_diagnostics().live_frames, 0);
        if candidate {
            match case {
                Case::Canonical => {
                    assert_eq!((entries, deopts, fallbacks), (2, 0, 0));
                    assert_eq!(reads, 2);
                }
                Case::BeforeEntry => {
                    assert_eq!((entries, deopts, fallbacks), (0, 0, 2));
                    assert_eq!(reads, 4);
                }
                Case::DescriptorCollision => {
                    assert_eq!((entries, deopts, fallbacks), (0, 0, 0));
                    assert_eq!(reads, 4);
                }
                _ => {
                    assert_eq!((entries, deopts, fallbacks), (1, 1, 1));
                    assert_eq!(reads, 3);
                }
            }
        } else {
            assert_eq!(reads, 4);
        }
        drop(scope);
        assert!(runtime.deopt_trial.borrow().is_none());
        assert_eq!(Arc::strong_count(&original), retained_descriptors);
        last = Some((outcome, entries, deopts, reads));
    }
    last.unwrap()
}

#[test]
fn same_frame_deopt_experiment_matrix() {
    for case in [
        Case::Canonical,
        Case::BeforeEntry,
        Case::RestoreAfterMutation,
        Case::CustomLookup,
        Case::MachineOverride,
        Case::ReplaceActive,
        Case::PreciseGc,
        Case::NestedEntry,
        Case::MissingRecoveryPc,
        Case::DescriptorCollision,
    ] {
        let (baseline, _, _, _) = run_case(case, false);
        let (candidate, entries, deopts, reads) = run_case(case, true);
        assert_eq!(
            baseline,
            candidate,
            "{} baseline/candidate effect parity",
            case.name()
        );
        println!(
            "DEOPT_EXPERIMENT {{\"case\":\"{}\",\"answer\":{},\"effects_equal\":true,\"optimized_entries\":{},\"deoptimizations\":{},\"candidate_reads\":{},\"baseline_reads\":4,\"same_frame\":true,\"pins_after\":0}}",
            case.name(),
            candidate.answer,
            entries,
            deopts,
            reads
        );
    }
}

#[test]
fn same_frame_deopt_experiment_repeated_trials_release_state() {
    for case in [Case::Canonical, Case::RestoreAfterMutation] {
        let (outcome, _, _, _) = run_case_repeated(case, true, 128);
        assert_eq!(outcome.answer, 28);
    }
    println!(
        "DEOPT_EXPERIMENT {{\"case\":\"repeated-trials\",\"trials\":256,\"pins_after\":0,\"trial_records_after\":0,\"descriptor_references_balanced\":true}}"
    );
}
