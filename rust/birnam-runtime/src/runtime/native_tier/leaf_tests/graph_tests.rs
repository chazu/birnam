//! Ordinary Id graph producers exercising real registered recovery, not only
//! the graph constructor. The vector fixture collects on each protocol send.
use super::*;

unsafe extern "C" fn vector_size(object: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| {
        r.gc_collect_from_native_range(0, 0);
        unsafe {
            let heap = &(*r.state.get()).heap;
            heap.object_at_address(heap.read_payload_word(object, 0).unwrap())
                .unwrap()
        }
    })
}
unsafe extern "C" fn vector_at(object: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|r| {
        assert_eq!(count, 1);
        let index = crate::execution::nonnegative_index(r, unsafe { args.read() }).unwrap();
        r.gc_collect_from_native_range(0, 0);
        unsafe {
            let heap = &(*r.state.get()).heap;
            heap.object_at_address(heap.read_payload_word(object, index + 1).unwrap())
                .unwrap()
        }
    })
}

fn vector(r: &Runtime, vtable: Oop, values: &[Oop]) -> Oop {
    let size = r.make_integer(values.len() as i64);
    let object = r.allocate_instance(vtable, (values.len() + 1) * core::mem::size_of::<Oop>());
    unsafe {
        let heap = &mut (*r.state.get()).heap;
        assert!(heap.write_payload_word(object, 0, size.addr()));
        for (index, value) in values.iter().enumerate() {
            assert!(heap.write_payload_word(object, index + 1, value.addr()));
        }
    }
    object
}

#[test]
fn id_graph_recipe_materializes_shared_aggregate_and_numeric_nodes_into_registered_frames() {
    for capture in [false, true] {
        let bootstrap = crate::dispatch::build_bootstrap();
        let r = &bootstrap.runtime;
        r.publish_canonical_exports();
        r.install_execution_objects();
        r.gc_set_precise(true);
        let input = r.make_integer(71);
        let _input = ScopedOopRootGuard::new(r, input);
        let callee = install(r, &caller(), 741410, None);
        let _callee = ScopedOopRootGuard::new(r, callee.receiver);
        let vector_vt = r.create_vtable(r.globals().object_vt);
        r.set_layout(vector_vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
        r.add_method(vector_vt, r.intern(b"size").unwrap(), vector_size);
        r.add_method(vector_vt, r.intern(b"at:").unwrap(), vector_at);
        let aggregate_vt = r.create_vtable(r.globals().object_vt);
        r.set_layout(aggregate_vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
        let integer_node = vector(r, vector_vt, &[r.make_integer(0), r.make_integer(i64::MAX)]);
        let float_node = vector(r, vector_vt, &[r.make_integer(1), r.make_float(-0.0)]);
        let fields = vector(
            r,
            vector_vt,
            &[
                r.make_integer(1),
                r.make_integer(-1),
                r.make_integer(-1),
                r.make_integer(-2),
            ],
        );
        let aggregate_node = vector(
            r,
            vector_vt,
            &[r.make_integer(2), r.make_integer(0), fields],
        );
        let inputs = vector(r, vector_vt, &[aggregate_vt, input]);
        let nodes = vector(r, vector_vt, &[integer_node, float_node, aggregate_node]);
        let slots = vector(
            r,
            vector_vt,
            &vec![r.make_integer(-3); callee.descriptor.slot_count()],
        );
        let graph = vector(r, vector_vt, &[inputs, nodes, slots]);
        let _graph = ScopedOopRootGuard::new(r, graph);

        // The graph's at: implementation is compiled Id code that sends at: to
        // its captured data. Native storage beneath the protocol is incidental.
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"saved")),
                    selector: Cow::Borrowed(b"at:"),
                    args: vec![Expr::Global(b"input")],
                    source: b"saved at: input",
                })],
            },
            &[b"saved".to_vec()],
        )
        .unwrap();
        let policy = install(r, &bytes, 741420, Some(graph));
        let _policy = ScopedOopRootGuard::new(r, policy.receiver);
        let vt = r.checked_vtable_of(policy.receiver);
        r.add_method(
            vt,
            r.intern(b"at:").unwrap(),
            r.lookup_method(vt, policy.selector).unwrap().to_method(),
        );
        r.add_method(vt, r.intern(b"size").unwrap(), recipe_size);
        RECIPE_SIZE.set(3);

        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
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
        let selector = r.intern(b"recoverAt:withGraph:").unwrap();
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
            nil
        );
        assert_eq!(snapshot(), before);
        let StacklessTarget::Registered { entry } = r
            .enter(|| r.resolve_stackless_target(callee.receiver, callee.selector))
            .unwrap()
        else {
            panic!("registered target");
        };
        r.with_recovery_request(cap, entry, 0, || {
            RECIPE_SIZE.set(0);
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                nil
            );
            assert_eq!(snapshot(), before);
            RECIPE_SIZE.set(3);
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                machine
            );
            assert_eq!(
                r.enter(|| r.send_slice(machine, selector, &[pc, policy.receiver])),
                nil
            );
            Ok(())
        })
        .unwrap();
        let aggregate = unsafe {
            let arena = (*r.state.get())
                .execution
                .machine(owner, cap)
                .unwrap()
                .arena();
            let frame = arena.current_handle().unwrap();
            let value = arena.read_oop(frame, 0).unwrap();
            for slot in 1..callee.descriptor.slot_count() {
                assert_eq!(arena.read_oop(frame, slot).unwrap(), value);
            }
            value
        };
        let _aggregate = ScopedOopRootGuard::new(r, aggregate);
        r.enter(|| r.gc_collect_from_native_range(0, 0));
        let read = |i| unsafe {
            let heap = &(*r.state.get()).heap;
            heap.object_at_address(heap.read_payload_word(aggregate, i).unwrap())
                .unwrap()
        };
        assert_eq!(read(0), input);
        assert_eq!(read(1), read(2));
        assert_eq!(r.integer_value(read(1)), i64::MAX);
        assert_eq!(r.float_value(read(3)).to_bits(), (-0.0f64).to_bits());
        if capture {
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(objects.machine_vtable, objects.call_site, capture_call);
            let continuation = r
                .enter(|| r.run_resumed_stackless(machine, cap, owner))
                .unwrap();
            let _continuation = ScopedOopRootGuard::new(r, continuation);
            r.remove_method(r.checked_vtable_of(callee.receiver), callee.selector);
            assert_eq!(
                r.request_stackless_code_generation_retirement(callee.generation),
                crate::runtime::CodeGenerationRetirement::Rejected
            );
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert_eq!(
                r.enter(|| r.restore_stackless_continuation(machine, continuation, aggregate))
                    .unwrap(),
                aggregate
            );
            assert_eq!(
                r.enter(|| r.request_stackless_code_generation_retirement(callee.generation)),
                crate::runtime::CodeGenerationRetirement::Retired
            );
        } else {
            assert_eq!(
                r.enter(|| r.run_resumed_stackless(machine, cap, owner))
                    .unwrap(),
                aggregate
            );
        }
        assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        assert_eq!(r.stackless_registration_diagnostics().acquired_entries, 0);
        r.destroy_stackless_machine(machine).unwrap();
    }
}
