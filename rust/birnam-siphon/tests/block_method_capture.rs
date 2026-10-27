//! Captured Block methods retain their body while ordinary value sends remain live.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
fn eval(h: &Context, q: &Scope<'_>, source: &[u8]) -> Oop {
    let result = q
        .call(
            h.named(b"Codegen").unwrap(),
            b"eval:",
            &[q.string(source).unwrap()],
        )
        .unwrap();
    q.check_result(result).unwrap();
    result
}
#[test]
#[ignore = "requires production Oop modules"]
fn captured_block_body_survives_rebinding_collection_and_other_receivers() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let cg = named(b"Codegen");
    let gc = named(b"GC");
    let tcc = named(b"TCC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let number = |v| as_size(call(v, b"word", &[])).unwrap();
    let int = |v| call(named(b"Integer"), b"fromWord:", &[size(v).unwrap()]);
    let vector = |v| call(call(named(b"Vector"), b"new", &[]), b"add:", &[v]);
    let collect = || {
        call(gc, b"collectPrecisely", &[]);
        call(tcc, b"checkpoint", &[]);
    };
    let units = || as_size(call(tcc, b"liveNativeUnits", &[])).unwrap();
    let receiver = eval(&h, &q, b"[:argument | 19]");
    let argument = int(73);
    let args = vector(argument);
    let receiver_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[receiver]);
    let value = key(b"value:");
    let alias = key(b"retainedBody:");
    call(cg, b"evictMaterializations", &[]);
    collect();
    let baseline = units();
    for mode in ["source", "synchronous", "resumable"] {
        let retained = h.scope();
        let method = {
            let source_roots = h.scope();
            let source = eval(&h, &source_roots, b"[:argument | argument]");
            let block = if mode == "source" {
                source
            } else {
                let code = source_roots.call(source, b"codeObject", &[]).unwrap();
                let captures = source_roots.call(named(b"Vector"), b"new", &[]).unwrap();
                source_roots
                    .call(
                        cg,
                        if mode == "synchronous" {
                            b"materializeSynchronous:captures:"
                        } else {
                            b"materializeResumable:captures:"
                        },
                        &[code, captures],
                    )
                    .unwrap()
            };
            source_roots.check_result(block).unwrap();
            let method = source_roots.call(block, b"methodFor:", &[value]).unwrap();
            retained.keep(method).unwrap();
            assert_eq!(
                retained
                    .call(method, b"invokeOn:arguments:", &[receiver, args])
                    .unwrap(),
                argument,
                "captured {mode} body must not switch to the receiver's body"
            );
            method
        };
        call(cg, b"evictMaterializations", &[]);
        collect();
        assert_eq!(
            units(),
            baseline + 1,
            "{mode} capture owns code after source Block release"
        );
        assert_eq!(
            retained
                .call(method, b"invokeOn:arguments:", &[receiver, args])
                .unwrap(),
            argument
        );
        h.send(receiver_vt, b"addMethod:as:", &[method, alias])
            .unwrap();
        drop(retained);
        collect();
        assert_eq!(units(), baseline + 1);
        assert_eq!(call(receiver, b"retainedBody:", &[argument]), argument);
        assert_eq!(number(call(receiver, b"value:", &[argument])), 19);
        h.send(receiver_vt, b"removeMethod:", &[alias]).unwrap();
        collect();
        assert_eq!(units(), baseline);
    }
    let block = eval(&h, &q, b"[:argument | argument]");
    let captured = call(block, b"methodFor:", &[value]);
    let block_vt = named(b"BlockVt");
    let default_value = call(block_vt, b"lookup", &[value]);
    let sentinel = call(named(b"Object"), b"clone", &[]);
    let count = hooks.context(&[sentinel, size(0).unwrap()]);
    call(
        block_vt,
        b"addMethod",
        &[value, hooks.method(b"fixedCount", count)],
    );
    let overridden = call(block, b"methodFor:", &[value]);
    assert_eq!(
        hooks.count(count, 1),
        0,
        "reflection must not execute the selected handler"
    );
    assert_eq!(
        call(overridden, b"invokeOn:arguments:", &[receiver, args]),
        sentinel
    );
    assert_eq!(call(block, b"value:", &[argument]), sentinel);
    assert_eq!(hooks.count(count, 1), 2);
    assert_eq!(
        call(captured, b"invokeOn:arguments:", &[receiver, args]),
        argument
    );
    call(block_vt, b"addMethod", &[value, default_value]);
    // A current custom lookup still participates in reflection.
    let private = call(h.id.root, b"ensurePrivateVtableFor:", &[block]);
    let meta = call(h.id.root, b"behaviorOf:", &[private]);
    let custom_meta = call(meta, b"delegated", &[]);
    let lookup = call(meta, b"lookup", &[key(b"lookup")]);
    let lookup_alias = key(b"captureOriginalLookup");
    call(custom_meta, b"addMethod", &[lookup_alias, lookup]);
    let observed = hooks.context(&[lookup_alias, size(0).unwrap()]);
    call(
        custom_meta,
        b"addMethod",
        &[key(b"lookup"), hooks.method(b"lookupForward", observed)],
    );
    call(
        h.id.root,
        b"reassociateBehaviorOf:with:",
        &[private, custom_meta],
    );
    let current = call(block, b"methodFor:", &[value]);
    assert!(hooks.count(observed, 1) >= 2);
    assert_eq!(
        call(current, b"invokeOn:arguments:", &[receiver, args]),
        argument
    );
    call(h.id.root, b"reassociateBehaviorOf:with:", &[private, meta]);
    // A foreign receiver with the same handler must not be read as Block storage.
    let foreign = call(named(b"Object"), b"clone", &[]);
    let foreign_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[foreign]);
    call(foreign_vt, b"addMethod", &[value, default_value]);
    let plain = call(foreign, b"methodFor:", &[value]);
    assert_eq!(
        as_size(call(
            h.id.root,
            b"sameNativeMethod:as:",
            &[plain, default_value]
        )),
        Some(1)
    );
    // Keep methodFor: itself replaceable through the ordinary inherited binding.
    let base = named(b"ObjectVt");
    let method_for = call(base, b"lookup", &[key(b"methodFor:")]);
    call(
        base,
        b"addMethod",
        &[key(b"methodFor:"), hooks.method(b"fixed", sentinel)],
    );
    assert_eq!(call(block, b"methodFor:", &[value]), sentinel);
    call(base, b"addMethod", &[key(b"methodFor:"), method_for]);
}
