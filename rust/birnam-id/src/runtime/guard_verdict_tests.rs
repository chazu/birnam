//! Run this same witness against the preserved tuple Cells and extracted owners.
use super::*;
use crate::dispatch::build_bootstrap;
use crate::execution::{DescriptorIdentity, FrameDescriptor, FrameDescriptorV2Spec, SemanticRegistrationV1, probe_spec};

unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop { receiver }
unsafe extern "C" fn read_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { crate::execution::machine_oop_at(receiver, args, count) }
}
unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn step(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.stackless_return_at(machine, 0).unwrap())
}
fn descriptor() -> std::sync::Arc<FrameDescriptor> {
    FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity { implementation: 810, generation: 811, source_map: 0 },
        program_counter_count: 1, driver_argument_count: 0, step_method: step,
        call_sites: Vec::new(), edges: Vec::new(), ..probe_spec(2, 1)
    }).unwrap()
}
fn cells(r: &Runtime) -> [(u64, bool); 2] { [r.slot_guard_cache.get(), r.lookup_guard_cache.get()] }
fn epoch(r: &Runtime) -> u64 { unsafe { (*r.state.get()).vtables.mutation_epoch() } }
fn lookup(r: &Runtime, shape: Oop) -> bool {
    let _read = r.enter_managed_turn();r.guarded_default_lookup_eligible(shape)
}
fn pair(r: &Runtime, machine: Oop, shape: Oop, expected: [bool; 2], label: &str) {
    for _ in 0..2 {
        assert_eq!([r.guarded_slot_access_eligible(machine), lookup(r, shape)], expected);
        assert_eq!(cells(r), [(epoch(r), expected[0]), (epoch(r), expected[1])]);
    }
    println!("verdict {label}: {expected:?}");
}
#[test]
fn runtime_guard_verdict_transcript() {
    let mut owners = vec![build_bootstrap(), build_bootstrap()];
    let moved = owners.remove(0);let r = &moved.runtime;
    let initial = cells(r);assert_eq!(initial[0], (u64::MAX, false));
    // Bootstrap itself performs lookup; the factory tests cover pre-bootstrap cold state.
    assert!(initial[1].1 && initial[1].0 != u64::MAX);
    assert_eq!(cells(&owners[0].runtime), initial);
    assert!(!r.guarded_slot_access_eligible(r.globals().object_vt));
    assert!(!r.guarded_default_lookup_eligible(r.globals().object_vt));
    assert_eq!(cells(r), initial);
    println!("verdict missing execution objects and dispatch authority preserve bootstrap cells");
    r.publish_canonical_exports();r.install_execution_objects();r.gc_set_precise(true);
    let machine = r.create_stackless_machine().unwrap();
    let _machine_root = ScopedOopRootGuard::new(r, machine);
    let parent = r.create_vtable(r.globals().object_vt);
    let shape = r.create_vtable(parent);
    let receiver = r.allocate_instance(shape, 0);
    let _receiver_root = ScopedOopRootGuard::new(r, receiver);
    let selector = r.intern(b"verdictOwnerAnswer").unwrap();r.add_method(parent, selector, identity);
    let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
    pair(r, machine, shape, [true, true], "moved owner warms both proofs");
    assert_eq!(cells(&owners[0].runtime), initial);
    let warm = cells(r);
    assert!(!r.guarded_default_lookup_eligible(shape));
    assert!(!r.guarded_slot_access_eligible(receiver));assert_eq!(cells(r), warm);
    println!("verdict early receiver and reader refusal leave warm state untouched");

    r.add_method(objects.machine_vtable, objects.oop_at, read_override);
    pair(r, machine, shape, [false, true], "opposite verdicts at one epoch");
    r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_override);
    pair(r, machine, shape, [false, false], "both false verdicts remain reusable");
    r.add_method(objects.machine_vtable, objects.oop_at, crate::execution::machine_oop_at);
    r.add_method(r.globals().vtable_vt, r.globals().lookup, crate::dispatch::vtable_lookup_method);
    pair(r, machine, shape, [true, true], "restoring canonical methods recomputes");

    // Registration changes classification without changing dictionary epochs.
    for method in [crate::execution::machine_oop_at as Method, crate::execution::machine_oop_at_put as Method, crate::dispatch::vtable_lookup_method as Method] {
        let before = cells(r);let before_epoch = epoch(r);
        let token = r.register_stackless_resumable(SemanticRegistrationV1::new(method, descriptor(), 1, 0, 1)).unwrap();
        assert_eq!(epoch(r), before_epoch);
        assert!(!r.guarded_slot_access_eligible(machine));
        assert_eq!(lookup(r, shape), method as usize != crate::dispatch::vtable_lookup_method as Method as usize);
        assert_eq!(cells(r), before);
        r.unregister_unpublished_stackless_resumable(token).unwrap();
        assert!(r.guarded_slot_access_eligible(machine));assert!(lookup(r, shape));
    }
    println!("verdict three canonical registrations reject warm proofs without cell or epoch changes");

    // Raw header reassociation mirrors the bootstrap/image path for private metas.
    let meta = r.create_vtable(r.globals().vtable_vt);
    pair(r, machine, shape, [true, true], "before receiver reassociation");
    unsafe { assert!((*r.state.get()).heap.set_vtable(shape, meta));(*r.state.get()).vtables.note_mutation(); }
    let before = cells(r);assert!(!lookup(r, shape));assert_eq!(cells(r), before);
    unsafe { assert!((*r.state.get()).heap.set_vtable(shape, r.globals().vtable_vt));(*r.state.get()).vtables.note_mutation(); }
    pair(r, machine, shape, [true, true], "receiver reassociation restores eligibility");
    let private_machine = r.create_stackless_machine().unwrap();r.ensure_private_vtable(private_machine);
    let before = cells(r);assert!(!r.guarded_slot_access_eligible(private_machine));assert_eq!(cells(r), before);
    println!("verdict private Machine refuses before reading or writing cached proof");

    let other = r.create_vtable(r.globals().object_vt);r.add_method(other, selector, identity);
    r.set_vtable_parent(shape, other);
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    pair(r, machine, shape, [true, true], "parent mutation and ordinary result");
    let before = cells(r);let before_epoch = epoch(r);
    r.enter(|| r.gc_collect_from_native_range(0, 0));
    assert!(epoch(r) >= before_epoch);assert_eq!(cells(r), before);
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    println!("verdict collection preserves both cells and rooted receiver identity");

    unsafe { (*r.state.get()).vtables.exhaust_mutation_epoch_for_test(); }
    pair(r, machine, shape, [true, true], "saturated canonical facts");
    r.add_method(objects.machine_vtable, objects.oop_at, read_override);
    pair(r, machine, shape, [false, true], "saturated true to false without epoch advance");
    r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_override);
    pair(r, machine, shape, [false, false], "saturated lookup replacement");
    r.add_method(objects.machine_vtable, objects.oop_at, crate::execution::machine_oop_at);
    r.add_method(r.globals().vtable_vt, r.globals().lookup, crate::dispatch::vtable_lookup_method);
    pair(r, machine, shape, [true, true], "saturated false to true without epoch advance");
    drop(_receiver_root);drop(_machine_root);drop(moved);
    let survivor = &owners[0].runtime;assert_eq!(cells(survivor), initial);
    survivor.slot_guard_cache.set((19, false));survivor.lookup_guard_cache.set((19, true));
    assert_eq!(cells(survivor), [(19, false), (19, true)]);
    let d = survivor.stackless_registration_diagnostics();assert_eq!((d.active_frame_pins, d.acquired_entries), (0, 0));
    println!("verdict independent owner survives destruction without pins or acquired entries");
}
