use super::*;
use crate::dispatch::build_bootstrap;
use crate::execution::{DescriptorIdentity, FrameDescriptor, SemanticRegistrationV1};
use crate::execution::{FrameDescriptorV2Spec, probe_spec};
use crate::runtime::{ScopedOopRootGuard, current_thread_token, with_active};
use core::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Canonical,
    InvalidOperands,
    ReadOverride,
    WriteOverride,
    RemovedRead,
    PrivateReceiver,
    CustomLookup,
    Saturated,
    Collection,
}

thread_local! {
    static CASE: Cell<Case> = const { Cell::new(Case::Canonical) };
    static EFFECTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static VALUES: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
    static MACHINE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static MARK_VALUE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
}

struct Mode(bool);
impl Mode {
    fn set(enabled: bool) -> Self {
        HITS.set(0);
        Self(ENABLED.replace(enabled))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        ENABLED.set(self.0);
    }
}

unsafe extern "C" fn read_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    EFFECTS.with_borrow_mut(|effects| effects.push("read"));
    unsafe { crate::execution::machine_oop_at(receiver, args, count) }
}

unsafe extern "C" fn write_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    EFFECTS.with_borrow_mut(|effects| effects.push("write"));
    unsafe { crate::execution::machine_oop_at_put(receiver, args, count) }
}

unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    let selector = unsafe { args.read() };
    with_active(|runtime| {
        let objects = unsafe { (*runtime.state.get()).execution.objects().unwrap() };
        if selector == objects.oop_at {
            EFFECTS.with_borrow_mut(|effects| effects.push("lookup-read"));
        } else if selector == objects.oop_at_put {
            EFFECTS.with_borrow_mut(|effects| effects.push("lookup-write"));
        }
        unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
    })
}

unsafe extern "C" fn mutate_root(mark: crate::abi::MarkFn) {
    let value = MARK_VALUE.get();
    unsafe { mark(value) };
    with_active(|runtime| {
        runtime
            .stackless_oop_at_put(MACHINE.get(), 0, value)
            .unwrap();
    });
    EFFECTS.with_borrow_mut(|effects| effects.push("gc-write"));
}

fn frame_words(runtime: &Runtime, machine: Oop) -> [usize; 3] {
    let capability = runtime.stackless_machine_capability(machine).unwrap();
    unsafe {
        (*runtime.state.get())
            .execution
            .machine(current_thread_token(), capability)
            .unwrap()
            .arena()
            .current_handle()
            .unwrap()
            .words()
            .unwrap()
    }
}

unsafe extern "C" fn probe_step(machine: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_active(|runtime| {
        let frame = frame_words(runtime, machine);
        let objects = unsafe { (*runtime.state.get()).execution.objects().unwrap() };
        let globals = runtime.globals();
        let nil = runtime.canonical().nil;
        let index = runtime.make_integer(0);
        let _index_root = ScopedOopRootGuard::new(runtime, index);
        let value = runtime.stackless_oop_at(machine, 1).unwrap();
        let read = || {
            let result = runtime.send_slice(machine, objects.oop_at, &[index]);
            VALUES.with_borrow_mut(|values| values.push(runtime.integer_value(result)));
            result
        };
        assert_eq!(runtime.integer_value(read()), 11);
        let initial_hits = HITS.get();
        assert_eq!(initial_hits, usize::from(ENABLED.get()));

        match CASE.get() {
            Case::Canonical => {}
            Case::InvalidOperands => {
                let negative = runtime.make_integer(-1);
                let raw = runtime.make_integer(2);
                let beyond = runtime.make_integer(99);
                for operands in [
                    vec![],
                    vec![index, value],
                    vec![nil],
                    vec![negative],
                    vec![raw],
                    vec![beyond],
                ] {
                    assert_eq!(runtime.send_slice(machine, objects.oop_at, &operands), nil);
                }
                for operands in [
                    vec![],
                    vec![index],
                    vec![index, value, nil],
                    vec![nil, value],
                    vec![negative, value],
                    vec![raw, value],
                    vec![beyond, value],
                ] {
                    assert_eq!(
                        runtime.send_slice(machine, objects.oop_at_put, &operands),
                        nil
                    );
                }
            }
            Case::ReadOverride | Case::Saturated => {
                if CASE.get() == Case::Saturated {
                    unsafe {
                        (*runtime.state.get())
                            .vtables
                            .exhaust_mutation_epoch_for_test()
                    };
                    // Cache a positive verdict at MAX before changing the method.
                    assert!(runtime.guarded_slot_access_eligible(machine));
                }
                runtime.add_method(objects.machine_vtable, objects.oop_at, read_override);
            }
            Case::WriteOverride => {
                runtime.add_method(objects.machine_vtable, objects.oop_at_put, write_override);
            }
            Case::RemovedRead => {
                runtime.add_method(globals.object_vt, objects.oop_at, read_override);
                runtime
                    .remove_method(objects.machine_vtable, objects.oop_at)
                    .unwrap();
            }
            Case::PrivateReceiver => {
                runtime.ensure_private_vtable(machine);
            }
            Case::CustomLookup => {
                runtime.add_method(globals.vtable_vt, globals.lookup, lookup_override);
            }
            Case::Collection => {
                MACHINE.set(machine);
                MARK_VALUE.set(value);
                runtime.gc_register_root_provider(mutate_root);
                let before_epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
                runtime.gc_collect_from_native_range(0, 0);
                assert_eq!(
                    unsafe { (*runtime.state.get()).vtables.mutation_epoch() },
                    before_epoch
                );
            }
        }
        let expected = if CASE.get() == Case::Collection {
            22
        } else {
            11
        };
        assert_eq!(runtime.integer_value(read()), expected);
        assert_eq!(
            runtime.send_slice(machine, objects.oop_at_put, &[index, value]),
            value
        );
        assert_eq!(read(), value);
        if !matches!(
            CASE.get(),
            Case::Canonical | Case::InvalidOperands | Case::Collection
        ) {
            assert_eq!(
                HITS.get(),
                initial_hits,
                "ineligible sends used the fast path"
            );
        }
        runtime.add_method(
            objects.machine_vtable,
            objects.oop_at,
            crate::execution::machine_oop_at,
        );
        runtime.add_method(
            objects.machine_vtable,
            objects.oop_at_put,
            crate::execution::machine_oop_at_put,
        );
        runtime.add_method(
            globals.vtable_vt,
            globals.lookup,
            crate::dispatch::vtable_lookup_method,
        );
        let before_restored = HITS.get();
        assert_eq!(read(), value);
        assert_eq!(
            HITS.get() - before_restored,
            usize::from(ENABLED.get() && CASE.get() != Case::PrivateReceiver)
        );
        assert_eq!(frame_words(runtime, machine), frame);
        runtime.send_slice(machine, runtime.intern(b"returnAt:").unwrap(), &[index])
    })
}

fn descriptor(step: crate::abi::Method, raw: bool) -> std::sync::Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 741,
            generation: 741,
            source_map: 741,
        },
        program_counter_count: 1,
        driver_argument_count: 0,
        call_sites: Vec::new(),
        step_method: step,
        edges: vec![],
        ..probe_spec(if raw { 2 } else { 3 }, 1)
    })
    .unwrap()
}

fn run(case: Case, enabled: bool) -> (Vec<i64>, Vec<&'static str>) {
    let _mode = Mode::set(enabled);
    CASE.set(case);
    EFFECTS.with_borrow_mut(Vec::clear);
    VALUES.with_borrow_mut(Vec::clear);
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    runtime.gc_set_precise(true);
    let machine = runtime.create_stackless_machine().unwrap();
    let first = runtime.make_integer(11);
    let second = runtime.make_integer(22);
    let answer = runtime
        .drive_stackless(
            machine,
            descriptor(probe_step, true),
            &[(0, first), (1, second)],
        )
        .unwrap();
    assert_eq!(runtime.integer_value(answer), 22);
    assert_eq!(
        runtime
            .stackless_registration_diagnostics()
            .active_frame_pins,
        0
    );
    (
        VALUES.with_borrow(Clone::clone),
        EFFECTS.with_borrow(Clone::clone),
    )
}

#[test]
fn ordinary_sends_preserve_frame_state_mutation_effects_and_gc() {
    for case in [
        Case::Canonical,
        Case::InvalidOperands,
        Case::ReadOverride,
        Case::WriteOverride,
        Case::RemovedRead,
        Case::PrivateReceiver,
        Case::CustomLookup,
        Case::Saturated,
        Case::Collection,
    ] {
        let baseline = run(case, false);
        let candidate = run(case, true);
        assert_eq!(candidate, baseline, "case {case:?}");
        assert_eq!(
            candidate.0,
            vec![11, if case == Case::Collection { 22 } else { 11 }, 22, 22]
        );
        let expected = match case {
            Case::ReadOverride | Case::RemovedRead | Case::Saturated => vec!["read", "read"],
            Case::WriteOverride => vec!["write"],
            Case::CustomLookup => vec!["lookup-read", "lookup-write", "lookup-read"],
            Case::Collection => vec!["gc-write"],
            _ => vec![],
        };
        assert_eq!(candidate.1, expected, "case {case:?}");
    }
}

unsafe extern "C" fn argument_step(machine: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_active(|runtime| {
        runtime.send_slice(
            machine,
            runtime.intern(b"returnAt:").unwrap(),
            &[runtime.make_integer(1)],
        )
    })
}

#[test]
fn registration_is_a_dependency_even_without_dictionary_mutation() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let machine = runtime.create_stackless_machine().unwrap();
    for method in [
        crate::execution::machine_oop_at as crate::abi::Method,
        crate::execution::machine_oop_at_put as crate::abi::Method,
        crate::dispatch::vtable_lookup_method as crate::abi::Method,
    ] {
        assert!(runtime.guarded_slot_access_eligible(machine));
        let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
        let registration = runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                method,
                descriptor(argument_step, false),
                1,
                0,
                1,
            ))
            .unwrap();
        assert_eq!(
            unsafe { (*runtime.state.get()).vtables.mutation_epoch() },
            epoch
        );
        assert!(!runtime.guarded_slot_access_eligible(machine));
        runtime
            .unregister_unpublished_stackless_resumable(registration)
            .unwrap();
        assert!(runtime.guarded_slot_access_eligible(machine));
    }
}

#[test]
fn canonical_address_registered_as_resumable_uses_ordinary_invocation() {
    for enabled in [false, true] {
        let _mode = Mode::set(enabled);
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime.create_stackless_machine().unwrap();
        let value = runtime.make_integer(99);
        let method = crate::execution::machine_oop_at;
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                method,
                descriptor(argument_step, false),
                1,
                0,
                1,
            ))
            .unwrap();
        // A distinct binding publishes the entry while retaining the canonical
        // Machine dictionary's original address for oopAt:.
        runtime.add_method(
            runtime.globals().object_vt,
            runtime.intern(b"registeredFloorRead").unwrap(),
            method,
        );
        let answer = runtime
            .enter(|| runtime.send_slice(machine, runtime.intern(b"oopAt:").unwrap(), &[value]));
        assert_eq!(
            answer, value,
            "the registered implementation returns its argument"
        );
        assert_eq!(HITS.get(), 0);
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .active_frame_pins,
            0
        );
    }
}
