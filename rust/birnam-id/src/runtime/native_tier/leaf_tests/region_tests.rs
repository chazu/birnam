use super::super::regions;
use super::*;
use core::cell::RefCell;

thread_local! {
    static ARGUMENT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static INPUT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static LEAF: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static SNAPSHOTS: RefCell<Vec<(usize, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
}

fn bytes(chained: bool, captured: bool) -> Vec<u8> {
    let names: Vec<_> = (0..100).map(|i| format!("alias{i}").into_bytes()).collect();
    let mut body: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| Statement::Let {
            name,
            value: Expr::Global(if chained && i > 0 {
                &names[i - 1]
            } else if captured {
                b"saved"
            } else {
                b"input"
            }),
        })
        .collect();
    body.push(Statement::Expr(Expr::Global(names.last().unwrap())));
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body,
        },
        &if captured {
            vec![b"saved".to_vec()]
        } else {
            vec![]
        },
    )
    .unwrap()
}
fn snapshot(r: &Runtime, machine: Oop) {
    let cap = r.stackless_machine_capability(machine).unwrap();
    let state = unsafe { &*r.state.get() };
    let arena = state
        .execution
        .machine(current_thread_token(), cap)
        .unwrap()
        .arena();
    let handle = arena.current_handle().unwrap();
    let descriptor = arena.descriptor(handle).unwrap();
    if descriptor.identity().generation != 740300 {
        return;
    }
    assert_eq!(arena.diagnostics().live_frames, 2);
    let words = (0..descriptor.slot_count())
        .map(|i| {
            let word = arena.read_oop(handle, i).unwrap();
            if word == INPUT.get() {
                1
            } else if word == LEAF.get() {
                2
            } else if word == ARGUMENT.get() {
                3
            } else if word == CALL_SELECTOR.get() {
                4
            } else {
                assert_eq!(word, r.canonical().nil);
                0
            }
        })
        .collect();
    SNAPSHOTS.with_borrow_mut(|s| s.push((arena.program_counter(handle).unwrap(), words)));
}
unsafe extern "C" fn observe_return(machine: Oop, args: *const Oop, n: usize) -> Oop {
    with_active(|r| snapshot(r, machine));
    unsafe { crate::execution::machine_return_at(machine, args, n) }
}
unsafe extern "C" fn observe_continue(machine: Oop, args: *const Oop, n: usize) -> Oop {
    with_active(|r| snapshot(r, machine));
    unsafe { crate::execution::machine_continue_at(machine, args, n) }
}

#[test]
fn native_region_preserves_all_frame_words_boxes_and_observation_boundaries() {
    for chained in [false, true] {
        for captured in [false, true] {
            let mut baseline = None;
            for mode in ["off", "plain", "reuse"] {
                regions::ENABLED.set(mode != "off");
                regions::REUSE.set(mode == "reuse");
                let bootstrap = crate::dispatch::build_bootstrap();
                let r = &bootstrap.runtime;
                r.publish_canonical_exports();
                r.install_execution_objects();
                let input = r.make_integer(73);
                let _input = ScopedOopRootGuard::new(r, input);
                let saved = r.make_integer(93);
                let _saved = ScopedOopRootGuard::new(r, saved);
                let expected = if captured { saved } else { input };
                INPUT.set(expected);
                ARGUMENT.set(input);
                let leaf = install(
                    r,
                    &bytes(chained, captured),
                    740300,
                    captured.then_some(saved),
                );
                let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
                LEAF.set(leaf.receiver);
                let caller = install(r, &caller(), 740400, None);
                let _caller = ScopedOopRootGuard::new(r, caller.receiver);
                r.native_tier.borrow_mut().records.retain(|key, _| {
                    mode != "off" && *key == Arc::as_ptr(&leaf.descriptor) as usize
                });
                let drive = || {
                    r.enter(|| {
                        r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input])
                    })
                };
                for _ in 0..80 {
                    assert_eq!(drive(), expected);
                }
                let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                r.add_method(objects.machine_vtable, objects.return_at, observe_return);
                r.add_method(
                    objects.machine_vtable,
                    objects.continue_at,
                    observe_continue,
                );
                regions::HITS.set(0);
                regions::LOADS.set(0);
                SNAPSHOTS.with_borrow_mut(Vec::clear);
                alias_tests::BOXES.set(Some(Vec::new()));
                let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
                for _ in 0..8 {
                    assert_eq!(drive(), expected);
                }
                let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
                let boxes = alias_tests::BOXES.take().unwrap();
                let snapshots = SNAPSHOTS.take();
                assert_eq!(snapshots.len(), 16);
                assert_eq!(regions::HITS.get(), if mode == "off" { 0 } else { 16 });
                assert_eq!(
                    regions::LOADS.get(),
                    if mode == "off" {
                        0
                    } else if mode == "reuse" && !chained {
                        16
                    } else {
                        800
                    }
                );
                let observed = (allocated, boxes, snapshots);
                if let Some(baseline) = &baseline {
                    assert_eq!(&observed, baseline);
                } else {
                    baseline = Some(observed);
                }
                // A read override makes the region ineligible before any box.
                r.add_method(objects.machine_vtable, objects.oop_at, read_override);
                regions::HITS.set(0);
                assert_eq!(drive(), expected);
                assert_eq!(regions::HITS.get(), 0);
                assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
            }
        }
    }
    regions::ENABLED.set(true);
    regions::REUSE.set(true);
}

unsafe extern "C" fn read_override(machine: Oop, args: *const Oop, n: usize) -> Oop {
    unsafe { crate::execution::machine_oop_at(machine, args, n) }
}

#[test]
fn native_region_declines_before_gc_and_keeps_compiled_ownership() {
    for enabled in [false, true] {
        regions::ENABLED.set(enabled);
        regions::REUSE.set(true);
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let input = r.make_integer(73);
        let _input = ScopedOopRootGuard::new(r, input);
        let leaf = install(r, &bytes(false, false), 740100, None);
        let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
        let caller = install(r, &caller(), 740400, None);
        let _caller = ScopedOopRootGuard::new(r, caller.receiver);
        r.native_tier
            .borrow_mut()
            .records
            .retain(|key, _| enabled && *key == Arc::as_ptr(&leaf.descriptor) as usize);
        let drive =
            || r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]));
        for _ in 0..80 {
            assert_eq!(drive(), input);
        }
        regions::MISSES.set(0);
        regions::HITS.set(0);
        GC_DEPTH.set(0);
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(
            objects.machine_vtable,
            objects.call_site,
            alias_tests::alias_boundary_call,
        );
        let anchor = 0usize;
        r.gc_set_stack_bottom(core::ptr::from_ref(&anchor).addr());
        r.gc_register_root_provider(gc_observe);
        BOUNDARY.set(1);
        assert_eq!(drive(), input);
        r.gc_set_auto_requested(false);
        assert_eq!(GC_DEPTH.get(), 2);
        assert_eq!(regions::MISSES.get(), usize::from(enabled));
        assert_eq!(
            regions::HITS.get(),
            usize::from(enabled),
            "first PC declines, second can run after collection"
        );
        r.native_tier.borrow_mut().records.clear();
        assert_eq!(drive(), input);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
    }
    regions::ENABLED.set(true);
}

#[test]
#[ignore = "74S isolated whole-PC region timing"]
fn native_region_timing() {
    let mode = std::env::var("BIRNAM_74S_MODE").unwrap();
    let case = std::env::var("BIRNAM_74S_CASE").unwrap();
    let count: usize = std::env::var("BIRNAM_74S_COUNT").unwrap().parse().unwrap();
    assert!(["off", "plain", "reuse"].contains(&mode.as_str()));
    regions::ENABLED.set(mode != "off");
    regions::REUSE.set(mode == "reuse");
    let program = if case == "control" {
        alias_tests::timing_body("control")
    } else if case == "arguments" {
        argument_program(true, b"timingArguments:with:with:with:")
    } else {
        bytes(case == "chain", false)
    };
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let input = r.make_integer(73);
    let _input = ScopedOopRootGuard::new(r, input);
    if case == "arguments" {
        let shape = r.ensure_private_vtable(input);
        r.add_method(
            shape,
            r.intern(b"timingArguments:with:with:with:").unwrap(),
            timing_argument_target,
        );
    }
    let prep = std::time::Instant::now();
    let leaf = install(r, &program, 740300, None);
    let prepare_ns = prep.elapsed().as_nanos();
    let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
    let caller = install(r, &caller(), 740400, None);
    let _caller = ScopedOopRootGuard::new(r, caller.receiver);
    r.native_tier
        .borrow_mut()
        .records
        .retain(|key, _| *key == Arc::as_ptr(&leaf.descriptor) as usize);
    let (source_bytes, metadata_bytes) = {
        let registry = r.native_tier.borrow();
        let prepared = registry
            .records
            .get(&(Arc::as_ptr(&leaf.descriptor) as usize))
            .and_then(|r| r.pending.as_ref());
        prepared.map_or((0, 0), |p| {
            (
                p.source.as_bytes().len(),
                p.regions
                    .iter()
                    .flatten()
                    .map(|r| {
                        core::mem::size_of::<frame_region::Plan>()
                            + 16
                            + r.copies.capacity() * 8
                            + r.boxed_indices.capacity() * 4
                            + r.selector_store.as_ref().map_or(0, |s| s.2.capacity())
                    })
                    .sum::<usize>(),
            )
        })
    };
    if case == "fallback" {
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(objects.machine_vtable, objects.oop_at, read_override);
    }
    let drive =
        || r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]));
    regions::COMPILE_NS.set(0);
    let warm = std::time::Instant::now();
    for _ in 0..80 {
        assert_eq!(drive(), input);
    }
    let warm_ns = warm.elapsed().as_nanos();
    let compile_ns = regions::COMPILE_NS.get();
    regions::HITS.set(0);
    regions::LOADS.set(0);
    regions::MISSES.set(0);
    CALLS.set(0);
    let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
    let start = std::time::Instant::now();
    for _ in 0..count {
        assert_eq!(drive(), input);
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
    let eligible = mode != "off" && ["repeat", "chain", "arguments"].contains(&case.as_str());
    let pcs = if case == "arguments" { 1 } else { 2 };
    let plain_loads = if case == "arguments" { 5 } else { 100 };
    assert_eq!(regions::HITS.get(), if eligible { pcs * count } else { 0 });
    assert_eq!(
        regions::LOADS.get(),
        if !eligible {
            0
        } else if mode == "reuse" && case != "chain" {
            pcs * count
        } else {
            plain_loads * count
        }
    );
    assert_eq!(CALLS.get(), if case == "arguments" { count } else { 0 });
    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
    assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
    println!(
        "REGION_TIMING {{\"mode\":\"{mode}\",\"case\":\"{case}\",\"count\":{count},\"prepare_ns\":{prepare_ns},\"warm_ns\":{warm_ns},\"elapsed_ns\":{elapsed_ns},\"bytes\":{allocated},\"hits\":{},\"loads\":{},\"misses\":{},\"source_bytes\":{source_bytes},\"plan_storage_bytes\":{metadata_bytes},\"compile_ns\":{compile_ns}}}",
        regions::HITS.get(),
        regions::LOADS.get(),
        regions::MISSES.get()
    );
}

thread_local! {
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static CALL_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
}
fn argument_program(tail: bool, selector: &'static [u8]) -> Vec<u8> {
    let send = Expr::Send {
        receiver: Box::new(Expr::Global(b"input")),
        selector: Cow::Borrowed(selector),
        args: (0..4).map(|_| Expr::Global(b"input")).collect(),
        source: b"",
    };
    let body = if tail {
        vec![Statement::Expr(send)]
    } else {
        vec![
            Statement::Let {
                name: b"answer",
                value: send,
            },
            Statement::Expr(Expr::Global(b"answer")),
        ]
    };
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body,
        },
        &[],
    )
    .unwrap()
}
unsafe extern "C" fn argument_target(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    assert_eq!(n, 4);
    assert!(
        unsafe { core::slice::from_raw_parts(args, n) }
            .iter()
            .all(|v| *v == receiver)
    );
    CALLS.set(CALLS.get() + 1);
    with_active(|r| snapshot(r, MACHINE.get()));
    receiver
}
unsafe extern "C" fn replacement_target(_: Oop, _: *const Oop, _: usize) -> Oop {
    CALLS.set(CALLS.get() + 1);
    with_active(|r| r.canonical().nil)
}
unsafe extern "C" fn missing_target(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    assert_eq!(n, 5);
    assert!(
        unsafe { core::slice::from_raw_parts(args.add(1), n - 1) }
            .iter()
            .all(|v| *v == receiver)
    );
    CALLS.set(CALLS.get() + 1);
    receiver
}

#[test]
fn native_region_call_staging_keeps_lookup_effects_and_symbol_allocation() {
    for tail in [false, true] {
        let mut baseline = None;
        for mode in ["off", "plain", "reuse"] {
            regions::ENABLED.set(mode != "off");
            regions::REUSE.set(mode == "reuse");
            let b = crate::dispatch::build_bootstrap();
            let r = &b.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let input = r.make_integer(73);
            let _input = ScopedOopRootGuard::new(r, input);
            INPUT.set(input);
            ARGUMENT.set(input);
            let selector = r.intern(b"regionArguments:with:with:with:").unwrap();
            CALL_SELECTOR.set(selector);
            let shape = r.ensure_private_vtable(input);
            r.add_method(shape, selector, argument_target);
            let leaf = install(
                r,
                &argument_program(tail, b"regionArguments:with:with:with:"),
                740300,
                None,
            );
            let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
            LEAF.set(leaf.receiver);
            let caller = install(r, &caller(), 740400, None);
            let _caller = ScopedOopRootGuard::new(r, caller.receiver);
            r.native_tier
                .borrow_mut()
                .records
                .retain(|key, _| mode != "off" && *key == Arc::as_ptr(&leaf.descriptor) as usize);
            let drive = || {
                r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]))
            };
            for _ in 0..80 {
                assert_eq!(drive(), input);
            }
            regions::HITS.set(0);
            regions::LOADS.set(0);
            CALLS.set(0);
            SNAPSHOTS.with_borrow_mut(Vec::clear);
            alias_tests::BOXES.set(Some(Vec::new()));
            let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            for _ in 0..8 {
                assert_eq!(drive(), input);
            }
            let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
            let boxes = alias_tests::BOXES.take().unwrap();
            let snapshots = SNAPSHOTS.take();
            assert_eq!(CALLS.get(), 8);
            assert_eq!(snapshots.len(), 8);
            assert_eq!(regions::HITS.get(), if mode == "off" { 0 } else { 8 });
            assert_eq!(
                regions::LOADS.get(),
                if mode == "off" {
                    0
                } else if mode == "plain" {
                    40
                } else {
                    8
                }
            );
            let observed = (bytes, boxes, snapshots);
            if let Some(baseline) = &baseline {
                assert_eq!(&observed, baseline);
            } else {
                baseline = Some(observed);
            }
            r.add_method(shape, selector, replacement_target);
            assert_eq!(drive(), r.canonical().nil);
            assert_eq!(CALLS.get(), 9);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
        }
    }
    let mut baseline = None;
    for enabled in [false, true] {
        regions::ENABLED.set(enabled);
        regions::REUSE.set(true);
        let b = crate::dispatch::build_bootstrap();
        let r = &b.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let input = r.make_integer(73);
        let _input = ScopedOopRootGuard::new(r, input);
        let shape = r.ensure_private_vtable(input);
        r.add_method(shape, r.globals().does_not_understand, missing_target);
        let leaf = install(
            r,
            &argument_program(true, b"freshRegionMissing:with:with:with:"),
            740300,
            None,
        );
        let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
        let caller = install(r, &caller(), 740400, None);
        let _caller = ScopedOopRootGuard::new(r, caller.receiver);
        r.native_tier
            .borrow_mut()
            .records
            .retain(|key, _| enabled && *key == Arc::as_ptr(&leaf.descriptor) as usize);
        for record in r.native_tier.borrow_mut().records.values_mut() {
            record.visits = HOT_ENTRIES;
        }
        assert!(
            unsafe {
                (*r.state.get())
                    .symbols
                    .existing(b"freshRegionMissing:with:with:with:")
            }
            .is_none()
        );
        regions::MISSES.set(0);
        CALLS.set(0);
        alias_tests::BOXES.set(Some(Vec::new()));
        let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        assert_eq!(
            r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input])),
            input
        );
        let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
        let boxes = alias_tests::BOXES.take().unwrap();
        assert_eq!(CALLS.get(), 1);
        assert_eq!(regions::MISSES.get(), usize::from(enabled));
        let observed = (bytes, boxes);
        if let Some(baseline) = &baseline {
            assert_eq!(&observed, baseline);
        } else {
            baseline = Some(observed);
        }
    }
    regions::ENABLED.set(true);
    regions::REUSE.set(true);
}

unsafe extern "C" fn timing_argument_target(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    assert_eq!(n, 4);
    assert!(
        unsafe { core::slice::from_raw_parts(args, n) }
            .iter()
            .all(|v| *v == receiver)
    );
    CALLS.set(CALLS.get() + 1);
    receiver
}
