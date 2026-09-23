//! Recovery and candidate fixtures run in the same production owner as scheduling.
use super::*;
unsafe fn inline(h: &mut H, baseline: Oop, owner: Oop, mode: usize) -> Oop {
    unsafe {
        let v = load(
            h.id,
            "execution_machine_fixture",
            &[
                Operand::object(h.ns),
                Operand::object(baseline),
                Operand::object(owner),
                Operand::word(mode),
                Operand::word(1),
            ],
        );
        h.keep(v)
    }
}
unsafe fn invoke_on_machine(h: &mut H, method: Oop) -> Result<Oop, Outcome> {
    unsafe {
        let receiver = h.block(method, &[]);
        let m = h.machine();
        h.id.send_named(m, b"runBlock:", &[Operand::object(receiver)])
            .map(object)
    }
}
unsafe fn integer(h: &mut H, i: i64) -> Oop {
    unsafe {
        let v = object(h.integer(i as usize));
        h.keep(v)
    }
}
unsafe fn vec_i(h: &mut H, values: &[i64]) -> Oop {
    unsafe {
        let values: Vec<_> = values.iter().map(|i| integer(h, *i)).collect();
        h.vector(&values)
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        let nil = h.named(b"nil");
        let baseline_owner = allocate(h.id, behavior(h.id), 0);
        h.keep(baseline_owner);
        let answer = allocate(h.id, behavior(h.id), 0);
        h.keep(answer);
        let baseline = h.method("minimal-return", 1, baseline_owner);
        let answer_owner = h.vector(&[answer]);
        let direct = inline(h, baseline, answer_owner, 0);
        assert_eq!(invoke_on_machine(h, direct).unwrap(), answer);
        let fallback = inline(h, baseline, answer_owner, 1);
        assert_eq!(invoke_on_machine(h, fallback).unwrap(), baseline_owner);
        let recipe = h.vector(&[nil, answer]);
        let recipe_owner = h.vector(&[recipe]);
        let recovered = inline(h, baseline, recipe_owner, 2);
        assert_eq!(invoke_on_machine(h, recovered).unwrap(), answer);
        let idle = h.machine();
        assert!(
            h.id.send_named(
                idle,
                b"recoverAt:with:",
                &[h.integer(0), Operand::object(recipe)]
            )
            .is_err()
        );

        // Graph preservation cases from the retained recovery/graph tests: aliases,
        // distinct equal boxes, exact NaN bits, and nested aggregates.
        let object_behavior = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(h.named(b"Object"))],
        ));
        let vt = object(send(h.id, object_behavior, b"delegated", &[]));
        h.keep(vt);
        send(
            h.id,
            h.named(b"ObjectModel"),
            b"setLayoutOf:to:rest:",
            &[Operand::object(vt), Operand::bytes(&[]), Operand::word(1)],
        );
        let input = integer(h, 71);
        let inputs = h.vector(&[vt, input]);
        let node0 = vec_i(h, &[0, i64::MIN]);
        let node1 = vec_i(h, &[0, i64::MIN]);
        let bits = 0xfff8_0000_0000_0042usize;
        let float = object(send(
            h.id,
            h.named(b"Float"),
            b"fromBits:",
            &[Operand::word(bits)],
        ));
        h.keep(float);
        let one = integer(h, 1);
        let node2 = h.vector(&[one, float]);
        let fields = vec_i(h, &[1, -1, -1, -2, -3]);
        let two = integer(h, 2);
        let zero = integer(h, 0);
        let node3 = h.vector(&[two, zero, fields]);
        let outer_fields = vec_i(h, &[-4, -4]);
        let node4 = h.vector(&[two, zero, outer_fields]);
        let nodes = h.vector(&[node0, node1, node2, node3, node4]);
        let slots = vec_i(h, &[-5, -5]);
        let graph = h.vector(&[inputs, nodes, slots]);
        let graph_owner = h.vector(&[graph]);
        let graph_method = inline(h, baseline, graph_owner, 3);
        let integer_object = h.named(b"Integer");
        let integer_behavior = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer_object)],
        ));
        let original = selected(h.id, integer_object, b"fromWord:");
        let factory_context = Box::leak(Box::new(CollectingFactory {
            id: h.id,
            roots: h.roots,
            gc: h.gc,
            model: h.named(b"ObjectModel"),
            original,
            layout: vt,
            mode: Cell::new(0),
            calls: Cell::new(0),
            fresh: Cell::new(ptr::null_mut()),
        }));
        install(
            h.id,
            integer_behavior,
            b"fromWord:",
            factory,
            ptr::from_mut(factory_context).cast(),
        );
        let value = invoke_on_machine(h, graph_method).unwrap();
        h.keep(value);
        collect(h.id, h.gc);
        let read = |value: Oop, i: usize| {
            object(send(
                h.id,
                h.named(b"ObjectModel"),
                b"slotAt:in:",
                &[Operand::word(i), Operand::object(value)],
            ))
        };
        let inner = read(value, 0);
        assert_eq!(inner, read(value, 1));
        assert_eq!(read(inner, 0), input);
        assert_eq!(read(inner, 1), read(inner, 2));
        assert_ne!(read(inner, 1), read(inner, 3));
        assert_eq!(
            send(
                h.id,
                h.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(read(inner, 1))]
            )
            .as_word(),
            Some(i64::MIN as usize)
        );
        assert_eq!(
            send(
                h.id,
                h.named(b"Float"),
                b"bitsOf:",
                &[Operand::object(read(inner, 4))]
            )
            .as_word(),
            Some(bits)
        );
        assert_eq!(factory_context.calls.get(), 2);
        factory_context.mode.set(1);
        factory_context.calls.set(0);
        assert!(invoke_on_machine(h, graph_method).is_err());
        assert_eq!(factory_context.calls.get(), 1);
        replace(h.id, integer_behavior, b"fromWord:", original);
        send(
            h.id,
            h.named(b"ObjectModel"),
            b"setLayoutOf:to:rest:",
            &[Operand::object(vt), Operand::bytes(&[]), Operand::word(1)],
        );

        // Restore the layout before collecting: earlier successful aggregates
        // still contain references under this same behavior.
        collect(h.id, h.gc);
        assert!(
            !present(h.id, factory_context.fresh.get()),
            "failed graph construction releases its temporary nodes"
        );

        // A live Machine control replacement prevents entry into the candidate.
        let guarded_machine = h.machine();
        let machine_behavior = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(guarded_machine)],
        ));
        let view = selected(h.id, guarded_machine, b"currentFrameView");
        install(
            h.id,
            machine_behavior,
            b"currentFrameView",
            view_override,
            ptr::null_mut(),
        );
        assert_eq!(invoke_on_machine(h, direct).unwrap(), baseline_owner);
        replace(h.id, machine_behavior, b"currentFrameView", view);
        assert_eq!(invoke_on_machine(h, direct).unwrap(), answer);
        // A custom lookup that delegates exactly is still observable. Id's
        // owner-local query must reject it without guessing at its context.
        let original_meta = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(machine_behavior)],
        ));
        let lookup = selected(h.id, machine_behavior, b"lookup");
        let meta = behavior(h.id);
        h.keep(meta);
        let lookup_context = Box::leak(Box::new(LookupForward {
            entry: lookup,
            calls: Cell::new(0),
        }));
        install(
            h.id,
            meta,
            b"lookup",
            lookup_forward,
            ptr::from_mut(lookup_context).cast(),
        );
        send(
            h.id,
            h.id.receiver,
            b"reassociateBehaviorOf:with:",
            &[Operand::object(machine_behavior), Operand::object(meta)],
        );
        assert_eq!(
            send(
                h.id,
                h.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(guarded_machine)]
            )
            .as_word(),
            Some(0)
        );
        assert_eq!(invoke_on_machine(h, direct).unwrap(), baseline_owner);
        assert!(lookup_context.calls.get() > 0);
        send(
            h.id,
            h.id.receiver,
            b"reassociateBehaviorOf:with:",
            &[
                Operand::object(machine_behavior),
                Operand::object(original_meta),
            ],
        );
        assert_eq!(
            send(
                h.id,
                h.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(guarded_machine)]
            )
            .as_word(),
            Some(1)
        );
        assert_eq!(invoke_on_machine(h, direct).unwrap(), answer);

        collect(h.id, h.gc);
        // Revalidate the actual inherited reference owner, not just an Oop
        // layout. Even a forwarding replacement must refuse materialization.
        let model = h.named(b"ObjectModel");
        assert_eq!(
            send(h.id, h.gc, b"referenceOwnerOf:", &[Operand::object(vt)]).as_object(),
            Some(model)
        );
        let trace_behavior = behavior(h.id);
        h.keep(trace_behavior);
        let trace_context = Box::leak(Box::new(TraceForward { id: h.id, model }));
        install(
            h.id,
            trace_behavior,
            b"referencesOf:do:",
            trace_forward,
            ptr::from_mut(trace_context).cast(),
        );
        let trace_owner = allocate(h.id, trace_behavior, 0);
        h.keep(trace_owner);
        send(
            h.id,
            h.gc,
            b"trace:with:",
            &[Operand::object(vt), Operand::object(trace_owner)],
        );
        assert_eq!(
            send(h.id, h.gc, b"referenceOwnerOf:", &[Operand::object(vt)]).as_object(),
            Some(trace_owner)
        );
        assert!(invoke_on_machine(h, graph_method).is_err());
        send(
            h.id,
            h.gc,
            b"trace:with:",
            &[Operand::object(vt), Operand::unit()],
        );
        assert_eq!(
            send(h.id, h.gc, b"referenceOwnerOf:", &[Operand::object(vt)]).as_object(),
            Some(model)
        );
        assert!(invoke_on_machine(h, graph_method).is_ok());

        collect(h.id, h.gc);
        // Recipe callbacks collect and may recursively consume the same request.
        // The outer read must reject without publishing a second frame.
        let rv = object(send(h.id, object_behavior, b"delegated", &[]));
        h.keep(rv);
        send(
            h.id,
            h.named(b"ObjectModel"),
            b"setLayoutOf:to:rest:",
            &[Operand::object(rv), Operand::bytes(&[]), Operand::word(1)],
        );
        let rp = object(send(
            h.id,
            h.named(b"ObjectModel"),
            b"allocate:bytes:",
            &[Operand::object(rv), Operand::word(16)],
        ));
        h.keep(rp);
        for (i, value) in [nil, answer].iter().enumerate() {
            send(
                h.id,
                h.named(b"ObjectModel"),
                b"slotAt:in:put:",
                &[
                    Operand::word(i),
                    Operand::object(rp),
                    Operand::object(*value),
                ],
            );
        }
        let rc = Box::leak(Box::new(Recipe {
            id: h.id,
            roots: h.roots,
            gc: h.gc,
            integer: integer_object,
            model: h.named(b"ObjectModel"),
            inner: recipe,
            machine: Cell::new(ptr::null_mut()),
            mode: Cell::new(0),
            calls: Cell::new(0),
        }));
        install(h.id, rv, b"size", recipe_size, ptr::from_mut(rc).cast());
        install(h.id, rv, b"at:", recipe_at, ptr::from_mut(rc).cast());
        let rp_owner = h.vector(&[rp]);
        let rp_method = inline(h, baseline, rp_owner, 2);
        assert_eq!(invoke_on_machine(h, rp_method).unwrap(), answer);
        let reentrant_machine = h.machine();
        rc.machine.set(reentrant_machine);
        rc.mode.set(1);
        let rb = h.block(rp_method, &[]);
        assert!(
            h.id.send_named(reentrant_machine, b"runBlock:", &[Operand::object(rb)])
                .is_err()
        );
        assert_eq!(rc.calls.get(), 1);
        assert_eq!(
            h.value(send(h.id, reentrant_machine, b"liveFrameCount", &[])),
            2
        );
        send(h.id, reentrant_machine, b"destroy", &[]);

        // Empty aggregates cannot forge native raw carriers; custom trace owners
        // cannot be treated as ObjectModel field storage, even with an Oop layout.
        let integer_vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(input)],
        ));
        let bad_inputs = h.vector(&[integer_vt]);
        let empty = h.vector(&[]);
        let bad_node = h.vector(&[two, zero, empty]);
        let bad_nodes = h.vector(&[bad_node]);
        let bad_slots = vec_i(h, &[-1, -1]);
        let bad_graph = h.vector(&[bad_inputs, bad_nodes, bad_slots]);
        let bad_owner = h.vector(&[bad_graph]);
        let bad = inline(h, baseline, bad_owner, 3);
        assert!(invoke_on_machine(h, bad).is_err());
        let bad_length = h.vector(&[answer]);
        let bad_owner = h.vector(&[bad_length]);
        let bad = inline(h, baseline, bad_owner, 2);
        assert!(invoke_on_machine(h, bad).is_err());
        lifetimes(h);
    }
}

struct CollectingFactory {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    model: Oop,
    original: NativeEntry,
    layout: Oop,
    mode: Cell<usize>,
    calls: Cell<usize>,
    fresh: Cell<Oop>,
}
unsafe extern "C" fn factory(c: *mut c_void, r: Oop, args: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<CollectingFactory>();
        let result = c.original.method.unwrap()(c.original.context, r, args, n);
        if result.status == 0
            && borrowed_slice(args, n)
                .and_then(|a| a.first())
                .and_then(|a| a.as_word())
                == Some(i64::MIN as usize)
        {
            let value = object(result.value);
            c.fresh.set(value);
            c.calls.set(c.calls.get() + 1);
            let token = retain(c.id, c.roots, value);
            collect(c.id, c.gc);
            if c.mode.get() == 1 {
                send(
                    c.id,
                    c.model,
                    b"setLayoutOf:to:rest:",
                    &[
                        Operand::object(c.layout),
                        Operand::bytes(&[]),
                        Operand::word(2),
                    ],
                );
            }
            release(c.id, c.roots, token);
        }
        result
    }
}
unsafe fn replace(id: Protocol, behavior: Oop, name: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            id,
            behavior,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
unsafe fn selected(id: Protocol, receiver: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::object(receiver),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
struct Recipe {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    integer: Oop,
    model: Oop,
    inner: Oop,
    machine: Cell<Oop>,
    mode: Cell<usize>,
    calls: Cell<usize>,
}
unsafe extern "C" fn recipe_size(c: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Recipe>();
        collect(c.id, c.gc);
        if c.mode.replace(0) == 1 {
            let pc = send(c.id, c.integer, b"fromWord:", &[Operand::word(0)]);
            let token = retain(c.id, c.roots, object(pc));
            send(
                c.id,
                c.machine.get(),
                b"recoverAt:with:",
                &[pc, Operand::object(c.inner)],
            );
            release(c.id, c.roots, token);
            c.calls.set(c.calls.get() + 1);
        }
        let _ = r;
        Outcome::loaded(send(c.id, c.integer, b"fromWord:", &[Operand::word(2)]))
    }
}
unsafe extern "C" fn recipe_at(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Recipe>();
        let args = borrowed_slice(a, n).unwrap();
        let i = send(c.id, c.integer, b"wordOf:", args).as_word().unwrap();
        collect(c.id, c.gc);
        Outcome::loaded(send(
            c.id,
            c.model,
            b"slotAt:in:",
            &[Operand::word(i), Operand::object(r)],
        ))
    }
}
unsafe extern "C" fn view_override(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}

// Drop every temporary root and replace the only installed binding. Recovery
// retains the baseline through its frame; the finished candidate has no pin.
unsafe fn lifetimes(h: &mut H) {
    unsafe {
        for abandon in [false, true] {
            let start = h.tokens.len();
            let baseline_owner = allocate(h.id, behavior(h.id), 0);
            h.keep(baseline_owner);
            let baseline = h.method("minimal-return", 1, baseline_owner);
            let result = h.named(b"true");
            let nil = h.named(b"nil");
            let recipe = h.vector(&[nil, result]);
            let owner = h.vector(&[recipe]);
            let candidate = inline(h, baseline, owner, 2);
            let vt = behavior(h.id);
            h.keep(vt);
            let receiver = allocate(h.id, vt, 0);
            h.keep(receiver);
            let selector = h.id.intern_name(b"value").unwrap();
            h.keep(selector);
            h.bind(vt, selector, candidate);
            let replacement = h.method("minimal-return", 1, nil);
            let machine = h.machine();
            let root = retain(h.id, h.roots, machine);
            send(
                h.id,
                machine,
                b"runBlock:budget:",
                &[Operand::object(receiver), h.integer(1)],
            );
            assert_eq!(h.value(send(h.id, machine, b"liveFrameCount", &[])), 2);
            h.bind(vt, selector, replacement);
            for token in h.tokens.drain(start..) {
                release(h.id, h.roots, token);
            }
            collect(h.id, h.gc);
            assert!(present(h.id, baseline_owner));
            assert!(present(h.id, baseline));
            assert!(
                !present(h.id, owner),
                "published recovery no longer needs candidate context"
            );
            assert!(!present(h.id, candidate));
            if abandon {
                send(h.id, machine, b"destroy", &[]);
            } else {
                assert_eq!(
                    object(send(
                        h.id,
                        machine,
                        b"continueWithBudget:",
                        &[h.integer(10)]
                    )),
                    result
                );
            }
            collect(h.id, h.gc);
            assert!(!present(h.id, baseline_owner));
            assert!(!present(h.id, baseline));
            send(h.id, h.named(b"Execution"), b"checkpoint", &[]);
            release(h.id, h.roots, root);
            collect(h.id, h.gc);
        }
        // Failed admission owns no frame pin or acquired-entry lease after destroy.
        let start = h.tokens.len();
        let owner = allocate(h.id, behavior(h.id), 0);
        h.keep(owner);
        let baseline = h.method("minimal-return", 1, owner);
        let invalid = h.vector(&[]);
        let recipe_owner = h.vector(&[invalid]);
        let candidate = inline(h, baseline, recipe_owner, 2);
        let block = h.block(candidate, &[]);
        let machine = h.machine();
        assert!(
            h.id.send_named(machine, b"runBlock:", &[Operand::object(block)])
                .is_err()
        );
        send(h.id, machine, b"destroy", &[]);
        for token in h.tokens.drain(start..) {
            release(h.id, h.roots, token);
        }
        collect(h.id, h.gc);
        send(h.id, h.named(b"Execution"), b"checkpoint", &[]);
        assert!(!present(h.id, owner));
        assert!(!present(h.id, baseline));
        assert!(!present(h.id, recipe_owner));
        assert!(!present(h.id, candidate));
    }
}

struct LookupForward {
    entry: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn lookup_forward(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<LookupForward>();
        c.calls.set(c.calls.get() + 1);
        c.entry.method.unwrap()(c.entry.context, r, a, n)
    }
}
struct TraceForward {
    id: Protocol,
    model: Oop,
}
unsafe extern "C" fn trace_forward(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<TraceForward>();
        c.id.send_named(c.model, b"referencesOf:do:", borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
