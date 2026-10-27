//! Collection inputs and detached snapshots survive reentry, collection and failure.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
use std::collections::HashMap;
#[test]
#[ignore = "requires production Oop owners and compiler"]
fn inputs_and_map_snapshots_survive_collecting_reentrant_and_failing_reads() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let names: HashMap<&[u8], Oop> = [
        b"at:".as_slice(),
        b"at:put:",
        b"retain:",
        b"release:",
        b"collectPrecisely",
        b"identityOf:",
        b"sequence",
        b"addMethod",
        b"removeMethod:",
        b"lookup",
        b"bind:to:",
        b"new",
        b"add:",
        b"allocate",
        b"behaviorOf:",
        b"slotAt:in:",
        b"value:value:",
        b"fromWord:",
        b"wordOf:",
        b"vtableOf:",
        b"collectionHash",
        b"nameHash",
        b"canOmitAllocationFor:",
        b"collectionOriginalRetain:",
        b"collectionOriginalVectorAt:",
        b"collectionQueryAlias",
        b"present:",
    ]
    .into_iter()
    .map(|n| (n, q.keep(host.intern(n).unwrap()).unwrap()))
    .collect();
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { host.id.send(r, names[n], a) };
    let gc = named(b"GC");
    let roots = host.roots;
    let model = named(b"ObjectModel");
    let vector = named(b"Vector");
    let integer = named(b"Integer");
    let reflect = named(b"Reflect");
    let object_vt = named(b"ObjectVt");
    let word_vt = named(b"WordVt");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let symbol = q.keep(host.intern(b"borrowed-symbol").unwrap()).unwrap();
    let root_vt = call(host.id.root, b"behaviorOf:", &[host.id.root]);
    let roots_vt = call(host.id.root, b"behaviorOf:", &[roots]);
    let vector_vt = named(b"VectorVt");
    let integer_vt = named(b"IntegerVt");
    let symbol_vt = call(host.id.root, b"behaviorOf:", &[symbol]);
    let gc_vt = call(host.id.root, b"behaviorOf:", &[gc]);
    let retainer = call(
        host.id.root,
        b"bind:to:",
        &[names[b"retain:".as_slice()], roots],
    );
    let vector_entry = call(
        host.id.root,
        b"bind:to:",
        &[names[b"at:".as_slice()], vector],
    );
    call(
        roots_vt,
        b"addMethod",
        &[names[b"collectionOriginalRetain:".as_slice()], retainer],
    );
    call(
        vector_vt,
        b"addMethod",
        &[
            names[b"collectionOriginalVectorAt:".as_slice()],
            vector_entry,
        ],
    );
    let lease = |v| raw(roots, b"collectionOriginalRetain:", &[v]).unwrap();
    let release = |token| {
        raw(roots, b"release:", &[token]).unwrap();
    };
    let id_of = |value| raw(host.id.root, b"identityOf:", &[value]).unwrap();
    let path = std::env::temp_dir().join(format!(
        "birnam-collection-inputs-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/collection-inputs.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let fault = |status, detail| {
        let e = unsafe {
            host.id.send(
                named(b"Execution"),
                host.intern(b"failNative:detail:").unwrap(),
                &[size(status).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(e).unwrap()
    };
    let hash_failure = fault(812, 19);
    let read_failure = fault(813, 20);
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let z = size(0).unwrap();
    let one = size(1).unwrap();
    let null = std::ptr::null_mut();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            roots,
            model,
            monitor,
            null,
            z,
            z,
            z,
            z,
            null,
            null,
            one,
            z,
            z,
            z,
            z,
            z,
            z,
            z,
            null,
            null,
            z,
            z,
            names[b"collectionQueryAlias".as_slice()],
            hash_failure,
            read_failure,
        ],
    );
    let put = |i, v| {
        raw(context, b"at:put:", &[size(i).unwrap(), v]).unwrap();
    };
    let get = |i| raw(context, b"at:", &[size(i).unwrap()]).unwrap();
    let set_inputs = |values: &[Oop]| {
        let ids: Vec<_> = values.iter().map(|&v| id_of(v)).collect();
        let list = raw(host.id.root, b"sequence", &ids).unwrap();
        put(4, list);
        put(5, size(values.len()).unwrap());
        put(6, z);
    };
    let set = |vt, n: &[u8], m| {
        raw(vt, b"addMethod", &[names[n], m]).unwrap();
    };
    let method = |n: &[u8]| {
        let name = q.keep(host.intern(n).unwrap()).unwrap();
        call(
            call(fixture, b"methodAt:", &[name]),
            b"withContext:",
            &[context],
        )
    };
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        let name = q.keep(host.intern(n).unwrap()).unwrap();
        call(monitor_vt, b"addMethod", &[name, method(n)]);
    }
    let hash = method(b"hash");
    let query = method(b"query");
    let retain_hook = method(b"retain");
    let vector_hook = method(b"vector");
    let present = |id| raw(monitor, b"present:", &[id]).unwrap() == one;
    let collect = || {
        put(11, one);
        raw(gc, b"collectPrecisely", &[]).unwrap();
        put(11, z);
    };
    let key_vt = call(object_vt, b"delegated", &[]);
    set(key_vt, b"collectionHash", hash);
    let key = raw(key_vt, b"allocate", &[z]).unwrap();
    let kt = lease(key);
    let value = call(named(b"Object"), b"clone", &[]);
    let empty = call(named(b"Map"), b"new", &[]);
    let map = raw(empty, b"at:put:", &[key, value]).unwrap();
    let mt = lease(map);
    put(7, id_of(map));
    put(8, id_of(key));
    put(9, mt);
    put(10, kt);
    let old_node = raw(model, b"slotAt:in:", &[size(0).unwrap(), map]).unwrap();
    let old_id = id_of(old_node);
    put(15, old_id);
    let portable = encode(ProgramSpec {
        parameters: &[b"map", b"key"],
        captures: &[],
        lets: &[],
        statements: &[StatementSpec::Expression { root_node: 2 }],
        nodes: &[
            NodeSpec::LexicalRead(0),
            NodeSpec::LexicalRead(1),
            NodeSpec::Send {
                selector: b"at:",
                operands: &[0, 1],
            },
        ],
    })
    .unwrap();
    let bytes = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(&portable).unwrap()],
    );
    let code = call(named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes]);
    let captures = call(vector, b"new", &[]);
    let block = call(
        named(b"Codegen"),
        b"materializeSynchronous:captures:",
        &[code, captures],
    );
    let resumable = call(
        named(b"Codegen"),
        b"materializeResumable:captures:",
        &[code, captures],
    );
    set(roots_vt, b"retain:", retain_hook);
    set_inputs(&[map, key]);
    put(11, z);
    assert_eq!(raw(block, b"value:value:", &[map, key]).unwrap(), value);
    put(9, lease(map));
    put(10, lease(key));
    assert_eq!(get(6), z, "Map must not re-register caller inputs");
    put(14, one);
    put(6, z);
    assert_eq!(raw(block, b"value:value:", &[map, key]).unwrap(), value);
    put(9, lease(map));
    put(10, lease(key));
    assert_eq!(get(6), z);
    collect();
    assert!(
        !present(old_id),
        "completed Map lookup must release its detached node"
    );
    put(14, z);
    put(12, one);
    assert_eq!(
        raw(block, b"value:value:", &[map, key]).unwrap_err(),
        hash_failure
    );
    put(12, z);

    let observe = |receiver, vt, n: &[u8]| {
        let original = raw(host.id.root, b"bind:to:", &[names[n], receiver]).unwrap();
        set(vt, b"collectionQueryAlias", original);
        set(vt, n, query);
        original
    };
    let restore = |vt, n: &[u8], original| {
        set(vt, n, original);
        raw(
            vt,
            b"removeMethod:",
            &[names[b"collectionQueryAlias".as_slice()]],
        )
        .unwrap();
    };
    set_inputs(&[reflect, value, symbol]);
    let original = observe(host.id.root, root_vt, b"behaviorOf:");
    assert_eq!(raw(reflect, b"vtableOf:", &[value]).unwrap(), object_vt);
    assert_eq!(get(6), z);
    put(12, one);
    assert_eq!(
        raw(reflect, b"vtableOf:", &[value]).unwrap_err(),
        read_failure
    );
    put(12, z);
    restore(root_vt, b"behaviorOf:", original);
    let original = observe(symbol, symbol_vt, b"nameHash");
    let before = get(13);
    assert!(as_size(raw(symbol, b"collectionHash", &[]).unwrap()).is_some());
    assert_ne!(
        get(13),
        before,
        "Symbol hashing must enter the current nameHash"
    );
    assert_eq!(get(6), z);
    restore(symbol_vt, b"nameHash", original);
    set_inputs(&[word_vt]);
    let original = observe(gc, gc_vt, b"canOmitAllocationFor:");
    assert!(as_size(raw(model, b"canOmitAllocationFor:", &[word_vt]).unwrap()).is_some());
    assert_eq!(get(6), z);
    put(12, one);
    assert_eq!(
        raw(model, b"canOmitAllocationFor:", &[word_vt]).unwrap_err(),
        read_failure
    );
    put(12, z);
    restore(gc_vt, b"canOmitAllocationFor:", original);

    put(21, one);
    set(vector_vt, b"at:", vector_hook);
    for body in [block, resumable] {
        set_inputs(&[]);
        let mut vector_value = raw(vector, b"new", &[]).unwrap();
        let mut vector_token = lease(vector_value);
        let mut replacement = raw(vector, b"new", &[]).unwrap();
        let mut replacement_token = lease(replacement);
        for _ in 0..40 {
            for (v, t, element) in [
                (&mut vector_value, &mut vector_token, value),
                (&mut replacement, &mut replacement_token, symbol),
            ] {
                let next = raw(*v, b"add:", &[element]).unwrap();
                let token = lease(next);
                release(*t);
                *v = next;
                *t = token;
            }
        }
        let nodes = |v| [2, 3].map(|i| raw(model, b"slotAt:in:", &[size(i).unwrap(), v]).unwrap());
        let [old_root, old_tail] = nodes(vector_value);
        let old_ids = [id_of(old_root), id_of(old_tail)];
        let [new_root, new_tail] = nodes(replacement);
        let index = raw(integer, b"fromWord:", &[size(39).unwrap()]).unwrap();
        let index_token = lease(index);
        put(7, id_of(vector_value));
        put(8, id_of(index));
        put(9, vector_token);
        put(10, index_token);
        set_inputs(&[vector_value, index]);
        put(16, one);
        put(17, old_ids[0]);
        put(18, old_ids[1]);
        put(19, new_root);
        put(20, new_tail);
        let original = observe(integer, integer_vt, b"wordOf:");
        assert_eq!(
            raw(body, b"value:value:", &[vector_value, index]).unwrap(),
            value
        );
        put(9, lease(vector_value));
        put(10, lease(index));
        assert_eq!(get(6), z, "Vector must not re-register caller inputs");
        put(16, z);
        collect();
        assert!(
            !present(old_ids[0]) && !present(old_ids[1]),
            "completed Vector read must release the detached tree and tail"
        );
        put(6, z);
        put(12, one);
        assert_eq!(
            raw(body, b"value:value:", &[vector_value, index]).unwrap_err(),
            read_failure
        );
        assert_eq!(get(6), z);
        put(12, z);
        restore(integer_vt, b"wordOf:", original);
        release(replacement_token);
        for field in [9, 10] {
            let token = get(field);
            put(field, null);
            if !token.is_null() {
                release(token);
            }
        }
    }
    set(vector_vt, b"at:", vector_entry);
    set(roots_vt, b"retain:", retainer);
    raw(
        vector_vt,
        b"removeMethod:",
        &[names[b"collectionOriginalVectorAt:".as_slice()]],
    )
    .unwrap();
    raw(
        roots_vt,
        b"removeMethod:",
        &[names[b"collectionOriginalRetain:".as_slice()]],
    )
    .unwrap();
}
