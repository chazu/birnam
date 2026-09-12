use super::*;

struct CollectionMode;
impl Drop for CollectionMode {
    fn drop(&mut self) {
        COLLECT_EACH_NODE.set(false);
    }
}

#[test]
fn materialization_preserves_aliases_distinct_equal_numbers_and_exact_bits_through_gc() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    r.gc_set_precise(true);
    let vtable = r.create_vtable(r.globals().object_vt);
    r.set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_OOP);
    let input = r.make_integer(71);
    let bits = 0xfff8_0000_0000_0042;
    let plan = Graph {
        input_count: 2,
        nodes: vec![
            Node::Integer(i64::MIN),
            Node::Integer(i64::MIN),
            Node::FloatBits(bits),
            Node::Aggregate {
                vtable_input: 0,
                fields: vec![
                    Value::Input(1),
                    Value::Node(0),
                    Value::Node(0),
                    Value::Node(1),
                    Value::Node(2),
                ],
            },
            Node::Aggregate {
                vtable_input: 0,
                fields: vec![Value::Node(3), Value::Node(3)],
            },
        ],
        slots: vec![Value::Node(4), Value::Node(4), Value::Input(1)],
    }
    .validate()
    .unwrap();
    COLLECT_EACH_NODE.set(true);
    let _mode = CollectionMode;
    r.enter(|| {
        r.with_materialized_graph(
            &plan,
            &[vtable, input],
            || Ok(()),
            |slots| {
                r.gc_collect_from_native_range(0, 0);
                assert_eq!(slots[0], slots[1]);
                assert_eq!(slots[2], input);
                let read = |object, index| unsafe {
                    let heap = &(*r.state.get()).heap;
                    heap.object_at_address(heap.read_payload_word(object, index).unwrap())
                        .unwrap()
                };
                let inner = read(slots[0], 0);
                assert_eq!(inner, read(slots[0], 1));
                assert_eq!(read(inner, 0), input);
                assert_eq!(read(inner, 1), read(inner, 2));
                assert_ne!(read(inner, 1), read(inner, 3));
                assert_eq!(r.integer_value(read(inner, 1)), i64::MIN);
                assert_eq!(r.integer_value(read(inner, 3)), i64::MIN);
                assert_eq!(r.float_value(read(inner, 4)).to_bits(), bits);
                Ok(())
            },
        )
    })
    .unwrap();
}

unsafe extern "C" fn trace(_: Oop, _: crate::abi::MarkFn) {}

thread_local! {
    static MUTATE_LAYOUT: core::cell::Cell<Oop> = const { core::cell::Cell::new(core::ptr::null_mut()) };
}

unsafe extern "C" fn change_layout(_: Oop) {
    with_active(|r| {
        r.set_layout(
            MUTATE_LAYOUT.get(),
            core::ptr::null(),
            0,
            crate::abi::SLOT_RAW,
        )
    });
}

#[test]
fn rejects_layout_changed_by_real_finalizer_before_construction() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    let vtable = r.create_vtable(r.globals().object_vt);
    r.set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_OOP);
    MUTATE_LAYOUT.set(vtable);
    let garbage = r.make_integer(1);
    r.set_finalizer(garbage, change_layout);
    let plan = Graph {
        input_count: 1,
        nodes: vec![Node::Aggregate {
            vtable_input: 0,
            fields: vec![Value::Input(0)],
        }],
        slots: vec![Value::Node(0)],
    }
    .validate()
    .unwrap();
    COLLECT_EACH_NODE.set(true);
    let _mode = CollectionMode;
    assert!(
        r.enter(|| r.with_materialized_graph(
            &plan,
            &[vtable],
            || Ok(()),
            |_| {
                panic!("a changed layout cannot be published");
                #[allow(unreachable_code)]
                Ok(())
            }
        ))
        .is_err()
    );
    assert_eq!(
        unsafe { (*r.state.get()).vtables.slot_kind(vtable, 0) },
        crate::abi::SLOT_RAW
    );
    MUTATE_LAYOUT.set(core::ptr::null_mut());
}

#[test]
fn empty_aggregate_cannot_forge_a_raw_numeric_carrier() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    let plan = Graph {
        input_count: 1,
        nodes: vec![Node::Aggregate {
            vtable_input: 0,
            fields: vec![],
        }],
        slots: vec![Value::Node(0)],
    }
    .validate()
    .unwrap();
    assert!(
        r.enter(|| r.with_materialized_graph(
            &plan,
            &[r.canonical().smallint_vt],
            || Ok(()),
            |_| {
                panic!("raw carrier is ineligible");
                #[allow(unreachable_code)]
                Ok(())
            }
        ))
        .is_err()
    );
}

#[test]
fn rejects_invalid_heap_inputs_and_untraced_or_custom_storage_before_allocation() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    let vtable = r.create_vtable(r.globals().object_vt);
    let plan = Graph {
        input_count: 1,
        nodes: vec![
            Node::Integer(1),
            Node::Aggregate {
                vtable_input: 0,
                fields: vec![Value::Node(0)],
            },
        ],
        slots: vec![Value::Node(1)],
    }
    .validate()
    .unwrap();
    for mode in 0..4 {
        let input = match mode {
            0 => core::ptr::without_provenance_mut(0x1234),
            1 => vtable,
            2 => {
                r.set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
                vtable
            }
            _ => {
                r.set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_OOP);
                r.set_trace_hook(vtable, trace);
                vtable
            }
        };
        let before = r.gc_live_count();
        assert!(
            r.enter(|| r.with_materialized_graph(
                &plan,
                &[input],
                || Ok(()),
                |_| -> Result<(), ExecutionError> { panic!("must not publish") },
            ))
            .is_err()
        );
        assert_eq!(r.gc_live_count(), before);
    }
}

#[test]
fn failed_authority_recheck_does_not_publish_or_allocate_the_next_node() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.install_execution_objects();
    let plan = Graph {
        input_count: 0,
        nodes: vec![Node::Integer(1), Node::Integer(2)],
        slots: vec![Value::Node(1)],
    }
    .validate()
    .unwrap();
    let calls = core::cell::Cell::new(0);
    let before = r.gc_live_count();
    assert!(
        r.enter(|| r.with_materialized_graph(
            &plan,
            &[],
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 3 {
                    Err(invalid())
                } else {
                    Ok(())
                }
            },
            |_| -> Result<(), ExecutionError> { panic!("must not publish") }
        ))
        .is_err()
    );
    assert_eq!(r.gc_live_count(), before + 1);
    r.enter(|| r.gc_collect_from_native_range(0, 0));
    assert!(
        r.gc_live_count() <= before,
        "failed preparation released its roots"
    );
}
