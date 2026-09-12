//! Real portable staging, native baseline code and production Call dispatch.
use super::*;
use crate::execution::{
    FrameDescriptorV2Spec, StacklessTarget, probe_call, probe_control, probe_spec,
};
use crate::registration::{BirnamResumableRegistrationV2, CARRIER_VERSION_V2, PREPARE_V1};
use crate::runtime::{ScopedOopRootGuard, current_thread_token};
use birnam_artifact::code_binding_inventory;
use birnam_codegen::{Block, Expr, Statement, portable};
use core::cell::Cell;
use std::borrow::Cow;

thread_local! {
    static DECODES: Cell<usize> = const { Cell::new(0) };
    static RETURNS: Cell<usize> = const { Cell::new(0) };
    static LOOKUPS: Cell<usize> = const { Cell::new(0) };
    static MACHINE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
}
unsafe extern "C" fn value(object: Oop) -> i64 {
    DECODES.set(DECODES.get() + 1);
    with_active(|r| r.integer_value(object))
}
unsafe extern "C" fn send(receiver: Oop, selector: Oop, args: *mut Oop, count: i64) -> Oop {
    with_active(|r| {
        if r.stackless_machine_capability(receiver).is_ok() {
            MACHINE.set(receiver);
        }
        unsafe { r.send_abi(receiver, selector, args, count as usize) }
    })
}
struct Installed {
    receiver: Oop,
    selector: Oop,
    descriptor: Arc<FrameDescriptor>,
    generation: u64,
    _unit: Unit,
}
fn install(r: &Runtime, bytes: &[u8], generation: u64, capture: Option<Oop>) -> Installed {
    let programs = portable_inventory::encode_closure(&[bytes]).unwrap();
    let bindings = code_binding_inventory::encode(&[]).unwrap();
    let selected = selection::encode(
        &programs,
        &bindings,
        &[],
        &[selection::Disposition::Resumable],
    )
    .unwrap();
    let view = selection::inspect(&selected, &programs, &bindings).unwrap();
    let row = view.row(0).unwrap();
    let logical = resumable::lower_portable_block(bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let descriptor =
        resumable_descriptor::compile_portable_descriptor_v2(&logical, &layout, row, 1).unwrap();
    let authority = selection::authority(&programs, &bindings, &selected).unwrap();
    let manifest_bytes = manifest::materialize(
        authority,
        &view,
        &[manifest::EntrySpec {
            selection_row_index: 0,
            driver_argument_count: 1,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &descriptor,
        }],
    )
    .unwrap();
    let entry = manifest::inspect(&manifest_bytes, authority, &view)
        .unwrap()
        .entry(0)
        .unwrap();
    let mut source = include_bytes!("shims.c").to_vec();
    source.extend_from_slice(b"extern long birnam_smallIntValue(void*);\n");
    source.extend(resumable_c::emit_step_method(0, &logical, &layout, 1).unwrap());
    source.extend(resumable_c::emit_entry_thunk(0, row.selector));
    source.extend_from_slice(b"void* entry(long n){return n?(void*)__birnam_resumable_step_0:(void*)__birnam_resumable_thunk_0;}\n");
    let unit = unsafe {
        Unit::compile(
            &CString::new(source).unwrap(),
            &[
                (c"birnam_send", send as *const c_void),
                (c"birnam_intern", shim_intern as *const c_void),
                (c"birnam_smallIntValue", value as *const c_void),
                (c"birnam_mkSmallInt", shim_integer as *const c_void),
                (c"birnam_nil", shim_nil as *const c_void),
            ],
        )
    }
    .unwrap();
    let getter: unsafe extern "C" fn(i64) -> *mut c_void =
        unsafe { core::mem::transmute(unit.symbol(c"entry").unwrap().as_ptr()) };
    let thunk: Method = unsafe { core::mem::transmute(getter(0)) };
    let step: Method = unsafe { core::mem::transmute(getter(1)) };
    assert!(r.begin_staged_portable_resumable_unit(
        &programs,
        &bindings,
        &selected,
        &manifest_bytes,
        authority,
        generation,
        generation + 1
    ));
    let mut envelope = BirnamResumableRegistrationV2 {
        carrier_version: CARRIER_VERSION_V2,
        struct_size: core::mem::size_of::<BirnamResumableRegistrationV2>() as u32,
        operation: PREPARE_V1,
        flags: 0,
        manifest_entry_index: 0,
        reserved0: 0,
        registration_token: 0,
        descriptor_bytes: descriptor.as_ptr(),
        descriptor_length: descriptor.len() as u64,
        entry_thunk: Some(thunk),
        step_method: Some(step),
        implementation_oop: core::ptr::null_mut(),
        code_generation_identity: generation,
        code_generation_lease: generation + 1,
        portable_implementation_identity: entry.portable_implementation_identity,
        language_arity: row.language_arity,
        driver_argument_count: 1,
        descriptor_sha256: entry.descriptor_sha256,
        receiver_capture_offset_words: 1,
        reserved1: 0,
    };
    assert_eq!(
        r.enter(|| unsafe { r.register_resumable_envelope(&mut envelope) }),
        crate::registration::OK_V1
    );
    let vt = r.create_vtable(r.globals().object_vt);
    r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
    let selector = r.intern(row.selector).unwrap();
    r.add_method(vt, selector, thunk);
    assert!(r.finish_staged_resumable_unit());
    let receiver = r.allocate_instance(
        vt,
        (1 + usize::from(capture.is_some())) * core::mem::size_of::<Oop>(),
    );
    if let Some(capture) = capture {
        unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(receiver, 1, capture.addr());
        }
    }
    let StacklessTarget::Registered { entry } = r
        .enter(|| r.resolve_stackless_target(receiver, selector))
        .unwrap()
    else {
        panic!("registered fixture");
    };
    let descriptor = r
        .materialize_registered_target(entry, receiver, &[])
        .unwrap()
        .descriptor;
    unsafe {
        (*r.state.get())
            .execution
            .release_acquired_entry(entry)
            .unwrap();
    }
    Installed {
        receiver,
        selector,
        descriptor,
        generation,
        _unit: unit,
    }
}
fn leaf(captured: bool) -> Vec<u8> {
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![Statement::Expr(Expr::Global(if captured {
                b"saved"
            } else {
                b"input"
            }))],
        },
        &if captured {
            vec![b"saved".to_vec()]
        } else {
            vec![]
        },
    )
    .unwrap()
}
fn caller() -> Vec<u8> {
    portable::encode_block(
        &Block {
            params: vec![b"target", b"input"],
            body: vec![
                Statement::Let {
                    name: b"result",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"target")),
                        selector: Cow::Borrowed(b"value:"),
                        args: vec![Expr::Global(b"input")],
                        source: b"",
                    },
                },
                Statement::Expr(Expr::Global(b"result")),
            ],
        },
        &[],
    )
    .unwrap()
}
unsafe extern "C" fn return_override(machine: Oop, args: *const Oop, count: usize) -> Oop {
    RETURNS.set(RETURNS.get() + 1);
    with_active(|r| {
        let cap = r.stackless_machine_capability(machine).unwrap();
        let state = unsafe { &*r.state.get() };
        let arena = state
            .execution
            .machine(current_thread_token(), cap)
            .unwrap()
            .arena();
        if arena
            .descriptor(arena.current_handle().unwrap())
            .unwrap()
            .identity()
            .generation
            == 740100
        {
            assert_eq!(
                arena.diagnostics().live_frames,
                2,
                "overridden return observes the reconstructed callee"
            );
        }
    });
    unsafe { crate::execution::machine_return_at(machine, args, count) }
}
unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    LOOKUPS.set(LOOKUPS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

#[test]
fn production_projection_inline_preserves_identity_allocations_and_malleability() {
    for capture in [false, true] {
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
            let leaf = install(r, &leaf(capture), 740100, capture.then_some(saved));
            let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
            let caller = install(r, &caller(), 740200, None);
            let _caller = ScopedOopRootGuard::new(r, caller.receiver);
            r.native_tier
                .borrow_mut()
                .records
                .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
            let drive = || {
                r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]))
            };
            for _ in 0..80 {
                assert_eq!(drive(), if capture { saved } else { input });
            }
            INLINE_HITS.set(0);
            INLINE_RECOVERIES.set(0);
            DECODES.set(0);
            let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            for _ in 0..16 {
                assert_eq!(drive(), if capture { saved } else { input });
            }
            let allocated = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
            if optimized {
                assert_eq!(INLINE_HITS.get(), 16);
                assert_eq!(Some(allocated), baseline);
                assert_eq!(DECODES.get(), 32);
            } else {
                baseline = Some(allocated);
                assert_eq!(INLINE_HITS.get(), 0);
                assert_eq!(DECODES.get(), 48);
            }
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            RETURNS.set(0);
            LOOKUPS.set(0);
            r.add_method(objects.machine_vtable, objects.return_at, return_override);
            assert_eq!(drive(), if capture { saved } else { input });
            assert_eq!(RETURNS.get(), 2);
            assert_eq!(INLINE_RECOVERIES.get(), usize::from(optimized));
            r.add_method(
                objects.machine_vtable,
                objects.return_at,
                crate::execution::machine_return_at,
            );
            r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_override);
            assert_eq!(drive(), if capture { saved } else { input });
            assert!(LOOKUPS.get() > 0);
            let diagnostics = r.stackless_registration_diagnostics();
            assert_eq!(diagnostics.acquired_entries, 0);
            assert_eq!(diagnostics.active_frame_pins, 0);
            assert_eq!(r.stackless_runtime_diagnostics().live_frames, 0);
            println!(
                "PROJECTION_INLINE {{\"capture\":{capture},\"optimized\":{optimized},\"allocations_bytes\":{allocated},\"inline_hits\":{},\"recoveries\":{}}}",
                INLINE_HITS.get(),
                INLINE_RECOVERIES.get()
            );
        }
    }
}

thread_local! { static RECIPE_SIZE: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn recipe_size(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        r.gc_collect_from_native_range(0, 0);
        r.make_integer(RECIPE_SIZE.get() as i64)
    })
}
unsafe extern "C" fn recovered_caller(machine: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.stackless_return_at(machine, 0).unwrap())
}

#[test]
fn production_recovery_admits_id_recipe_and_rejects_stale_or_invalid_requests() {
    for capture in [false, true] {
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        let input = r.make_integer(71);
        let _input = ScopedOopRootGuard::new(r, input);
        // This two-PC callee is recovered at its post-call continuation, not entry.
        let callee = install(r, &caller(), 741100, None);
        let _callee = ScopedOopRootGuard::new(r, callee.receiver);
        // The ordinary at: policy is a real compiler-produced, captured Id block:
        // [:destination | saved]. The supplied value aliases every recovery slot.
        let policy = install(r, &leaf(true), 741200, Some(input));
        let _policy = ScopedOopRootGuard::new(r, policy.receiver);
        let vt = r.checked_vtable_of(policy.receiver);
        let method = r.lookup_method(vt, policy.selector).unwrap().to_method();
        r.add_method(vt, r.intern(b"at:").unwrap(), method);
        r.add_method(vt, r.intern(b"size").unwrap(), recipe_size);
        RECIPE_SIZE.set(callee.descriptor.slot_count());
        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
        let wrong = r.create_stackless_machine().unwrap();
        let _wrong = ScopedOopRootGuard::new(r, wrong);
        let cap = r.stackless_machine_capability(machine).unwrap();
        let owner = current_thread_token();
        let nil = r.canonical().nil;
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            call_sites: vec![probe_control(0, 1)],
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            step_method: recovered_caller,
            ..probe_spec(1, 2)
        })
        .unwrap();
        unsafe {
            let m = (*r.state.get()).execution.machine_mut(owner, cap).unwrap();
            m.begin_initial_drive(owner).unwrap();
            let frame = m
                .arena_mut()
                .push(descriptor, 0, nil, &[(0, input)])
                .unwrap();
            m.arena_mut()
                .begin_v2_transition(
                    frame,
                    crate::execution::EdgeTransitionKind::Call,
                    1,
                    Some(0),
                )
                .unwrap();
        }
        let selector = r.intern(b"recoverAt:with:").unwrap();
        let pc = r.make_integer(if capture { 0 } else { 1 });
        let _pc = ScopedOopRootGuard::new(r, pc);
        let snapshot = || unsafe {
            (*r.state.get())
                .execution
                .machine(owner, cap)
                .unwrap()
                .arena()
                .recovery_experiment_snapshot()
        };
        let before = snapshot();
        assert_eq!(
            r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
            nil,
            "no request authority"
        );
        let StacklessTarget::Registered { entry } = r
            .enter(|| r.resolve_stackless_target(callee.receiver, callee.selector))
            .unwrap()
        else {
            panic!("registered target");
        };
        let authority = unsafe {
            (*r.state.get())
                .execution
                .recovery_authority(owner, cap, entry, 0)
                .unwrap()
        };
        let slots: Vec<_> = (0..callee.descriptor.slot_count())
            .map(|i| (i, i))
            .collect();
        let values = vec![input; slots.len()];
        let reject = |authority: &crate::execution::RecoveryAuthority,
                      pc,
                      slots: &[(usize, usize)],
                      values: &[Oop]| {
            assert!(
                unsafe {
                    (*r.state.get())
                        .execution
                        .recover_acquired_call(owner, authority, pc, slots, values, nil)
                }
                .is_err()
            );
            assert_eq!(snapshot(), before);
            assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 1);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        };
        reject(&authority, usize::MAX, &slots, &values);
        reject(&authority, 1, &[], &values);
        reject(&authority, 1, &[(0, 0), (0, 0)], &values);
        reject(&authority, 1, &slots, &[]);
        let mut stale = authority.clone();
        stale.epoch += 1;
        reject(&stale, 1, &slots, &values);
        let mut stale = authority.clone();
        stale.machine = r.stackless_machine_capability(wrong).unwrap();
        reject(&stale, 1, &slots, &values);
        r.with_recovery_request(cap, entry, 0, || {
            assert_eq!(
                r.enter(|| r.send_slice(wrong, selector, &[pc, policy.receiver])),
                nil
            );
            RECIPE_SIZE.set(0);
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                nil
            );
            assert_eq!(snapshot(), before);
            RECIPE_SIZE.set(callee.descriptor.slot_count());
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                machine
            );
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                nil,
                "one-shot authority"
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 1);
        r.enter(|| r.gc_collect_from_native_range(0, 0));
        if capture {
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.call_site, capture_call);
            let continuation = r
                .enter(|| r.run_resumed_stackless(machine, cap, owner))
                .unwrap();
            let _continuation = ScopedOopRootGuard::new(r, continuation);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(
                r.stackless_registration_diagnostics()
                    .continuation_frame_pins,
                1
            );
            r.remove_method(r.checked_vtable_of(callee.receiver), callee.selector);
            assert_eq!(
                r.request_stackless_code_generation_retirement(callee.generation),
                crate::runtime::CodeGenerationRetirement::Rejected,
                "a continuation pin prevents retirement"
            );
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert_eq!(
                r.enter(|| r.restore_stackless_continuation(machine, continuation, input))
                    .unwrap(),
                input
            );
            assert_eq!(
                r.stackless_registration_diagnostics()
                    .continuation_frame_pins,
                0
            );
            assert_eq!(
                r.enter(|| r.request_stackless_code_generation_retirement(callee.generation)),
                crate::runtime::CodeGenerationRetirement::Retired
            );
        } else {
            assert_eq!(
                r.enter(|| r.run_resumed_stackless(machine, cap, owner))
                    .unwrap(),
                input
            );
        }
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        r.destroy_stackless_machine(machine).unwrap();
        unsafe {
            (*r.state.get())
                .execution
                .machine_mut(owner, r.stackless_machine_capability(wrong).unwrap())
                .unwrap()
                .fail(owner)
                .unwrap();
        }
        r.destroy_stackless_machine(wrong).unwrap();
        println!(
            "PRODUCTION_RECOVERY {{\"capture\":{capture},\"id_recipe\":true,\"rejections\":10,\"one_shot\":true,\"precise_gc\":true,\"pins_after\":0}}"
        );
    }
}

unsafe extern "C" fn capture_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| {
        let cap = r.stackless_machine_capability(machine).unwrap();
        let (result, pc) = unsafe {
            let state = &*r.state.get();
            let arena = state
                .execution
                .machine(current_thread_token(), cap)
                .unwrap()
                .arena();
            (
                state.heap.read_payload_word(action, 8).unwrap(),
                arena
                    .program_counter(arena.current_handle().unwrap())
                    .unwrap(),
            )
        };
        r.stackless_capture_current(machine, result, pc).unwrap()
    })
}

#[test]
#[ignore = "host-gated projection timing; run scripts/production-recovery.py"]
fn projection_timing() {
    let optimized = std::env::var("BIRNAM_74P_MODE").unwrap() == "on";
    let case = std::env::var("BIRNAM_74P_CASE").unwrap();
    let count: usize = std::env::var("BIRNAM_74P_COUNT")
        .unwrap_or_else(|_| "10000".into())
        .parse()
        .unwrap();
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let input = r.make_integer(73);
    let _input = ScopedOopRootGuard::new(r, input);
    let bytes = if case == "control" {
        portable::encode_block(
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
        .unwrap()
    } else {
        leaf(false)
    };
    let leaf = install(r, &bytes, 740100, None);
    let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
    let caller = install(r, &caller(), 740200, None);
    let _caller = ScopedOopRootGuard::new(r, caller.receiver);
    r.native_tier
        .borrow_mut()
        .records
        .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
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
    DECODES.set(0);
    let before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
    let start = std::time::Instant::now();
    for _ in 0..count {
        assert_eq!(drive(), input);
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let bytes = unsafe { (*r.state.get()).heap.bytes_since_collection() } - before;
    let hits = INLINE_HITS.get();
    let recoveries = INLINE_RECOVERIES.get();
    assert_eq!(
        hits,
        if optimized && case == "projection" {
            count
        } else {
            0
        }
    );
    assert_eq!(
        recoveries, 0,
        "a persistent guard miss was recovered during warmup"
    );
    assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
    println!(
        "PROJECTION_TIMING {{\"optimized\":{optimized},\"case\":\"{case}\",\"count\":{count},\"warm_ns\":{warm_ns},\"elapsed_ns\":{elapsed_ns},\"bytes\":{bytes},\"hits\":{hits},\"recoveries\":{recoveries}}}"
    );
}

thread_local! {
    static BOUNDARY: Cell<u8> = const { Cell::new(0) };
    static GC_DEPTH: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn boundary_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| match BOUNDARY.replace(0) {
        1 => {
            r.ensure_private_vtable(machine);
        }
        2 => unsafe {
            let state = &mut *r.state.get();
            state.heap.allocate(
                crate::gc::AUTO_COLLECTION_THRESHOLD,
                crate::heap::Permanence::Collectable,
            );
            state.metadata.set_auto_requested(true);
        },
        _ => {}
    });
    action
}
unsafe extern "C" fn gc_observe(_: crate::abi::MarkFn) {
    with_active(|r| {
        let cap = r.stackless_machine_capability(MACHINE.get()).unwrap();
        let state = unsafe { &*r.state.get() };
        let arena = state
            .execution
            .machine(current_thread_token(), cap)
            .unwrap()
            .arena();
        assert_eq!(
            arena
                .descriptor(arena.current_handle().unwrap())
                .unwrap()
                .identity()
                .generation,
            740100
        );
        GC_DEPTH.set(arena.diagnostics().live_frames);
    });
}
unsafe extern "C" fn replaced(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.make_integer(99))
}

#[test]
fn production_projection_recovers_before_gc_and_preserves_current_targets() {
    for optimized in [false, true] {
        for boundary in [1, 2] {
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let input = r.make_integer(73);
            let _input = ScopedOopRootGuard::new(r, input);
            let leaf = install(r, &leaf(false), 740100, None);
            let _leaf = ScopedOopRootGuard::new(r, leaf.receiver);
            let caller = install(r, &caller(), 740200, None);
            let _caller = ScopedOopRootGuard::new(r, caller.receiver);
            r.native_tier
                .borrow_mut()
                .records
                .retain(|key, _| optimized && *key == Arc::as_ptr(&leaf.descriptor) as usize);
            let drive = || {
                r.enter(|| r.send_slice(caller.receiver, caller.selector, &[leaf.receiver, input]))
            };
            for _ in 0..80 {
                assert_eq!(drive(), input);
            }
            INLINE_HITS.set(0);
            INLINE_RECOVERIES.set(0);
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.call_site, boundary_call);
            let anchor = 0usize;
            r.gc_set_stack_bottom(core::ptr::from_ref(&anchor).addr());
            r.gc_set_auto_requested(false);
            if boundary == 2 {
                GC_DEPTH.set(0);
                r.gc_register_root_provider(gc_observe);
            }
            BOUNDARY.set(boundary);
            assert_eq!(drive(), input);
            r.gc_set_auto_requested(false);
            assert_eq!(INLINE_RECOVERIES.get(), usize::from(optimized));
            if boundary == 2 {
                assert_eq!(
                    GC_DEPTH.get(),
                    2,
                    "collection must see the reconstructed callee"
                );
            }
            r.add_method(r.checked_vtable_of(leaf.receiver), leaf.selector, replaced);
            assert_eq!(
                r.integer_value(drive()),
                99,
                "lookup must use the current target"
            );
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
            println!(
                "PROJECTION_BOUNDARY {{\"optimized\":{optimized},\"boundary\":{boundary},\"recoveries\":{},\"new_target_answer\":99}}",
                INLINE_RECOVERIES.get()
            );
        }
    }
}
