//! Numeric owner scopes retain fresh Oops across nested collection and failure.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop numeric owners"]
fn native_inputs_borrow_call_coverage_but_outputs_survive_collecting_validation() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let roots = host.roots;
    let gc = named(b"GC");
    let word = named(b"Word");
    let integer = named(b"Integer");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let a = call(word, b"fromBits:", &[size(40).unwrap()]);
    let b = call(word, b"fromBits:", &[size(2).unwrap()]);
    let x = call(integer, b"fromWord:", &[size(17).unwrap()]);
    let path = std::env::temp_dir().join(format!(
        "birnam-numeric-inputs-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/numeric-inputs.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let query = key(b"behaviorOf:");
    let retain = key(b"retain:");
    let at = key(b"at:");
    let put = key(b"at:put:");
    let add = key(b"addMethod");
    let release = key(b"release:");
    let collect = key(b"collectPrecisely");
    let bits = key(b"bits");
    let word_of = key(b"wordOf:");
    let as_word = key(b"word");
    let from_word = key(b"fromWord:");
    let from_bits = key(b"fromBits:");
    let plus = key(b"wrappingAdd:");
    let identity = key(b"identityOf:");
    let present = key(b"present:");
    let query_alias = key(b"numericOriginalBehavior:");
    let retain_alias = key(b"numericOriginalRetain:");
    let original_query = call(host.id.root, b"bind:to:", &[query, host.id.root]);
    let original_retain = call(host.id.root, b"bind:to:", &[retain, roots]);
    let root_vt = call(host.id.root, b"behaviorOf:", &[host.id.root]);
    let roots_vt = call(host.id.root, b"behaviorOf:", &[roots]);
    call(root_vt, b"addMethod", &[query_alias, original_query]);
    call(roots_vt, b"addMethod", &[retain_alias, original_retain]);
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(812).unwrap(), size(19).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let ids: Vec<_> = [word, integer, a, b, x]
        .into_iter()
        .map(|value| call(host.id.root, b"identityOf:", &[value]))
        .collect();
    let inputs = call(host.id.root, b"sequence", &ids);
    let z = size(0).unwrap();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            word,
            monitor,
            z,
            z,
            z,
            z,
            z,
            size(2).unwrap(),
            z,
            z,
            z,
            failure,
            inputs,
        ],
    );
    let method = |name: &[u8]| {
        call(
            call(module, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[context],
        )
    };
    for name in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        call(monitor_vt, b"addMethod", &[key(name), method(name)]);
    }
    let query_hook = method(b"query");
    let retain_hook = method(b"retain");
    // Use raw Id sends while observing Roots, so Host argument snapshots cannot
    // be mistaken for registrations made by the numeric owners under test.
    let send = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let get = |index| send(context, at, &[size(index).unwrap()]).unwrap();
    send(roots_vt, add, &[retain, retain_hook]).unwrap();
    send(root_vt, add, &[query, query_hook]).unwrap();
    let result = send(a, plus, &[b]).unwrap();
    // Keep the returned value before inspecting counters; exclude this caller
    // bookkeeping by entering the captured retainer through its alias.
    let rt = send(roots, retain_alias, &[result]).unwrap();
    assert_eq!(
        get(3),
        z,
        "numeric construction must use owner roots without Root registrations"
    );
    assert_ne!(get(9), z, "nested construction must have been exercised");
    let result_id = send(host.id.root, identity, &[result]).unwrap();
    assert_eq!(as_size(send(result, bits, &[]).unwrap()), Some(42));
    assert_eq!(as_size(send(integer, word_of, &[x]).unwrap()), Some(17));
    let boxed = send(integer, from_word, &[size(91).unwrap()]).unwrap();
    let bt = send(roots, retain, &[boxed]).unwrap();
    assert_eq!(as_size(send(boxed, as_word, &[]).unwrap()), Some(91));
    assert_eq!(
        get(4),
        z,
        "numeric owners must not re-register receiver or input Oops"
    );
    assert!(as_size(get(5)).unwrap() >= 6);
    for (factory, constructor) in [(word, from_bits), (integer, from_word)] {
        send(context, put, &[size(10).unwrap(), size(2).unwrap()]).unwrap();
        assert_eq!(
            send(factory, constructor, &[size(1234).unwrap()]).unwrap_err(),
            failure
        );
        let failed = get(11);
        assert_ne!(failed, z);
        // Cleanup itself is not a validation callback. Keep the replacement
        // live, but do not rescan all allocations for every GC vtable lookup.
        send(context, put, &[size(7).unwrap(), size(1).unwrap()]).unwrap();
        send(gc, collect, &[]).unwrap();
        send(context, put, &[size(7).unwrap(), z]).unwrap();
        assert_eq!(
            send(monitor, present, &[failed]).unwrap(),
            z,
            "failed construction must release its temporary value"
        );
    }
    send(roots, release, &[rt]).unwrap();
    send(context, put, &[size(7).unwrap(), size(1).unwrap()]).unwrap();
    send(gc, collect, &[]).unwrap();
    send(context, put, &[size(7).unwrap(), z]).unwrap();
    assert_eq!(
        send(monitor, present, &[result_id]).unwrap(),
        z,
        "successful owner scopes must drain at return"
    );
    send(root_vt, add, &[query, original_query]).unwrap();
    send(roots_vt, add, &[retain, original_retain]).unwrap();
    send(roots, release, &[bt]).unwrap();
    call(root_vt, b"removeMethod:", &[query_alias]);
    call(roots_vt, b"removeMethod:", &[retain_alias]);
}
