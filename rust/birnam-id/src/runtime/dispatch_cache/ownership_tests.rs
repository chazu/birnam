use super::*;
use crate::dispatch::build_bootstrap;
use crate::runtime::{ScopedOopRootGuard, with_active};

unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop { receiver }
unsafe extern "C" fn nil(_: Oop, _: *const Oop, _: usize) -> Oop { with_active(|r| r.canonical().nil) }

fn counts() -> (usize, usize) { (HITS.get(), MISSES.get()) }
fn reset_counts() { HITS.set(0); MISSES.set(0); }
fn lookup(r: &Runtime, shape: Oop, selector: Oop) -> Option<MethodAddress> {
    r.enter(|| { let _read = r.enter_managed_turn(); r.cached_default_lookup(shape, selector) })
}
fn report(label: &str) { println!("cache {label}: {:?}", counts()); }

#[test]
fn ownership_and_lookup_transcript() {
    let mut owners = vec![build_bootstrap(), build_bootstrap()];
    let moved = owners.remove(0);
    let r = &moved.runtime;
    r.publish_canonical_exports();r.install_execution_objects();
    let parent = r.create_vtable(r.globals().object_vt);
    let shape = r.create_vtable(parent);
    let selector = r.intern(b"cacheOwnerAnswer").unwrap();
    let receiver = r.allocate_instance(shape, 0);
    let _root = ScopedOopRootGuard::new(r, receiver);
    r.add_method(parent, selector, identity);
    let expected = Some(MethodAddress::from_method(identity));
    r.dispatch_cache.clear();reset_counts();
    for _ in 0..2 { assert_eq!(lookup(r, parent, selector), expected); }
    assert_eq!(counts(), (0, 0));report("direct lookup bypass");
    for _ in 0..2 { assert_eq!(lookup(r, shape, selector), expected); }
    assert_eq!(counts(), (1, 1));report("moved owner warms inherited entry");

    // Find a real interned missing selector colliding with the warmed key.
    let index = |key: usize| ((key >> 4) ^ (key >> 10)) & 63;
    let collision = (0..4096).find_map(|i| {
        let other = r.intern(format!("cacheCollision{i}").as_bytes()).unwrap();
        (index(other.addr()) == index(selector.addr())).then_some(other)
    }).expect("interned collision");
    // Interning may mutate bootstrap dictionaries; warm at the current epoch.
    r.dispatch_cache.clear();reset_counts();
    assert_eq!(lookup(r, shape, selector), expected);
    for _ in 0..2 { assert_eq!(lookup(r, shape, collision), None); }
    assert_eq!(lookup(r, shape, selector), expected);
    assert_eq!(counts(), (1, 3));report("repeated misses retain colliding positive entry");
    r.add_method(parent, collision, nil);
    r.dispatch_cache.clear();reset_counts();
    assert_eq!(lookup(r, shape, selector), expected);
    assert_eq!(lookup(r, shape, collision), Some(MethodAddress::from_method(nil)));
    assert_eq!(lookup(r, shape, selector), expected);
    assert_eq!(counts(), (0, 3));report("positive collision replaces one slot");

    // Existing collection-phase factory changes phase; default lookup must not
    // probe or update the inherited table while collection is in progress.
    for phase in [CollectionPhase::Marking, CollectionPhase::Finalizing] {
        reset_counts();r.collection_phase.set(phase);
        assert_eq!(lookup(r, shape, selector), expected);
        assert_eq!(counts(), (0, 0));
        r.collection_phase.set(CollectionPhase::Idle);
        assert_eq!(lookup(r, shape, selector), expected);
        assert_eq!(counts(), (1, 0));report("phase bypass preserves warm entry");
    }
    r.add_method(parent, selector, nil);reset_counts();
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), r.canonical().nil);
    assert_eq!(counts(), (0, 1));report("method mutation selects current answer");
    let other = r.create_vtable(r.globals().object_vt);
    r.add_method(other, selector, identity);r.set_vtable_parent(shape, other);reset_counts();
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    assert_eq!(counts(), (0, 1));report("parent mutation selects current answer");

    let anchor = 0usize;r.gc_set_stack_bottom((&anchor as *const usize).addr());r.gc_set_precise(true);
    r.enter(|| r.gc_collect());reset_counts();
    assert_eq!(lookup(r, shape, selector), expected);assert_eq!(lookup(r, shape, selector), expected);
    assert_eq!(counts(), (1, 1));report("collection clears then lookup repopulates");
    unsafe { (*r.state.get()).vtables.exhaust_mutation_epoch_for_test(); }
    reset_counts();
    for _ in 0..2 { assert_eq!(lookup(r, shape, selector), expected); }
    assert_eq!(counts(), (0, 0));report("saturated mutation epoch bypass");

    let survivor = &owners[0].runtime;
    let parent = survivor.create_vtable(survivor.globals().object_vt);
    let shape = survivor.create_vtable(parent);
    let selector = survivor.intern(b"cacheOwnerAnswer").unwrap();
    survivor.add_method(parent, selector, identity);reset_counts();
    assert_eq!(lookup(survivor, shape, selector), expected);
    assert_eq!(counts(), (0, 1));report("independent owner starts cold");
    drop(_root);drop(moved);reset_counts();
    assert_eq!(lookup(survivor, shape, selector), expected);
    assert_eq!(counts(), (1, 0));report("independent owner survives destruction");
    let d = survivor.stackless_registration_diagnostics();
    assert_eq!((d.active_frame_pins, d.acquired_entries), (0, 0));
    println!("cache no retained Frame pins or acquired entries");
}
