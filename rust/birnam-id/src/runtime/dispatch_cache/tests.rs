use super::*;
use crate::dispatch::build_bootstrap;
use crate::runtime::{ScopedOopRootGuard, with_active};

struct Mode(bool);
impl Mode {
    fn new(on: bool) -> Self {
        HITS.set(0);
        MISSES.set(0);
        Self(ENABLED.replace(on))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        ENABLED.set(self.0);
    }
}
unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    receiver
}
unsafe extern "C" fn nil(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.canonical().nil)
}
thread_local! { static LOOKUPS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn observing_lookup(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    LOOKUPS.set(LOOKUPS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, n) }
}

#[test]
fn dispatch_specialization_observes_warm_mutations_gc_and_lookup() {
    for enabled in [false, true] {
        let _mode = Mode::new(enabled);
        let b = build_bootstrap();
        let r = &b.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let selector = r.intern(b"cachedAnswer").unwrap();
        let parent = r.create_vtable(r.globals().object_vt);
        let child = r.create_vtable(parent);
        let receiver = r.allocate_instance(child, 0);
        let _root = ScopedOopRootGuard::new(r, receiver);
        r.add_method(parent, selector, identity);
        let drive = || r.enter(|| r.send_slice(receiver, selector, &[]));
        for _ in 0..8 {
            assert_eq!(drive(), receiver);
        }
        assert_eq!(HITS.get() > 0, enabled);
        r.add_method(parent, selector, nil);
        assert_eq!(drive(), r.canonical().nil);
        r.add_method(child, selector, identity);
        for _ in 0..8 {
            assert_eq!(drive(), receiver);
        }
        r.remove_method(child, selector).unwrap();
        assert_eq!(drive(), r.canonical().nil);
        let other = r.create_vtable(r.globals().object_vt);
        r.add_method(other, selector, identity);
        r.set_vtable_parent(child, other);
        for _ in 0..8 {
            assert_eq!(drive(), receiver);
        }
        let anchor = 0usize;
        r.gc_set_stack_bottom((&anchor as *const usize).addr());
        r.gc_set_precise(true);
        let epoch = unsafe { (*r.state.get()).vtables.mutation_epoch() };
        assert_eq!(r.dispatch_cache.lookup(epoch, child.addr(), selector.addr()).is_some(), enabled);
        r.enter(|| r.gc_collect());
        // The module checks clearing every slot; here verify the warmed entry
        // was actually invalidated by Runtime's collection path.
        assert!(r.dispatch_cache.lookup(epoch, child.addr(), selector.addr()).is_none());
        assert_eq!(drive(), receiver);
        r.add_method(r.globals().vtable_vt, r.globals().lookup, observing_lookup);
        LOOKUPS.set(0);
        for _ in 0..8 {
            assert_eq!(drive(), receiver);
        }
        assert_eq!(LOOKUPS.get(), 8);
        r.add_method(
            r.globals().vtable_vt,
            r.globals().lookup,
            crate::dispatch::vtable_lookup_method,
        );
        for _ in 0..8 {
            assert_eq!(drive(), receiver);
        }
        unsafe {
            (*r.state.get()).vtables.exhaust_mutation_epoch_for_test();
        }
        r.add_method(other, selector, nil);
        let before = HITS.get();
        for _ in 0..8 {
            assert_eq!(drive(), r.canonical().nil);
        }
        assert_eq!(HITS.get(), before);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
    }
}

unsafe extern "C" fn replace_and_reenter(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        let selector = r.intern(b"replaceAndReenter").unwrap();
        let shape = unsafe { r.vtable_for_valid_receiver_active(receiver) };
        r.add_method(shape, selector, nil);
        assert_eq!(r.send_slice(receiver, selector, &[]), r.canonical().nil);
        receiver
    })
}

#[test]
fn dispatch_specialization_reentry_and_registration_are_late_classified() {
    use crate::execution::{
        DescriptorIdentity, FrameDescriptor, FrameDescriptorV2Spec, SemanticRegistrationV1,
        probe_spec,
    };
    unsafe extern "C" fn step(machine: Oop, _: *const Oop, _: usize) -> Oop {
        with_active(|r| {
            r.stackless_oop_at_put(machine, 0, r.canonical().nil)
                .unwrap();
            r.stackless_return_at(machine, 0).unwrap()
        })
    }
    let b = build_bootstrap();
    let r = &b.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let selector = r.intern(b"cachedRegistration").unwrap();
    let parent = r.create_vtable(r.globals().object_vt);
    let shape = r.create_vtable(parent);
    let receiver = r.allocate_instance(shape, 0);
    r.add_method(parent, selector, identity);
    for _ in 0..8 {
        assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    }
    let epoch = unsafe { (*r.state.get()).vtables.mutation_epoch() };
    let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 740301,
            generation: 740302,
            source_map: 0,
        },
        program_counter_count: 1,
        driver_argument_count: 0,
        step_method: step,
        call_sites: Vec::new(),
        edges: vec![],
        ..probe_spec(2, 1)
    })
    .unwrap();
    let registration = r
        .register_stackless_resumable(SemanticRegistrationV1::new(
            identity,
            descriptor.clone(),
            1,
            0,
            1,
        ))
        .unwrap();
    assert_eq!(epoch, unsafe { (*r.state.get()).vtables.mutation_epoch() });
    // An unpublished registration is deliberately rejected. A cache hit must
    // classify again instead of calling the formerly synchronous address.
    assert!(
        r.enter(|| r.resolve_stackless_target(receiver, selector))
            .is_err()
    );
    r.unregister_unpublished_stackless_resumable(registration)
        .unwrap();
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    r.register_stackless_resumable(SemanticRegistrationV1::new(identity, descriptor, 1, 0, 1))
        .unwrap();
    r.add_method(shape, r.intern(b"publishedCacheCarrier").unwrap(), identity);
    assert_eq!(
        r.enter(|| r.send_slice(receiver, selector, &[])),
        r.canonical().nil
    );
    let selector = r.intern(b"replaceAndReenter").unwrap();
    r.add_method(parent, selector, replace_and_reenter);
    // Populate without invoking the mutating body, then invoke the cached target.
    r.enter(|| {
        let _read = r.enter_dispatch_read();
        unsafe { r.bind_active(receiver, selector) };
    });
    assert_eq!(r.enter(|| r.send_slice(receiver, selector, &[])), receiver);
    assert_eq!(
        r.enter(|| r.send_slice(receiver, selector, &[])),
        r.canonical().nil
    );
    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
    assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
}

#[test]
#[ignore = "74S paired dispatch timings"]
fn dispatch_specialization_timing() {
    let mode = std::env::var("BIRNAM_74S_MODE").unwrap();
    let case = std::env::var("BIRNAM_74S_CASE").unwrap();
    let count: usize = std::env::var("BIRNAM_74S_COUNT").unwrap().parse().unwrap();
    let _mode = Mode::new(mode == "on");
    let prep = std::time::Instant::now();
    let b = build_bootstrap();
    let r = &b.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let selector = r.intern(b"dispatchTiming").unwrap();
    let parent = r.create_vtable(r.globals().object_vt);
    r.add_method(parent, selector, identity);
    let mut shape = parent;
    if ["inherited", "inherited-churn"].contains(&case.as_str()) {
        for _ in 0..12 {
            shape = r.create_vtable(shape);
        }
    }
    let receiver = r.allocate_instance(shape, 0);
    if case == "lookup" {
        r.add_method(r.globals().vtable_vt, r.globals().lookup, observing_lookup);
    }
    let prepare_ns = prep.elapsed().as_nanos();
    let drive = || r.enter(|| r.send_slice(receiver, selector, &[]));
    let warm = std::time::Instant::now();
    for _ in 0..100 {
        assert_eq!(drive(), receiver);
    }
    let warm_ns = warm.elapsed().as_nanos();
    HITS.set(0);
    MISSES.set(0);
    LOOKUPS.set(0);
    let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
    let start = std::time::Instant::now();
    for _ in 0..count {
        if ["churn", "inherited-churn"].contains(&case.as_str()) {
            r.add_method(parent, selector, identity);
        }
        assert_eq!(drive(), receiver);
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
    assert_eq!(bytes, 0);
    assert_eq!(LOOKUPS.get(), if case == "lookup" { count } else { 0 });
    assert_eq!(
        HITS.get(),
        if mode == "on" && case == "inherited" {
            count
        } else {
            0
        }
    );
    println!(
        "DISPATCH_TIMING {{\"mode\":\"{mode}\",\"case\":\"{case}\",\"count\":{count},\"elapsed_ns\":{elapsed_ns},\"prepare_ns\":{prepare_ns},\"warm_ns\":{warm_ns},\"bytes\":{bytes},\"hits\":{},\"misses\":{},\"metadata_bytes\":{}}}",
        HITS.get(),
        MISSES.get(),
        core::mem::size_of::<Cache>()
    );
}
