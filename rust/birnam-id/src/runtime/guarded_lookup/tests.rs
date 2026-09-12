use super::*;
use crate::dispatch::build_bootstrap;
use crate::execution::{DescriptorIdentity, FrameDescriptor, SemanticRegistrationV1};
use crate::execution::{FrameDescriptorV2Spec, probe_spec};
use crate::runtime::with_active;
use core::cell::{Cell, RefCell};
use std::sync::Arc;

thread_local! {
    static EFFECTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static LOOKUPS: Cell<usize> = const { Cell::new(0) };
}

struct Mode(bool);
impl Mode {
    fn new(enabled: bool) -> Self {
        HITS.set(0);
        Self(ENABLED.replace(enabled))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        ENABLED.set(self.0);
    }
}

unsafe extern "C" fn one(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.make_integer(1))
}
unsafe extern "C" fn two(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.make_integer(2))
}
unsafe extern "C" fn lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    LOOKUPS.set(LOOKUPS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn dnu(_: Oop, args: *const Oop, count: usize) -> Oop {
    EFFECTS.with_borrow_mut(|effects| {
        effects.extend(
            unsafe { core::slice::from_raw_parts(args, count) }
                .iter()
                .map(|oop| oop.addr()),
        )
    });
    with_active(|runtime| runtime.make_integer(count as i64))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Canonical,
    Replacement,
    Inheritance,
    ShadowRemoval,
    GlobalLookup,
    PrivateMeta,
    Saturated,
    Missing,
}

fn run(case: Case, enabled: bool) -> (i64, usize, Vec<usize>) {
    let _mode = Mode::new(enabled);
    LOOKUPS.set(0);
    EFFECTS.with_borrow_mut(Vec::clear);
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let selector = runtime.intern(b"lookupProbe:").unwrap();
    let parent = runtime.create_vtable(runtime.globals().object_vt);
    let child = runtime.create_vtable(parent);
    let receiver = runtime.allocate_instance(child, 0);
    runtime.add_method(parent, selector, one);
    let answer = runtime.enter(|| runtime.send_slice(receiver, selector, &[]));
    assert_eq!(runtime.integer_value(answer), 1);
    assert_eq!(HITS.get() != 0, enabled);
    match case {
        Case::Canonical => {}
        Case::Replacement => {
            runtime.add_method(parent, selector, two);
        }
        Case::Inheritance => {
            let other = runtime.create_vtable(runtime.globals().object_vt);
            runtime.add_method(other, selector, two);
            runtime.set_vtable_parent(child, other);
        }
        Case::ShadowRemoval => {
            runtime.add_method(child, selector, two);
            let shadow = runtime.enter(|| runtime.send_slice(receiver, selector, &[]));
            assert_eq!(runtime.integer_value(shadow), 2);
            runtime.remove_method(child, selector).unwrap();
        }
        Case::GlobalLookup => {
            runtime.add_method(
                runtime.globals().vtable_vt,
                runtime.globals().lookup,
                lookup,
            );
        }
        Case::PrivateMeta => {
            // Vtables already own their dictionaries; ensurePrivateVtable
            // returns them unchanged. Model the bootstrap/image header path.
            let meta = runtime.create_vtable(runtime.globals().vtable_vt);
            unsafe {
                let state = &mut *runtime.state.get();
                assert!(state.heap.set_vtable(child, meta));
                state.vtables.note_mutation();
            }
            runtime.add_method(meta, runtime.globals().lookup, lookup);
        }
        Case::Saturated => {
            unsafe {
                (*runtime.state.get())
                    .vtables
                    .exhaust_mutation_epoch_for_test();
            }
            let _read = runtime.enter_dispatch_read();
            assert_eq!(runtime.guarded_default_lookup_eligible(child), enabled);
            runtime.add_method(
                runtime.globals().vtable_vt,
                runtime.globals().lookup,
                lookup,
            );
        }
        Case::Missing => {
            runtime.remove_method(parent, selector).unwrap();
            runtime.add_method(parent, runtime.globals().does_not_understand, dnu);
        }
    }
    let values = [runtime.canonical().nil; 20];
    let answer = runtime.enter(|| runtime.send_slice(receiver, selector, &values));
    let expected = match case {
        Case::Replacement | Case::Inheritance => 2,
        Case::Missing => 17,
        _ => 1,
    };
    assert_eq!(runtime.integer_value(answer), expected);
    let lookups = LOOKUPS.get();
    assert_eq!(
        lookups,
        usize::from(matches!(
            case,
            Case::GlobalLookup | Case::PrivateMeta | Case::Saturated
        )),
        "{case:?}, enabled={enabled}"
    );
    let effects = EFFECTS.with_borrow(|effects| {
        if case == Case::Missing {
            assert_eq!(effects[0], selector.addr());
            assert!(
                effects[1..]
                    .iter()
                    .all(|value| *value == runtime.canonical().nil.addr())
            );
        }
        // Normalize process-local object addresses for the paired comparison.
        effects.iter().enumerate().map(|(index, _)| index).collect()
    });
    (expected, lookups, effects)
}

#[test]
fn guarded_lookup_preserves_dictionary_changes_and_lookup_effects() {
    for case in [
        Case::Canonical,
        Case::Replacement,
        Case::Inheritance,
        Case::ShadowRemoval,
        Case::GlobalLookup,
        Case::PrivateMeta,
        Case::Saturated,
        Case::Missing,
    ] {
        assert_eq!(run(case, false), run(case, true), "{case:?}");
    }
}

unsafe extern "C" fn lookup_step(machine: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_active(|runtime| {
        let method = MethodAddress::from_method(two).as_oop();
        runtime.stackless_oop_at_put(machine, 0, method).unwrap();
        runtime.stackless_return_at(machine, 0).unwrap()
    })
}

fn descriptor() -> Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 700,
            generation: 701,
            source_map: 0,
        },
        program_counter_count: 1,
        driver_argument_count: 0,
        step_method: lookup_step,
        call_sites: Vec::new(),
        edges: vec![],
        ..probe_spec(2, 1)
    })
    .unwrap()
}

#[test]
fn native_registration_invalidates_lookup_guard_without_dictionary_change() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let child = runtime.create_vtable(runtime.globals().object_vt);
    let receiver = runtime.allocate_instance(child, 0);
    let selector = runtime.intern(b"registeredLookupProbe").unwrap();
    runtime.add_method(child, selector, one);
    let method = crate::dispatch::vtable_lookup_method as Method;
    let _read = runtime.enter_dispatch_read();
    assert!(runtime.guarded_default_lookup_eligible(child));
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    let registration = runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(method, descriptor(), 1, 0, 1))
        .unwrap();
    assert_eq!(epoch, unsafe {
        (*runtime.state.get()).vtables.mutation_epoch()
    });
    assert!(!runtime.guarded_default_lookup_eligible(child));
    runtime
        .unregister_unpublished_stackless_resumable(registration)
        .unwrap();
    assert!(runtime.guarded_default_lookup_eligible(child));
    runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(method, descriptor(), 1, 0, 1))
        .unwrap();
    runtime.add_method(
        runtime.globals().object_vt,
        runtime.intern(b"publishedLookupCarrier").unwrap(),
        method,
    );
    let result = runtime.enter(|| runtime.send_slice(receiver, selector, &[]));
    assert_eq!(
        runtime.integer_value(result),
        2,
        "ordinary lookup must invoke the registered carrier"
    );
    assert_eq!(
        runtime
            .stackless_registration_diagnostics()
            .active_frame_pins,
        0
    );
}
