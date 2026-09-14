use super::*;
use crate::runtime::native_tier::discarded_clone;

struct Mode(bool);
impl Mode {
    fn set(enabled: bool) -> Self {
        Self(discarded_clone::ENABLED.replace(enabled))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        discarded_clone::ENABLED.set(self.0);
    }
}

fn program(clones: usize, escape: bool) -> Vec<u8> {
    program_with_last_selector(clones, escape, b"duplicatePrivately")
}

fn program_with_last_selector(
    clones: usize,
    escape: bool,
    last_selector: &'static [u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    for site in 0..clones {
        body.push(Statement::Let {
            name: b"copy",
            value: Expr::Send {
                receiver: Box::new(Expr::Global(b"prototype")),
                selector: Cow::Borrowed(if site + 1 == clones {
                    last_selector
                } else {
                    b"duplicatePrivately"
                }),
                args: vec![],
                source: b"",
            },
        });
    }
    body.push(Statement::Let {
        name: b"alias",
        value: Expr::Global(b"copy"),
    });
    body.push(Statement::Expr(Expr::Global(if escape {
        b"alias"
    } else {
        b"answer"
    })));
    portable::encode_block(
        &Block {
            params: vec![b"prototype", b"answer"],
            body,
        },
        &[],
    )
    .unwrap()
}

thread_local! {
    static OBSERVATIONS: Cell<usize> = const { Cell::new(0) };
    static MARK_COUNTS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    static FINALIZED: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn clone_observer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::dispatch::object_clone(receiver, args, count) }
}
unsafe extern "C" fn lookup_observer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn call_observer(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::execution::machine_call_site(receiver, args, count) }
}
unsafe extern "C" fn mark_observer(_: crate::abi::MarkFn) {
    with_active(|r| MARK_COUNTS.set((r.gc_live_count(), r.gc_live_bytes())));
}
unsafe extern "C" fn inert_finalizer(_: Oop) {}
unsafe extern "C" fn inert_trace(_: Oop, _: crate::abi::MarkFn) {}
unsafe extern "C" fn finalizer_observer(_: Oop) {
    with_active(|r| {
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(
            !heap.has_dead_allocations(),
            "observing sweep must settle first"
        );
        assert_eq!(heap.physical_allocation_count(), r.gc_live_count());
        FINALIZED.set(FINALIZED.get() + 1);
    });
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Canonical,
    Escape,
    Method,
    Lookup,
    Control,
    Epoch,
    Raw,
    Finalizer,
    Trace,
    Large,
    JournalFull,
    MixedMethod,
}

#[test]
fn discarded_clones_preserve_counts_identity_boundaries_and_late_binding() {
    for clones in [1, 3, 16] {
        for case in [
            Case::Canonical,
            Case::Escape,
            Case::Method,
            Case::Lookup,
            Case::Control,
            Case::Epoch,
            Case::Raw,
            Case::Finalizer,
            Case::Trace,
            Case::Large,
            Case::JournalFull,
            Case::MixedMethod,
        ] {
            let bytes = program_with_last_selector(
                clones,
                matches!(case, Case::Escape),
                if matches!(case, Case::MixedMethod) {
                    b"anotherDuplicate"
                } else {
                    b"duplicatePrivately"
                },
            );
            let mut evidence = Vec::new();
            for enabled in [false, true] {
                let _mode = Mode::set(false);
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                let block = install(r, &bytes, 741601, None);
                let _block = ScopedOopRootGuard::new(r, block.receiver);
                let vt = r.create_vtable(r.globals().object_vt);
                r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
                let selector = r.intern(b"duplicatePrivately").unwrap();
                r.add_method(vt, selector, crate::dispatch::object_clone);
                let another = r.intern(b"anotherDuplicate").unwrap();
                r.add_method(vt, another, crate::dispatch::object_clone);
                let prototype = r.allocate_instance(
                    vt,
                    if matches!(case, Case::Large) {
                        1032
                    } else {
                        16
                    },
                );
                let _prototype = ScopedOopRootGuard::new(r, prototype);
                let answer = r.make_integer(71);
                let _answer = ScopedOopRootGuard::new(r, answer);
                unsafe {
                    (*r.state.get())
                        .heap
                        .write_payload_word(prototype, 0, answer.addr());
                    (*r.state.get())
                        .heap
                        .write_payload_word(prototype, 1, answer.addr());
                }
                for _ in 0..128 {
                    r.enter(|| r.send_slice(block.receiver, block.selector, &[prototype, answer]));
                }
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                match case {
                    Case::MixedMethod => {
                        r.add_method(vt, another, clone_observer);
                    }
                    Case::Method => {
                        r.add_method(vt, selector, clone_observer);
                    }
                    Case::Lookup => {
                        r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_observer);
                    }
                    Case::Control => {
                        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                        r.add_method(objects.machine_vtable, objects.call_site, call_observer);
                    }
                    Case::Epoch => unsafe {
                        (*r.state.get()).vtables.exhaust_mutation_epoch_for_test();
                    },
                    Case::Raw => r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_RAW),
                    Case::Finalizer => {
                        assert!(unsafe {
                            (*r.state.get())
                                .heap
                                .set_finalizer(prototype, inert_finalizer)
                        });
                    }
                    Case::Trace => r.set_trace_hook(vt, inert_trace),
                    Case::JournalFull => {
                        let heap = unsafe { &mut (*r.state.get()).heap };
                        let snapshot = heap.prepare_dead_clone(prototype).unwrap();
                        for _ in 0..4096 {
                            assert!(heap.record_dead_allocation(snapshot.clone()));
                        }
                    }
                    _ => {}
                }
                let machine = r.create_stackless_machine().unwrap();
                let _machine = ScopedOopRootGuard::new(r, machine);
                discarded_clone::ENABLED.set(enabled);
                discarded_clone::HITS.set(0);
                OBSERVATIONS.set(0);
                let before = (r.gc_live_count(), r.gc_live_bytes(), unsafe {
                    (*r.state.get()).heap.physical_allocation_count()
                });
                let result = r
                    .enter(|| {
                        r.drive_stackless(
                            machine,
                            block.descriptor.clone(),
                            &[(0, block.receiver), (1, prototype), (2, answer)],
                        )
                    })
                    .unwrap();
                if matches!(case, Case::Escape) {
                    assert_ne!(result, prototype);
                    assert_ne!(result, answer);
                    for slot in 0..2 {
                        assert_eq!(
                            unsafe { (*r.state.get()).heap.read_payload_word(result, slot) },
                            Some(answer.addr())
                        );
                    }
                } else {
                    assert_eq!(result, answer);
                }
                let hits = if enabled && clones >= 3 && matches!(case, Case::Canonical) {
                    clones
                } else {
                    0
                };
                assert_eq!(
                    discarded_clone::HITS.get(),
                    hits,
                    "{case:?}, {clones} clones"
                );
                let count = r.gc_live_count() - before.0;
                assert_eq!(
                    unsafe { (*r.state.get()).heap.physical_allocation_count() } - before.2,
                    count - hits
                );
                let bytes = r.gc_live_bytes() - before.1;
                let cap = r.stackless_machine_capability(machine).unwrap();
                let diagnostics = unsafe {
                    (*r.state.get())
                        .execution
                        .machine_diagnostics(current_thread_token(), cap)
                }
                .unwrap();
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                r.gc_register_root_provider(mark_observer);
                let expected_mark = (r.gc_live_count(), r.gc_live_bytes());
                let reclaimed = r.enter(|| r.gc_collect_from_native_range(0, 0));
                assert_eq!(MARK_COUNTS.get(), expected_mark);
                assert!(!unsafe { (*r.state.get()).heap.has_dead_allocations() });
                evidence.push((
                    count,
                    bytes,
                    OBSERVATIONS.get(),
                    diagnostics.current_synchronous_boundary_depth,
                    diagnostics.high_water_synchronous_boundary_depth,
                    reclaimed,
                    r.gc_live_count(),
                    r.gc_live_bytes(),
                ));
            }
            assert_eq!(evidence[0], evidence[1], "{case:?}, {clones} clones");
        }
    }
}

#[test]
fn counted_frame_indices_match_physical_gc_and_foreign_finalizer_observations() {
    for precise in [false, true] {
        for observing in [false, true] {
            let mut evidence = Vec::new();
            for counted in [false, true] {
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                r.gc_set_precise(precise);
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                let indices = [0, 1, 1, 127];
                for _ in 0..3 {
                    if counted {
                        assert!(unsafe {
                            (*r.state.get())
                                .heap
                                .record_dead_frame_indices(r.canonical().smallint_vt, &indices)
                        });
                    } else {
                        for index in indices {
                            r.make_integer(index as i64);
                        }
                    }
                }
                if observing {
                    let garbage = r.make_integer(99);
                    assert!(unsafe {
                        (*r.state.get())
                            .heap
                            .set_finalizer(garbage, finalizer_observer)
                    });
                }
                FINALIZED.set(0);
                r.gc_register_root_provider(mark_observer);
                let before = (r.gc_live_count(), r.gc_live_bytes());
                assert_eq!(
                    before.0 - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                    if counted { 12 } else { 0 },
                );
                let reclaimed = r.enter(|| r.gc_collect_from_native_range(0, 0));
                assert_eq!(MARK_COUNTS.get(), before);
                assert_eq!(FINALIZED.get(), usize::from(observing));
                assert!(!unsafe { (*r.state.get()).heap.has_dead_allocations() });
                evidence.push((
                    before,
                    reclaimed,
                    r.gc_live_count(),
                    r.gc_live_bytes(),
                    unsafe { (*r.state.get()).heap.bytes_since_collection() },
                ));
            }
            assert_eq!(
                evidence[0], evidence[1],
                "precise={precise} observing={observing}"
            );
        }
    }
}

#[test]
fn discarded_clone_journal_is_untraced_and_settles_before_foreign_finalizers() {
    for words in [1usize, 2] {
        for observing in [false, true] {
            let mut evidence = Vec::new();
            for elide in [false, true] {
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                let vt = r.create_vtable(r.globals().object_vt);
                r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
                let prototype = r.allocate_instance(vt, words * core::mem::size_of::<Oop>());
                let _prototype = ScopedOopRootGuard::new(r, prototype);
                let child = r.make_integer(902);
                unsafe {
                    for slot in 0..words {
                        (*r.state.get())
                            .heap
                            .write_payload_word(prototype, slot, child.addr());
                    }
                }
                for _ in 0..3 {
                    if elide {
                        let heap = unsafe { &mut (*r.state.get()).heap };
                        let snapshot = heap.prepare_dead_clone(prototype).unwrap();
                        assert!(heap.record_dead_allocation(snapshot));
                    } else {
                        r.clone_object(prototype);
                    }
                }
                // The original reference disappears after the optimized body. Saved
                // bytes in unreachable clones must not keep their former child alive.
                unsafe {
                    for slot in 0..words {
                        (*r.state.get()).heap.write_payload_word(
                            prototype,
                            slot,
                            r.canonical().nil.addr(),
                        );
                    }
                }
                if observing {
                    let garbage = r.allocate_instance(vt, 0);
                    assert!(unsafe {
                        (*r.state.get())
                            .heap
                            .set_finalizer(garbage, finalizer_observer)
                    });
                }
                FINALIZED.set(0);
                r.gc_register_root_provider(mark_observer);
                let before = (r.gc_live_count(), r.gc_live_bytes());
                assert_eq!(
                    before.0 - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                    if elide { 3 } else { 0 }
                );
                let reclaimed = r.enter(|| r.gc_collect_from_native_range(0, 0));
                assert_eq!(MARK_COUNTS.get(), before);
                assert_eq!(FINALIZED.get(), usize::from(observing));
                assert_eq!(unsafe { (*r.state.get()).heap.payload_size(child) }, None);
                assert!(!unsafe { (*r.state.get()).heap.has_dead_allocations() });
                evidence.push((
                    before,
                    reclaimed,
                    r.gc_live_count(),
                    r.gc_live_bytes(),
                    unsafe { (*r.state.get()).heap.bytes_since_collection() },
                ));
            }
            assert_eq!(evidence[0], evidence[1], "observing finalizer={observing}");
        }
    }
}

thread_local! {
    static GC_INPUT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static COLLECTIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn collecting_mutation(mark: crate::abi::MarkFn) {
    COLLECTIONS.set(COLLECTIONS.get() + 1);
    with_active(|r| {
        assert!(unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(GC_INPUT.get(), 0, 999)
        });
        unsafe { mark(GC_INPUT.get()) };
    });
}

#[test]
fn discarded_clone_budget_preserves_collection_and_mark_callback_effects() {
    let bytes = program(3, false);
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let plan = birnam_codegen::discarded_clone::Plan::derive(&logical, &layout).unwrap();
    let box_size = crate::heap::Heap::allocation_total_size(8).unwrap();
    let clone_size = crate::heap::Heap::allocation_total_size(16).unwrap();
    let body_bytes = (plan.integer_boxes() + 1) * box_size + 3 * clone_size;
    for gap in [
        body_bytes + 2 * box_size,
        body_bytes + box_size,
        body_bytes,
        (body_bytes / 2) & !(core::mem::size_of::<Oop>() - 1),
        box_size,
    ] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(false);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let block = install(r, &bytes, 741602, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let vt = r.create_vtable(r.globals().object_vt);
            r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
            r.add_method(
                vt,
                r.intern(b"duplicatePrivately").unwrap(),
                crate::dispatch::object_clone,
            );
            let prototype = r.allocate_instance(vt, 16);
            let _prototype = ScopedOopRootGuard::new(r, prototype);
            let answer = r.make_integer(71);
            let _answer = ScopedOopRootGuard::new(r, answer);
            for _ in 0..128 {
                assert_eq!(
                    r.enter(|| r.send_slice(block.receiver, block.selector, &[prototype, answer])),
                    answer
                );
            }
            let machine = r.create_stackless_machine().unwrap();
            let _machine = ScopedOopRootGuard::new(r, machine);
            let current = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let target = crate::gc::AUTO_COLLECTION_THRESHOLD - gap;
            r.allocate_instance(
                r.globals().object_vt,
                target - current - crate::heap::Heap::allocation_total_size(0).unwrap(),
            );
            assert_eq!(
                unsafe { (*r.state.get()).heap.bytes_since_collection() },
                target
            );
            GC_INPUT.set(answer);
            COLLECTIONS.set(0);
            r.gc_register_root_provider(collecting_mutation);
            discarded_clone::ENABLED.set(enabled);
            discarded_clone::HITS.set(0);
            let anchor = [0usize; 32];
            r.gc_set_stack_bottom(anchor.as_ptr().addr() + core::mem::size_of_val(&anchor));
            let result = r
                .enter(|| {
                    r.drive_stackless(
                        machine,
                        block.descriptor.clone(),
                        &[(0, block.receiver), (1, prototype), (2, answer)],
                    )
                })
                .unwrap();
            r.gc_set_stack_bottom(0);
            assert_eq!(result, answer);
            assert_eq!(
                discarded_clone::HITS.get(),
                if enabled && gap > body_bytes { 3 } else { 0 },
                "gap={gap}"
            );
            assert_eq!(
                COLLECTIONS.get(),
                usize::from(gap < body_bytes),
                "gap={gap}"
            );
            evidence.push((r.integer_value(result), COLLECTIONS.get(), unsafe {
                (*r.state.get()).heap.bytes_since_collection()
            }));
        }
        assert_eq!(evidence[0], evidence[1], "allocation threshold gap={gap}");
    }
}

#[test]
fn discarded_clone_machine_finalizer_admission_requires_exact_inert_capability() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let machine = r.create_stackless_machine().unwrap();
    let inert = |object, finalizer| unsafe {
        let state = &*r.state.get();
        state
            .execution
            .inert_machine_finalizer(&state.heap, object, finalizer)
    };
    assert!(!inert(machine, crate::execution::machine_finalizer));
    let capability = r.stackless_machine_capability(machine).unwrap();
    unsafe {
        (*r.state.get())
            .execution
            .destroy_machine(current_thread_token(), capability)
    }
    .unwrap();
    assert!(inert(machine, crate::execution::machine_finalizer));
    assert!(!inert(machine, inert_finalizer));
    let active = r.create_stackless_machine().unwrap();
    assert!(!inert(active, crate::execution::machine_finalizer));
    assert!(
        inert(machine, crate::execution::machine_finalizer),
        "reused index does not revive old generation"
    );
    let malformed = r.allocate_instance(r.globals().object_vt, 8);
    assert!(!inert(malformed, crate::execution::machine_finalizer));
}

#[test]
fn discarded_clone_profitability_uses_actual_prototype_identity() {
    let mut body: Vec<_> = [b"left".as_slice(), b"left", b"right"]
        .into_iter()
        .map(|receiver| Statement::Let {
            name: b"copy",
            value: Expr::Send {
                receiver: Box::new(Expr::Global(receiver)),
                selector: Cow::Borrowed(b"duplicatePrivately"),
                args: vec![],
                source: b"",
            },
        })
        .collect();
    body.push(Statement::Expr(Expr::Global(b"answer")));
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"left", b"right", b"answer"],
            body,
        },
        &[],
    )
    .unwrap();
    for aliases in [false, true] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(false);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let block = install(r, &bytes, 741604, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let vt = r.create_vtable(r.globals().object_vt);
            r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
            r.add_method(
                vt,
                r.intern(b"duplicatePrivately").unwrap(),
                crate::dispatch::object_clone,
            );
            let left = r.allocate_instance(vt, 16);
            let _left = ScopedOopRootGuard::new(r, left);
            let right = r.allocate_instance(vt, 16);
            let _right = ScopedOopRootGuard::new(r, right);
            let answer = r.make_integer(71);
            let _answer = ScopedOopRootGuard::new(r, answer);
            for object in [left, right] {
                for slot in 0..2 {
                    assert!(unsafe {
                        (*r.state.get())
                            .heap
                            .write_payload_word(object, slot, answer.addr())
                    });
                }
            }
            let invoke = || {
                r.enter(|| {
                    r.send_slice(
                        block.receiver,
                        block.selector,
                        &[left, if aliases { left } else { right }, answer],
                    )
                })
            };
            for _ in 0..128 {
                assert_eq!(invoke(), answer);
            }
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            discarded_clone::HITS.set(0);
            discarded_clone::ENABLED.set(enabled);
            let before = (r.gc_live_count(), r.gc_live_bytes());
            assert_eq!(invoke(), answer);
            let hits = if aliases && enabled { 3 } else { 0 };
            assert_eq!(discarded_clone::HITS.get(), hits);
            assert_eq!(
                r.gc_live_count() - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                hits
            );
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            evidence.push((r.gc_live_count() - before.0, r.gc_live_bytes() - before.1));
        }
        assert_eq!(
            evidence[0], evidence[1],
            "equal payloads are not identical prototypes; aliases={aliases}"
        );
    }
}

#[test]
#[ignore = "paired allocation timing; host qualification is external"]
fn discarded_clone_timing() {
    let batches = std::env::var("BENCH_CLONE_BATCHES")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((1..=500).contains(&batches));
    for clones in [1, 3, 16] {
        let bytes = program(clones, false);
        for payload in [16, 1024] {
            for pair in 0..4 {
                for enabled in if pair % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let _mode = Mode::set(enabled);
                    let bootstrap = crate::dispatch::build_bootstrap();
                    let r = &bootstrap.runtime;
                    r.publish_canonical_exports();
                    r.install_execution_objects();
                    let block = install(r, &bytes, 741603, None);
                    let _block = ScopedOopRootGuard::new(r, block.receiver);
                    let vt = r.create_vtable(r.globals().object_vt);
                    r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
                    r.add_method(
                        vt,
                        r.intern(b"duplicatePrivately").unwrap(),
                        crate::dispatch::object_clone,
                    );
                    let prototype = r.allocate_instance(vt, payload);
                    let _prototype = ScopedOopRootGuard::new(r, prototype);
                    let answer = r.make_integer(71);
                    let _answer = ScopedOopRootGuard::new(r, answer);
                    for slot in 0..payload / core::mem::size_of::<Oop>() {
                        assert!(unsafe {
                            (*r.state.get())
                                .heap
                                .write_payload_word(prototype, slot, answer.addr())
                        });
                    }
                    let invoke = || {
                        r.enter(|| {
                            r.send_slice(block.receiver, block.selector, &[prototype, answer])
                        })
                    };
                    let start = std::time::Instant::now();
                    for _ in 0..128 {
                        assert_eq!(invoke(), answer);
                    }
                    let cold_ns = start.elapsed().as_nanos();
                    r.enter(|| r.gc_collect_from_native_range(0, 0));
                    discarded_clone::HITS.set(0);
                    let mut ns = 0;
                    let mut gc_ns = 0;
                    let mut allocation_bytes = 0;
                    let mut physical_saved = 0;
                    let mut journal_peak_bytes = 0;
                    for _ in 0..batches {
                        let before = r.gc_live_bytes();
                        let start = std::time::Instant::now();
                        for _ in 0..128 {
                            assert_eq!(invoke(), answer);
                        }
                        ns += start.elapsed().as_nanos();
                        allocation_bytes += r.gc_live_bytes() - before;
                        let heap = unsafe { &(*r.state.get()).heap };
                        let missing = heap.allocation_count() - heap.physical_allocation_count();
                        assert_eq!(
                            missing,
                            if enabled && clones >= 3 {
                                clones * 128
                            } else {
                                0
                            }
                        );
                        physical_saved += missing;
                        journal_peak_bytes =
                            journal_peak_bytes.max(heap.dead_allocation_storage_bytes());
                        assert!(
                            heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD
                        );
                        let start = std::time::Instant::now();
                        r.enter(|| r.gc_collect_from_native_range(0, 0));
                        gc_ns += start.elapsed().as_nanos();
                    }
                    let calls = batches * 128;
                    let hits = discarded_clone::HITS.get();
                    assert_eq!(
                        hits,
                        if enabled && clones >= 3 {
                            calls * clones
                        } else {
                            0
                        }
                    );
                    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                    let physical_saved_bytes =
                        physical_saved * crate::heap::Heap::allocation_total_size(payload).unwrap();
                    println!(
                        "discarded-clone clones={clones} payload={payload} pair={pair} enabled={enabled} calls={calls} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} physical_saved_bytes={physical_saved_bytes} journal_peak_bytes={journal_peak_bytes} hits={hits} cold_ns={cold_ns}"
                    );
                }
            }
        }
    }
}

#[test]
fn numeric_batches_are_untraced_and_settle_with_clone_entries_before_finalizers() {
    for observing in [false, true] {
        let mut evidence = Vec::new();
        for elide in [false, true] {
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            let child = r.make_integer(902);
            let raw = child.addr() as i64;
            if elide {
                let heap = unsafe { &mut (*r.state.get()).heap };
                let batch = heap
                    .prepare_numeric_batch(r.canonical().smallint_vt, vec![raw, raw, -5, 999])
                    .unwrap();
                // Unexecuted final element must neither charge nor materialize.
                for index in 0..3 {
                    heap.record_numeric_result(batch.clone(), index).unwrap();
                }
                let clone = heap.prepare_dead_clone(r.canonical().nil).unwrap();
                assert!(heap.record_dead_allocation(clone));
            } else {
                r.make_integer(raw);
                r.make_integer(raw);
                r.make_integer(-5);
                r.clone_object(r.canonical().nil);
            }
            if observing {
                let garbage = r.make_integer(0);
                assert!(unsafe {
                    (*r.state.get())
                        .heap
                        .set_finalizer(garbage, finalizer_observer)
                });
            }
            FINALIZED.set(0);
            r.gc_register_root_provider(mark_observer);
            let before = (r.gc_live_count(), r.gc_live_bytes());
            let charged = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            assert_eq!(
                before.0 - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                if elide { 4 } else { 0 }
            );
            let reclaimed = r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert_eq!(MARK_COUNTS.get(), before);
            assert_eq!(FINALIZED.get(), usize::from(observing));
            assert_eq!(unsafe { (*r.state.get()).heap.payload_size(child) }, None);
            assert!(!unsafe { (*r.state.get()).heap.has_dead_allocations() });
            evidence.push((
                before,
                charged,
                reclaimed,
                r.gc_live_count(),
                r.gc_live_bytes(),
            ));
        }
        assert_eq!(evidence[0], evidence[1], "observing finalizer={observing}");
    }
}

unsafe extern "C" fn observing_string_cleanup(object: Oop) {
    unsafe {
        finalizer_observer(object);
        crate::public_api::string_finalizer(object);
    }
}

#[test]
fn floor_string_cleanup_preserves_deferred_allocation_observations() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "runtime::native_tier::leaf_tests::discarded_clone_tests::floor_string_cleanup_process_case",
            "--exact", "--ignored", "--nocapture", "--test-threads=1",
        ])
        .env("BIRNAM_TEST_STRING_CLEANUP_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "real public-runtime child invoked by the parent differential test"]
fn floor_string_cleanup_process_case() {
    assert_eq!(
        std::env::var("BIRNAM_TEST_STRING_CLEANUP_CHILD").as_deref(),
        Ok("1")
    );
    crate::public_api::initialize_once();
    let r = crate::public_api::runtime_for_string_cleanup_test();
    let retiring_selector = r.intern(b"retiringCleanupBinding").unwrap();
    let nonobserving = |runtime: &Runtime, object, finalizer| {
        crate::public_api::string_cleanup_matches_runtime(runtime, finalizer)
            && runtime.string_bytes(object).is_some()
    };
    let string = r.make_string_with_finalizer(b"admission", crate::public_api::string_finalizer);
    assert!(nonobserving(r, string, crate::public_api::string_finalizer));
    assert!(!nonobserving(r, string, observing_string_cleanup));
    assert!(!nonobserving(r, string, inert_finalizer));
    let integer = r.make_integer(42);
    assert!(!nonobserving(
        r,
        integer,
        crate::public_api::string_finalizer
    ));
    {
        let private = crate::dispatch::build_bootstrap();
        let object = private.runtime.make_string_with_finalizer(
            b"other runtime owner",
            crate::public_api::string_finalizer,
        );
        assert!(private.runtime.string_bytes(object).is_some());
        assert!(!nonobserving(
            &private.runtime,
            object,
            crate::public_api::string_finalizer
        ));
    }
    for precise in [false, true] {
        // Exact cleanup, observing wrapper, late replacement, dead dictionary.
        for observer in 0..4 {
            let mut evidence = Vec::new();
            for elide in [false, true] {
                r.gc_set_precise(precise);
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                let string = r.make_string_with_finalizer(
                    b"owned cleanup bytes",
                    if observer == 1 {
                        observing_string_cleanup
                    } else {
                        crate::public_api::string_finalizer
                    },
                );
                let child = r.make_integer(902);
                let raw = child.addr() as i64;
                if elide {
                    let heap = unsafe { &mut (*r.state.get()).heap };
                    let batch = heap
                        .prepare_numeric_batch(r.canonical().smallint_vt, vec![raw, raw, -5])
                        .unwrap();
                    for index in 0..3 {
                        heap.record_numeric_result(batch.clone(), index).unwrap();
                    }
                    for _ in 0..3 {
                        let snapshot = heap.prepare_dead_clone(r.canonical().nil).unwrap();
                        assert!(heap.record_dead_allocation(snapshot));
                    }
                    assert!(
                        heap.record_dead_frame_indices(r.canonical().smallint_vt, &[0, 1, 1, 127])
                    );
                } else {
                    for value in [raw, raw, -5, 0, 1, 1, 127] {
                        r.make_integer(value);
                    }
                    for _ in 0..3 {
                        r.clone_object(r.canonical().nil);
                    }
                }
                if observer == 2 {
                    r.set_finalizer(string, observing_string_cleanup);
                }
                if observer == 3 {
                    let vt = r.create_vtable(r.globals().object_vt);
                    r.add_method(vt, retiring_selector, crate::dispatch::object_clone);
                }
                FINALIZED.set(0);
                r.gc_register_root_provider(mark_observer);
                let before = (r.gc_live_count(), r.gc_live_bytes());
                let charged = unsafe { (*r.state.get()).heap.bytes_since_collection() };
                assert_eq!(
                    before.0 - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                    if elide { 10 } else { 0 }
                );
                let settled_before = crate::heap::Heap::settled_allocation_count_for_test();
                let reclaimed = r.enter(|| r.gc_collect_from_native_range(0, 0));
                let settled =
                    crate::heap::Heap::settled_allocation_count_for_test() - settled_before;
                assert_eq!(
                    settled,
                    if elide && observer != 0 { 10 } else { 0 },
                    "precise={precise}, observer={observer}, elide={elide}"
                );
                assert_eq!(MARK_COUNTS.get(), before);
                assert_eq!(FINALIZED.get(), usize::from(observer == 1 || observer == 2));
                assert!(
                    r.string_bytes(string).is_none(),
                    "cleanup must release its owned bytes"
                );
                assert!(!nonobserving(
                    r,
                    string,
                    crate::public_api::string_finalizer
                ));
                let heap = unsafe { &(*r.state.get()).heap };
                assert_eq!(
                    heap.payload_size(child),
                    None,
                    "deferred payloads are not roots"
                );
                assert_eq!(heap.payload_size(string), None);
                assert!(!heap.has_dead_allocations());
                evidence.push((
                    before,
                    charged,
                    reclaimed,
                    r.gc_live_count(),
                    r.gc_live_bytes(),
                ));
            }
            assert_eq!(
                evidence[0], evidence[1],
                "precise={precise}, observer={observer}"
            );
        }
    }
}
