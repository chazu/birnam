//! Actual pending-call recovery, not just rejection outside a call.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_artifact_codec::stackless_selection::{
    self, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use birnam_host_support::{Context, Scope};
use std::path::Path;

fn vector(h: &Context, q: &Scope<'_>, values: &[Oop]) -> Oop {
    let mut v = q.call(h.named(b"Vector").unwrap(), b"new", &[]).unwrap();
    for &value in values {
        v = q.call(v, b"add:", &[value]).unwrap();
    }
    v
}
fn integer(h: &Context, q: &Scope<'_>, value: i64) -> Oop {
    let word = q.word(value as usize).unwrap();
    q.call(h.named(b"Integer").unwrap(), b"fromWord:", &[word])
        .unwrap()
}
fn integers(h: &Context, q: &Scope<'_>, values: &[i64]) -> Oop {
    vector(
        h,
        q,
        &values.iter().map(|&v| integer(h, q, v)).collect::<Vec<_>>(),
    )
}
fn method(h: &Context, q: &Scope<'_>, module: Oop, name: &[u8], context: Oop) -> Oop {
    let method = q
        .call(module, b"methodAt:", &[h.intern(name).unwrap()])
        .unwrap();
    q.call(method, b"withContext:", &[context]).unwrap()
}
fn count(q: &Scope<'_>, context: Oop, index: usize) -> usize {
    as_size(q.call(context, b"at:", &[size(index).unwrap()]).unwrap()).unwrap()
}
fn run(h: &Context, q: &Scope<'_>, method: Oop) -> Result<Oop, birnam_host_support::HostError> {
    let captures = vector(h, q, &[h.named(b"nil").unwrap()]);
    let block = q.call(
        h.named(b"Block").unwrap(),
        b"fromMethod:captures:",
        &[method, captures],
    )?;
    let machine = q.call(h.named(b"Machine").unwrap(), b"new", &[])?;
    q.call(machine, b"runBlock:", &[block])
}

#[test]
#[ignore = "requires production Oop modules"]
fn candidates_return_decline_and_recover_without_replaying_effects() {
    let h = oop::bootstrap();
    let q = h.scope();
    let gc = h.named(b"GC").unwrap();
    let nil = h.named(b"nil").unwrap();
    q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let path = std::env::temp_dir().join(format!("birnam-recovery-{}.dylib", std::process::id()));
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/oop-host/recovery.c");
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(source)
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let hooks = oop::hooks::Hooks::new(&h);
    let steps = q
        .call(h.id.root, b"sequence", &[size(0).unwrap(), gc])
        .unwrap();
    let step = method(&h, &q, module, b"step", steps);
    let description = include_bytes!(
        "../../../tests/fixtures/stackless-s2/frame-descriptor-source-map-block.bin"
    );
    let revision = std::array::from_fn(|i| i as u8);
    let spec = SelectionSpec {
        source_revision: revision,
        root_kind: RootKind::MethodOperation,
        root_index: 0,
        path: &[2, 1],
        implementation_kind: ImplementationKind::BlockBody,
        disposition: Disposition::ResumableRequired,
    };
    let selection = stackless_selection::encode_artifact_selection(&[revision], &[spec]).unwrap();
    let baseline = q
        .call(
            h.named(b"Execution").unwrap(),
            b"fromStep:descriptor:selection:owner:",
            &[
                step,
                q.string(description).unwrap(),
                q.string(&selection).unwrap(),
                module,
            ],
        )
        .unwrap();
    let baseline = q.call(baseline, b"captured", &[]).unwrap();
    let answer = q.call(h.named(b"Object").unwrap(), b"clone", &[]).unwrap();
    let recipe = vector(&h, &q, &[nil, answer]);
    let source = q.string(b"[| [System gc]; 37]").unwrap();
    let nested = q
        .call(h.named(b"Codegen").unwrap(), b"eval:", &[source])
        .unwrap();
    q.check_result(nested).unwrap();
    let make = |mode: usize, data: Oop| {
        let context = q
            .call(
                h.id.root,
                b"sequence",
                &[size(mode).unwrap(), data, gc, size(0).unwrap(), nested, nil],
            )
            .unwrap();
        let candidate = method(&h, &q, module, b"candidate", context);
        let value = q
            .call(
                baseline,
                b"withInline:at:owner:",
                &[candidate, integer(&h, &q, 1), context],
            )
            .unwrap();
        (value, context)
    };
    for mode in [0, 1, 2, 5, 6] {
        let (candidate, context) = make(mode, if mode == 0 { answer } else { recipe });
        let before = count(&q, steps, 0);
        let result = run(&h, &q, candidate).unwrap_or_else(|e| {
            panic!(
                "mode {mode}: calls={}, steps={}, {e}",
                count(&q, context, 3),
                count(&q, steps, 0)
            )
        });
        assert_eq!(result, if mode == 1 { nil } else { answer }, "mode {mode}");
        assert_eq!(count(&q, context, 3), 1, "candidate effect replayed");
        assert_eq!(count(&q, steps, 0) - before, usize::from(mode != 0));
        if mode == 5 {
            let observed = q.call(context, b"at:", &[size(5).unwrap()]).unwrap();
            assert_eq!(
                q.word_bits(
                    q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[observed])
                        .unwrap()
                )
                .unwrap(),
                37
            );
        }
    }
    // A native callback cannot park. The failed drive also invalidates the
    // pending call, so a subsequent recovery attempt cannot publish into it.
    let (barrier, context) = make(7, recipe);
    let before = count(&q, steps, 0);
    assert!(run(&h, &q, barrier).is_err());
    assert_eq!(count(&q, context, 3), 1);
    assert_eq!(count(&q, steps, 0), before);
    let observed = q.call(context, b"at:", &[size(5).unwrap()]).unwrap();
    assert_eq!(
        q.call(observed, b"faultKind", &[]).unwrap(),
        h.intern(b"continuationBarrier").unwrap()
    );
    let idle = q.call(h.named(b"Machine").unwrap(), b"new", &[]).unwrap();
    for name in [b"recoverAt:with:".as_slice(), b"recoverAt:withGraph:"] {
        assert!(q.call(idle, name, &[size(0).unwrap(), recipe]).is_err());
    }
    let (bad, _) = make(2, vector(&h, &q, &[answer]));
    let before = count(&q, steps, 0);
    let error = run(&h, &q, bad).unwrap();
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        h.intern(b"nativeInvalidRequest").unwrap()
    );
    assert_eq!(count(&q, steps, 0), before);
    let (bad, _) = make(4, recipe);
    assert!(run(&h, &q, bad).is_err());
    // Changed Machine controls withdraw candidate entry, preserving ordinary execution.
    let machine_vt = h.named(b"MachineVt").unwrap();
    let selector = h.intern(b"currentFrameView").unwrap();
    let original = q.call(machine_vt, b"lookup", &[selector]).unwrap();
    let replacement = method(&h, &q, module, b"fixed", nil);
    q.call(machine_vt, b"addMethod", &[selector, replacement])
        .unwrap();
    let (direct, context) = make(0, answer);
    assert_eq!(run(&h, &q, direct).unwrap(), nil);
    assert_eq!(count(&q, context, 3), 0);
    q.call(machine_vt, b"addMethod", &[selector, original])
        .unwrap();
    assert_eq!(run(&h, &q, direct).unwrap(), answer);
    // An exactly forwarding custom lookup remains observable and also
    // withdraws candidate entry.
    let lookup = h.intern(b"lookup").unwrap();
    let meta = q.call(h.id.root, b"behaviorOf:", &[machine_vt]).unwrap();
    let original_lookup = q.call(meta, b"lookup", &[lookup]).unwrap();
    let custom = q.call(meta, b"delegated", &[]).unwrap();
    let alias = h.intern(b"recoveryOriginalLookup").unwrap();
    q.call(custom, b"addMethod", &[alias, original_lookup])
        .unwrap();
    let lookup_state = hooks.context(&[alias, size(0).unwrap()]);
    q.call(
        custom,
        b"addMethod",
        &[lookup, hooks.method(b"lookupForward", lookup_state)],
    )
    .unwrap();
    q.call(
        h.id.root,
        b"reassociateBehaviorOf:with:",
        &[machine_vt, custom],
    )
    .unwrap();
    assert_eq!(
        as_size(q.call(h.id.root, b"usesDefaultLookup:", &[idle]).unwrap()),
        Some(0)
    );
    let before = count(&q, context, 3);
    assert_eq!(run(&h, &q, direct).unwrap(), nil);
    assert_eq!(count(&q, context, 3), before);
    assert!(hooks.count(lookup_state, 1) > 0);
    q.call(
        h.id.root,
        b"reassociateBehaviorOf:with:",
        &[machine_vt, meta],
    )
    .unwrap();
    assert_eq!(
        as_size(q.call(h.id.root, b"usesDefaultLookup:", &[idle]).unwrap()),
        Some(1)
    );
    assert_eq!(run(&h, &q, direct).unwrap(), answer);

    // Only the returned Method keeps the candidate and its private data alive.
    let retained = h.scope();
    let (owned, private_answer, private_state) = {
        let local = h.scope();
        let answer = local
            .call(h.named(b"Object").unwrap(), b"clone", &[])
            .unwrap();
        let state = local
            .call(
                h.id.root,
                b"sequence",
                &[size(0).unwrap(), answer, gc, size(0).unwrap(), nil, nil],
            )
            .unwrap();
        let candidate = method(&h, &local, module, b"candidate", state);
        let owned = local
            .call(
                baseline,
                b"withInline:at:owner:",
                &[candidate, integer(&h, &local, 1), state],
            )
            .unwrap();
        (retained.keep(owned).unwrap(), answer, state)
    };
    q.call(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(run(&h, &q, owned).unwrap(), private_answer);
    assert_eq!(count(&q, private_state, 3), 1);

    // Graph recovery preserves aliases, distinct equal boxes, exact NaN bits,
    // and nested ObjectModel storage, under collection at candidate and step.
    let model = h.named(b"ObjectModel").unwrap();
    let vt = q
        .call(h.named(b"ObjectVt").unwrap(), b"delegated", &[])
        .unwrap();
    let empty = q.call(h.id.root, b"sequence", &[]).unwrap();
    q.call(
        model,
        b"setLayoutOf:to:rest:",
        &[vt, empty, size(1).unwrap()],
    )
    .unwrap();
    let input = integer(&h, &q, 71);
    let inputs = vector(&h, &q, &[vt, input]);
    let bits = 0xfff8_0000_0000_0042usize;
    let float = q
        .call(
            h.named(b"Float").unwrap(),
            b"fromBits:",
            &[q.word(bits).unwrap()],
        )
        .unwrap();
    let nodes = vector(
        &h,
        &q,
        &[
            integers(&h, &q, &[0, i64::MIN]),
            integers(&h, &q, &[0, i64::MIN]),
            vector(&h, &q, &[integer(&h, &q, 1), float]),
            vector(
                &h,
                &q,
                &[
                    integer(&h, &q, 2),
                    integer(&h, &q, 0),
                    integers(&h, &q, &[1, -1, -1, -2, -3]),
                ],
            ),
            vector(
                &h,
                &q,
                &[
                    integer(&h, &q, 2),
                    integer(&h, &q, 0),
                    integers(&h, &q, &[-4, -4]),
                ],
            ),
        ],
    );
    let graph = vector(&h, &q, &[inputs, nodes, integers(&h, &q, &[-5, -5])]);
    // A replaced number factory collects between materialized nodes.
    let factory = h.named(b"Integer").unwrap();
    let factory_vt = q.call(h.id.root, b"behaviorOf:", &[factory]).unwrap();
    let from_word = h.intern(b"fromWord:").unwrap();
    let original_factory = q.call(factory_vt, b"lookup", &[from_word]).unwrap();
    let factory_state = hooks.context(&[original_factory, gc, size(0).unwrap()]);
    q.call(
        factory_vt,
        b"addMethod",
        &[from_word, hooks.method(b"collectEvery", factory_state)],
    )
    .unwrap();
    let (candidate, context) = make(3, graph);
    let value = run(&h, &q, candidate).unwrap();
    q.call(factory_vt, b"addMethod", &[from_word, original_factory])
        .unwrap();
    assert!(hooks.count(factory_state, 2) > 0);
    assert_eq!(count(&q, context, 3), 1);
    q.call(gc, b"collectPrecisely", &[]).unwrap();
    let read = |object, index| {
        q.call(model, b"slotAt:in:", &[size(index).unwrap(), object])
            .unwrap()
    };
    let inner = read(value, 0);
    assert_eq!(inner, read(value, 1));
    assert_eq!(read(inner, 0), input);
    assert_eq!(read(inner, 1), read(inner, 2));
    assert_ne!(read(inner, 1), read(inner, 3));
    assert_eq!(
        q.word_bits(
            q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[read(inner, 1)])
                .unwrap()
        )
        .unwrap(),
        i64::MIN as usize
    );
    assert_eq!(
        q.word_bits(
            q.call(h.named(b"Float").unwrap(), b"bitsOf:", &[read(inner, 4)])
                .unwrap()
        )
        .unwrap(),
        bits
    );
    {
        let send = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
        let key = |n: &[u8]| h.intern(n).unwrap();
        let check_error =
            |value| assert_eq!(send(value, b"kind", &[]), key(b"nativeInvalidRequest"));
        // A constructor may collect and then change the admitted storage layout.
        let alias = key(b"recoveryOriginalInteger:");
        send(factory_vt, b"addMethod", &[alias, original_factory]);
        let state = send(
            h.id.root,
            b"sequence",
            &[
                alias,
                h.roots,
                gc,
                model,
                vt,
                empty,
                size(0).unwrap(),
                size(0).unwrap(),
                size(0).unwrap(),
            ],
        );
        send(
            factory_vt,
            b"addMethod",
            &[from_word, method(&h, &q, module, b"factory", state)],
        );
        q.check_result(run(&h, &q, candidate).unwrap()).unwrap();
        assert_eq!(count(&q, state, 7), 2);
        send(state, b"at:put:", &[size(6).unwrap(), size(1).unwrap()]);
        send(state, b"at:put:", &[size(7).unwrap(), size(0).unwrap()]);
        check_error(run(&h, &q, candidate).unwrap());
        assert_eq!(count(&q, state, 7), 1);
        let abandoned = send(state, b"at:", &[size(8).unwrap()]);
        send(factory_vt, b"addMethod", &[from_word, original_factory]);
        send(factory_vt, b"removeMethod:", &[alias]);
        send(
            model,
            b"setLayoutOf:to:rest:",
            &[vt, empty, size(1).unwrap()],
        );
        let monitor = send(h.named(b"Object").unwrap(), b"clone", &[]);
        let monitor_vt = send(h.id.root, b"ensurePrivateVtableFor:", &[monitor]);
        for name in [b"allocation:bytes:identity:".as_slice(), b"present"] {
            send(
                monitor_vt,
                b"addMethod",
                &[
                    key(name),
                    method(&h, &q, module, name, std::ptr::null_mut()),
                ],
            );
        }
        send(gc, b"collectPrecisely", &[]);
        assert_eq!(as_size(send(monitor, b"present", &[abandoned])), Some(0));

        // Exactly forwarding tracing still does not grant ObjectModel ownership.
        let tracing = send(h.named(b"Object").unwrap(), b"clone", &[]);
        let tracing_vt = send(h.id.root, b"ensurePrivateVtableFor:", &[tracing]);
        send(
            tracing_vt,
            b"addMethod",
            &[
                key(b"referencesOf:do:"),
                method(&h, &q, module, b"traceForward", model),
            ],
        );
        send(gc, b"trace:with:", &[vt, tracing]);
        assert_eq!(send(gc, b"referenceOwnerOf:", &[vt]), tracing);
        check_error(run(&h, &q, candidate).unwrap());
        send(gc, b"trace:with:", &[vt, std::ptr::null_mut()]);
        assert_eq!(send(gc, b"referenceOwnerOf:", &[vt]), model);
        q.check_result(run(&h, &q, candidate).unwrap()).unwrap();

        // Recipe reads may collect or consume the same request recursively.
        let recipe_vt = send(h.named(b"ObjectVt").unwrap(), b"delegated", &[]);
        send(
            model,
            b"setLayoutOf:to:rest:",
            &[recipe_vt, empty, size(1).unwrap()],
        );
        let dynamic = send(model, b"allocate:bytes:", &[recipe_vt, size(16).unwrap()]);
        for (i, value) in [nil, answer].into_iter().enumerate() {
            send(
                model,
                b"slotAt:in:put:",
                &[size(i).unwrap(), dynamic, value],
            );
        }
        let machine = send(h.named(b"Machine").unwrap(), b"new", &[]);
        let state = send(
            h.id.root,
            b"sequence",
            &[
                gc,
                factory,
                model,
                recipe,
                machine,
                size(0).unwrap(),
                size(0).unwrap(),
            ],
        );
        for (selector, name) in [
            (b"size".as_slice(), b"recipeSize".as_slice()),
            (b"at:", b"recipeAt"),
        ] {
            send(
                recipe_vt,
                b"addMethod",
                &[key(selector), method(&h, &q, module, name, state)],
            );
        }
        let (dynamic_method, _) = make(2, dynamic);
        assert_eq!(run(&h, &q, dynamic_method).unwrap(), answer);
        send(state, b"at:put:", &[size(5).unwrap(), size(1).unwrap()]);
        let block = send(
            h.named(b"Block").unwrap(),
            b"fromMethod:captures:",
            &[dynamic_method, vector(&h, &q, &[nil])],
        );
        assert!(q.call(machine, b"runBlock:", &[block]).is_err());
        assert_eq!(count(&q, state, 6), 1);
        let frames = send(machine, b"liveFrameCount", &[]);
        assert_eq!(
            q.word_bits(send(factory, b"wordOf:", &[frames])).unwrap(),
            2
        );
        send(machine, b"destroy", &[]);

        // Empty aggregate fields cannot forge a native numeric representation.
        let scalar_graph = vector(
            &h,
            &q,
            &[
                vector(&h, &q, &[h.named(b"IntegerVt").unwrap()]),
                vector(
                    &h,
                    &q,
                    &[vector(
                        &h,
                        &q,
                        &[integer(&h, &q, 2), integer(&h, &q, 0), vector(&h, &q, &[])],
                    )],
                ),
                integers(&h, &q, &[-1, -1]),
            ],
        );
        let (bad, _) = make(3, scalar_graph);
        check_error(run(&h, &q, bad).unwrap());
    }
    let invalid = vector(&h, &q, &[inputs, nodes, integers(&h, &q, &[-6, -6])]);
    let (bad, _) = make(3, invalid);
    let before = count(&q, steps, 0);
    let error = run(&h, &q, bad).unwrap();
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        h.intern(b"nativeInvalidRequest").unwrap()
    );
    assert_eq!(count(&q, steps, 0), before);
    // Aggregate storage must belong to the declared all-Oop owner. These
    // vtables have no instances, so changing their layout cannot hide live data.
    let rejected_vt = q
        .call(h.named(b"ObjectVt").unwrap(), b"delegated", &[])
        .unwrap();
    let rejected_inputs = vector(&h, &q, &[rejected_vt, input]);
    let rejected_graph = vector(
        &h,
        &q,
        &[rejected_inputs, nodes, integers(&h, &q, &[-5, -5])],
    );
    for trace_mismatch in [false, true] {
        q.call(
            model,
            b"setLayoutOf:to:rest:",
            &[
                rejected_vt,
                empty,
                size(usize::from(trace_mismatch)).unwrap(),
            ],
        )
        .unwrap();
        if trace_mismatch {
            q.call(
                gc,
                b"trace:with:",
                &[rejected_vt, h.named(b"Roots").unwrap()],
            )
            .unwrap();
        }
        let (bad, _) = make(3, rejected_graph);
        let before = count(&q, steps, 0);
        let error = run(&h, &q, bad).unwrap();
        assert_eq!(
            q.call(error, b"kind", &[]).unwrap(),
            h.intern(b"nativeInvalidRequest").unwrap()
        );
        assert_eq!(count(&q, steps, 0), before);
    }
}
