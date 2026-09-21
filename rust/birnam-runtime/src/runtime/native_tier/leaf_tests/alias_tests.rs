//! 74R: canonical compiled execution is the independent oracle for alias plans.
use super::*;
use core::cell::RefCell;

thread_local! {
    pub(super) static BOXES: RefCell<Option<Vec<i64>>> = const { RefCell::new(None) };
    static EFFECTS: RefCell<Vec<(u8, usize)>> = const { RefCell::new(Vec::new()) };
    static SUBSTITUTE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
}
pub(crate) fn record_box(value: i64) {
    BOXES.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            trace.push(value);
        }
    });
}
fn aliases(captured: bool) -> Vec<u8> {
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: Expr::Global(if captured { b"saved" } else { b"input" }),
                },
                Statement::Let {
                    name: b"second",
                    value: Expr::Global(b"first"),
                },
                Statement::Let {
                    name: b"third",
                    value: Expr::Global(b"second"),
                },
                Statement::Expr(Expr::Global(b"third")),
            ],
        },
        &if captured {
            vec![b"saved".to_vec()]
        } else {
            vec![]
        },
    )
    .unwrap()
}
fn eligible_frame(r: &Runtime, machine: Oop) -> bool {
    let cap = r.stackless_machine_capability(machine).unwrap();
    let arena = unsafe {
        (*r.state.get())
            .execution
            .machine(current_thread_token(), cap)
            .unwrap()
            .arena()
    };
    if arena
        .descriptor(arena.current_handle().unwrap())
        .unwrap()
        .identity()
        .generation
        != 740100
    {
        return false;
    }
    assert_eq!(
        arena.diagnostics().live_frames,
        2,
        "Machine observer sees the recovered callee"
    );
    true
}
unsafe extern "C" fn read_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let observed = with_active(|r| {
        if !eligible_frame(r, machine) {
            return false;
        }
        let index = r.integer_value(unsafe { args.read() }) as usize;
        EFFECTS.with_borrow_mut(|events| events.push((b'r', index)));
        true
    });
    let value = unsafe { crate::execution::machine_oop_at(machine, args, count) };
    if observed && !SUBSTITUTE.get().is_null() {
        SUBSTITUTE.get()
    } else {
        value
    }
}
unsafe extern "C" fn write_observer(machine: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        if eligible_frame(r, machine) {
            let index = r.integer_value(unsafe { args.read() }) as usize;
            EFFECTS.with_borrow_mut(|events| events.push((b'w', index)));
        }
    });
    unsafe { crate::execution::machine_oop_at_put(machine, args, count) }
}

#[test]
fn production_alias_inline_matches_canonical_boxes_identity_and_effects() {
    for captured in [false, true] {
        let mut baseline = None;
        for optimized in [false, true] {
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let input = r.make_integer(37);
            let _input = ScopedOopRootGuard::new(r, input);
            let saved = r.make_integer(93);
            let _saved = ScopedOopRootGuard::new(r, saved);
            let leaf = install(r, &aliases(captured), 740100, captured.then_some(saved));
            let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
            let caller = install(r, &caller(), 740200, None);
            let _caller = ScopedOopRootGuard::new(r, caller.receiver);
            r.compiler().native_tier
                .borrow_mut()
                .records
                .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
            let drive = || {
                r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]))
            };
            for _ in 0..80 {
                assert_eq!(drive(), if captured { saved } else { input });
            }
            INLINE_HITS.set(0);
            INLINE_RECOVERIES.set(0);
            DECODES.set(0);
            BOXES.set(Some(Vec::new()));
            let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            for _ in 0..16 {
                assert_eq!(drive(), if captured { saved } else { input });
            }
            let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
            let boxes = BOXES.take().unwrap();
            assert_eq!(INLINE_HITS.get(), if optimized { 16 } else { 0 });
            assert_eq!(DECODES.get(), if optimized { 32 } else { 48 });
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.oop_at, read_observer);
            r.add_method(objects.machine_vtable, objects.oop_at_put, write_observer);
            r.add_method(objects.machine_vtable, objects.return_at, return_override);
            EFFECTS.with_borrow_mut(Vec::clear);
            RETURNS.set(0);
            SUBSTITUTE.set(if captured { input } else { saved });
            assert_eq!(
                drive(),
                SUBSTITUTE.get(),
                "custom reads change the result through all aliases"
            );
            assert_eq!(drive(), SUBSTITUTE.get());
            SUBSTITUTE.set(core::ptr::null_mut());
            let effects = EFFECTS.take();
            assert_eq!(effects.len(), 12, "exactly three read/write pairs per call");
            assert_eq!(RETURNS.get(), 4);
            assert_eq!(
                INLINE_RECOVERIES.get(),
                usize::from(optimized),
                "persistent miss recovers once"
            );
            if optimized {
                assert_eq!(
                    baseline.as_ref(),
                    Some(&(bytes, boxes.clone(), effects.clone()))
                );
            } else {
                baseline = Some((bytes, boxes.clone(), effects));
            }
            r.add_method(
                objects.machine_vtable,
                objects.oop_at,
                crate::execution::machine_oop_at,
            );
            r.add_method(
                objects.machine_vtable,
                objects.oop_at_put,
                crate::execution::machine_oop_at_put,
            );
            r.add_method(
                objects.machine_vtable,
                objects.return_at,
                crate::execution::machine_return_at,
            );
            r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_override);
            LOOKUPS.set(0);
            assert_eq!(drive(), if captured { saved } else { input });
            assert!(LOOKUPS.get() > 0);
            assert_eq!(INLINE_RECOVERIES.get(), 2 * usize::from(optimized));
            r.add_method(r.checked_vtable_of(leaf.receiver), leaf.selector, replaced);
            assert_eq!(r.integer_value(drive()), 99);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
            println!(
                "SMALL_CALLEE {{\"captured\":{captured},\"optimized\":{optimized},\"bytes\":{bytes},\"boxes\":{},\"hits\":{},\"recoveries\":{},\"effects\":12}}",
                boxes.len(),
                INLINE_HITS.get(),
                INLINE_RECOVERIES.get()
            );
        }
    }
}

// Leave room for the old projection's two boxes, but not for the alias body.
pub(super) unsafe extern "C" fn alias_boundary_call(
    machine: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| {
        if BOUNDARY.replace(0) == 0 {
            return;
        }
        unsafe {
            let state = &mut *r.state.get();
            let box_size = crate::heap::Heap::allocation_total_size(8).unwrap();
            let target = crate::gc::AUTO_COLLECTION_THRESHOLD - 3 * box_size;
            let total = target - state.heap.bytes_since_collection();
            let payload = total - crate::heap::Heap::allocation_total_size(0).unwrap();
            state
                .heap
                .allocate(payload, crate::heap::Permanence::Collectable);
            assert_eq!(state.heap.bytes_since_collection(), target);
            state.metadata.set_auto_requested(true);
        }
    });
    action
}
#[test]
fn production_alias_recovery_precedes_gc_inside_boxing_sequence() {
    for optimized in [false, true] {
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let input = r.make_integer(73);
        let _input = ScopedOopRootGuard::new(r, input);
        let leaf = install(r, &aliases(false), 740100, None);
        let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
        let caller = install(r, &caller(), 740200, None);
        let _caller = ScopedOopRootGuard::new(r, caller.receiver);
        r.compiler().native_tier
            .borrow_mut()
            .records
            .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
        let drive =
            || r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]));
        for _ in 0..80 {
            assert_eq!(drive(), input);
        }
        INLINE_RECOVERIES.set(0);
        GC_DEPTH.set(0);
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(
            objects.machine_vtable,
            objects.call_site,
            alias_boundary_call,
        );
        let anchor = 0usize;
        r.gc_set_stack_bottom(core::ptr::from_ref(&anchor).addr());
        r.gc_register_root_provider(gc_observe);
        BOUNDARY.set(1);
        assert_eq!(drive(), input);
        r.gc_set_auto_requested(false);
        assert_eq!(GC_DEPTH.get(), 2);
        assert_eq!(INLINE_RECOVERIES.get(), usize::from(optimized));
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        println!(
            "SMALL_CALLEE_GC {{\"optimized\":{optimized},\"observed_depth\":2,\"recoveries\":{}}}",
            INLINE_RECOVERIES.get()
        );
    }
}

thread_local! { static SOURCE_EFFECTS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn observe_source(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        assert!(eligible_frame(r, MACHINE.get()));
    });
    SOURCE_EFFECTS.set(SOURCE_EFFECTS.get() + 1);
    receiver
}
#[test]
fn production_alias_call_boundary_preserves_source_effects() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let input = r.make_integer(73);
    let _input = ScopedOopRootGuard::new(r, input);
    let vt = r.ensure_private_vtable(input);
    r.add_method(vt, r.intern(b"observe").unwrap(), observe_source);
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"alias",
                    value: Expr::Global(b"input"),
                },
                Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"alias")),
                    selector: Cow::Borrowed(b"observe"),
                    args: vec![],
                    source: b"",
                }),
            ],
        },
        &[],
    )
    .unwrap();
    let program = resumable::lower_portable_block(&bytes).unwrap();
    assert_eq!(
        small_callee::compile(&program, &resumable::color_slots(&program).unwrap()),
        Err(small_callee::Refusal::RegionShape)
    );
    let leaf = install(r, &bytes, 740100, None);
    let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
    assert!(
        r.compiler().native_tier
            .borrow()
            .records
            .values()
            .all(|record| record.leaf.is_none())
    );
    let caller = install(r, &caller(), 740200, None);
    let _caller = ScopedOopRootGuard::new(r, caller.receiver);
    // Existing PC specialization is orthogonal to this entry-inline test.
    r.compiler().native_tier.borrow_mut().records.clear();
    SOURCE_EFFECTS.set(0);
    INLINE_HITS.set(0);
    for _ in 0..96 {
        assert_eq!(
            r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input])),
            input
        );
    }
    assert_eq!(SOURCE_EFFECTS.get(), 96);
    assert_eq!(INLINE_HITS.get(), 0);
    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
    println!(
        "SMALL_CALLEE_REFUSAL {{\"reason\":\"RegionShape\",\"source_effects\":96,\"hits\":0}}"
    );
}

pub(super) fn timing_body(case: &str) -> Vec<u8> {
    match case {
        "projection" => leaf(false),
        "alias" | "fallback" => aliases(false),
        "alias1" => portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![
                    Statement::Let {
                        name: b"alias",
                        value: Expr::Global(b"input"),
                    },
                    Statement::Expr(Expr::Global(b"alias")),
                ],
            },
            &[],
        )
        .unwrap(),
        "control" => portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![
                    Statement::Let {
                        name: b"constant",
                        value: Expr::SmallInt(1),
                    },
                    Statement::Expr(Expr::Global(b"input")),
                ],
            },
            &[],
        )
        .unwrap(),
        _ => panic!("unknown case"),
    }
}

#[test]
#[ignore = "74R paired timing; run scripts/small-callee.py"]
fn small_callee_timing() {
    // 74S isolates dispatch specialization while retaining identical 74R code.
    if let Ok(mode) = std::env::var("BIRNAM_74S_DISPATCH") {
        crate::runtime::dispatch_cache::ENABLED.set(mode == "on");
    }
    let mode = std::env::var("BIRNAM_74R_MODE").unwrap();
    let case = std::env::var("BIRNAM_74R_CASE").unwrap();
    let count: usize = std::env::var("BIRNAM_74R_COUNT")
        .unwrap_or_else(|_| "10000".into())
        .parse()
        .unwrap();
    assert!(["off", "74p", "on"].contains(&mode.as_str()));
    let bytes = timing_body(&case);
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let input = r.make_integer(73);
    let _input = ScopedOopRootGuard::new(r, input);
    let prep = std::time::Instant::now();
    let leaf = install(r, &bytes, 740100, None);
    let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
    let prepare_ns = prep.elapsed().as_nanos();
    let caller = install(r, &caller(), 740200, None);
    let _caller = ScopedOopRootGuard::new(r, caller.receiver);
    r.compiler().native_tier.borrow_mut().records.retain(|key, record| {
        mode != "off"
            && *key == Arc::as_ptr(&leaf.descriptor) as usize
            && (mode != "74p" || record.leaf.as_ref().is_some_and(|p| p.alias_count() == 0))
    });
    if case == "fallback" {
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        r.add_method(objects.machine_vtable, objects.return_at, return_override);
    }
    let drive =
        || r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]));
    let warm = std::time::Instant::now();
    for _ in 0..80 {
        assert_eq!(drive(), input);
    }
    let warm_ns = warm.elapsed().as_nanos();
    INLINE_HITS.set(0);
    INLINE_RECOVERIES.set(0);
    let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
    let start = std::time::Instant::now();
    for _ in 0..count {
        assert_eq!(drive(), input);
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
    let eligible = (case == "projection" && mode != "off")
        || (["alias", "alias1"].contains(&case.as_str()) && mode == "on");
    assert_eq!(INLINE_HITS.get(), if eligible { count } else { 0 });
    assert_eq!(INLINE_RECOVERIES.get(), 0);
    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
    assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let plan_start = std::time::Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(small_callee::compile(
            std::hint::black_box(&logical),
            std::hint::black_box(&layout),
        ))
        .ok();
    }
    let plan_ns = plan_start.elapsed().as_nanos() / 1000;
    let plan = small_callee::compile(&logical, &layout).ok();
    let recipe_slots = plan.as_ref().map_or(0, |p| p.recovery_sources().len());
    let boxed_indices = plan.as_ref().map_or(0, |p| p.boxed_indices().len());
    println!(
        "SMALL_CALLEE_TIMING {{\"mode\":\"{mode}\",\"case\":\"{case}\",\"count\":{count},\"prepare_ns\":{prepare_ns},\"warm_ns\":{warm_ns},\"elapsed_ns\":{elapsed_ns},\"bytes\":{allocated},\"hits\":{},\"recoveries\":{},\"plan_ns\":{plan_ns},\"recipe_slots\":{recipe_slots},\"boxed_indices\":{boxed_indices}}}",
        INLINE_HITS.get(),
        INLINE_RECOVERIES.get()
    );
}
