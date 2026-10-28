//! Synchronous code and its direct Method entry are constructed by their producer.
#[path = "../../birnam-host/src/test_hooks.rs"]
#[allow(dead_code)]
mod hooks;
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{Oop, as_size, size};

#[test]
#[ignore = "requires native owners and build-only Lang seed"]
fn synchronous_code_has_pure_construction_and_direct_receiver_binding() {
    let h = minimal::native_floor();
    let q = h.scope();
    for name in ["peg", "lang_seed"] {
        h.load_native(
            &q,
            &h.resources.lib_dir.join(format!("libbirnam_{name}.dylib")),
            &[h.namespace],
        )
        .unwrap();
    }
    let hooks = hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| {
        q.call(r, n, a)
            .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(n)))
    };
    let cg = named(b"Codegen");
    let execution = named(b"Execution");
    let gc = named(b"GC");
    let compile = |source: &[u8]| call(cg, b"compileSynchronous:", &[q.string(source).unwrap()]);
    let evaluate = |source: &[u8]| {
        let method = compile(source);
        let block = call(named(b"Block"), b"fromMethod:captureOperands:", &[method]);
        call(block, b"value", &[])
    };
    compile(b"0");
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let base = call(h.id.root, b"methodBehavior", &[]);
    let failure = hooks.fault(889);
    let denied = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied);
    let mut saved = Vec::new();
    for name in [
        b"admitCodeObject:".as_slice(),
        b"admitCodeObject:synchronous:",
    ] {
        saved.push((key(name), call(base, b"lookup", &[key(name)])));
        call(base, b"addMethod", &[key(name), deny]);
    }
    let before = epoch();
    let method = compile(b"[| 42]");
    assert_eq!(epoch(), before);
    assert_eq!(call(h.id.root, b"behaviorOf:", &[method]), base);
    assert_eq!(call(method, b"isSynchronous", &[]), named(b"true"));
    let source = call(method, b"codeObject", &[]);
    assert_ne!(source, named(b"nil"));
    assert_eq!(hooks.count(denied, 1), 0);
    for (name, m) in saved {
        call(base, b"addMethod", &[name, m]);
    }
    let before = epoch();
    let captured = call(method, b"captured", &[]);
    assert_eq!(epoch(), before);
    assert_eq!(call(captured, b"codeObject", &[]), source);
    let foreign_source = call(compile(b"99"), b"codeObject", &[]);
    let owner = named(b"CodeObject");
    let ovt = call(h.id.root, b"behaviorOf:", &[owner]);
    let mut saved = Vec::new();
    for name in [b"parameterCountOf:".as_slice(), b"captureCountOf:"] {
        saved.push((key(name), call(ovt, b"lookup", &[key(name)])));
        call(ovt, b"addMethod", &[key(name), deny]);
    }
    let before = epoch();
    assert_eq!(call(method, b"admitCodeObject:", &[source]), method);
    assert!(
        h.send(method, b"admitCodeObject:", &[foreign_source])
            .is_err()
    );
    assert_eq!(
        call(
            method,
            b"admitCodeObject:synchronous:",
            &[source, named(b"true")]
        ),
        method
    );
    assert!(
        h.send(
            method,
            b"admitCodeObject:synchronous:",
            &[source, named(b"false")]
        )
        .is_err()
    );
    assert_eq!(epoch(), before);
    assert_eq!(hooks.count(denied, 1), 0);
    for (name, m) in saved {
        call(ovt, b"addMethod", &[name, m]);
    }

    // Capture the real producer constructor and reject unrelated source even
    // when its parameter/capture counts happen to match.
    let path =
        std::env::temp_dir().join(format!("birnam-native-code-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/prepared-call.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let ev = call(h.id.root, b"behaviorOf:", &[execution]);
    let creation = key(b"fromNativeCodeFactory:codeObject:");
    let original = call(ev, b"lookup", &[creation]);
    let state = hooks.context(&[original, named(b"nil")]);
    let capture = call(
        call(fixture, b"methodAt:", &[key(b"capture")]),
        b"withContext:",
        &[state],
    );
    call(ev, b"addMethod", &[creation, capture]);
    compile(b"42");
    call(ev, b"addMethod", &[creation, original]);
    let recorded = call(state, b"at:", &[size(1).unwrap()]);
    let factory = call(recorded, b"at:", &[size(0).unwrap()]);
    let original_source = call(recorded, b"at:", &[size(1).unwrap()]);
    let before = epoch();
    assert!(unsafe { h.id.send(execution, creation, &[factory, foreign_source]) }.is_err());
    assert_eq!(epoch(), before);
    let count_name = key(b"captureCountOf:");
    let count_original = call(ovt, b"lookup", &[count_name]);
    call(ovt, b"addMethod", &[count_name, deny]);
    let before = epoch();
    assert_eq!(
        unsafe { h.id.send(execution, creation, &[factory, original_source]) }.unwrap_err(),
        failure
    );
    assert_eq!(epoch(), before);
    call(ovt, b"addMethod", &[count_name, count_original]);
    hooks.put(denied, 1, size(0).unwrap());

    let body = evaluate(b"[[| let captured := 37; [:self :arg | #[self captured arg]]] value]");
    // Runtime capture reads use the constructor's checked layout. Source
    // reflection must not become an implicit prerequisite for a Block call.
    let source_query = key(b"codeObject");
    let source_reader = call(base, b"lookup", &[source_query]);
    call(base, b"addMethod", &[source_query, deny]);
    let receiver = call(named(b"Object"), b"clone", &[]);
    let input = call(named(b"Object"), b"clone", &[]);
    let tuple = call(body, b"value:value:", &[receiver, input]);
    assert_eq!(q.vector_at(tuple, 0).unwrap(), receiver);
    assert_eq!(q.vector_at(tuple, 2).unwrap(), input);
    assert_eq!(hooks.count(denied, 1), 0);
    call(base, b"addMethod", &[source_query, source_reader]);
    let before = epoch();
    let bound = call(execution, b"methodFromBlock:", &[body]);
    assert_eq!(epoch(), before);
    let object = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[object]);
    call(
        named(b"Reflect"),
        b"defineMethod:on:as:",
        &[key(b"answer:"), object, bound],
    );
    let body_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[body]);
    call(body_vt, b"addMethod", &[key(b"value:value:"), deny]);
    let argument = call(named(b"Object"), b"clone", &[]);
    call(gc, b"collectPrecisely", &[]);
    let answer = call(object, b"answer:", &[argument]);
    assert_eq!(q.vector_at(answer, 0).unwrap(), object);
    assert_eq!(
        q.word_bits(call(q.vector_at(answer, 1).unwrap(), b"word", &[]))
            .unwrap(),
        37
    );
    assert_eq!(q.vector_at(answer, 2).unwrap(), argument);
    assert_eq!(hooks.count(denied, 1), 0);
    assert_eq!(call(object, b"answer:", &[]), named(b"nil"));
    assert_eq!(
        call(object, b"answer:", &[argument, argument]),
        named(b"nil")
    );

    // The shared source association can be rebound only on a fresh instance
    // of the same program, never by mutating an existing Method.
    let scalar = compile(b"42");
    let original = call(scalar, b"codeObject", &[]);
    let encoded = call(owner, b"canonicalBytesOf:", &[original]);
    let equal = call(owner, b"fromCanonicalBytes:", &[encoded]);
    let empty = call(named(b"Vector"), b"new", &[]);
    let before = epoch();
    let block = call(
        named(b"Block"),
        b"fromMethod:codeObject:captures:",
        &[scalar, equal, empty],
    );
    assert_eq!(epoch(), before);
    assert_eq!(call(block, b"codeObject", &[]), equal);
    assert_eq!(call(scalar, b"codeObject", &[]), original);
    assert!(
        h.send(
            named(b"Block"),
            b"fromMethod:codeObject:captures:",
            &[scalar, foreign_source, empty]
        )
        .is_err()
    );
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        q.word_bits(call(call(block, b"value", &[]), b"word", &[]))
            .unwrap(),
        42
    );

    // No code or capture lifetime depends on the original source roots.
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    let units = || as_size(call(named(b"TCC"), b"liveNativeUnits", &[])).unwrap();
    let baseline = units();
    {
        let local = h.scope();
        let text = local
            .string(b"[[| let captured := 53; [:self | captured]] value]")
            .unwrap();
        let code = local.call(cg, b"compileSynchronous:", &[text]).unwrap();
        let root = local
            .call(named(b"Block"), b"fromMethod:captureOperands:", &[code])
            .unwrap();
        let body = local.call(root, b"value", &[]).unwrap();
        let method = local.call(execution, b"methodFromBlock:", &[body]).unwrap();
        local
            .call(vt, b"addMethod", &[key(b"retained"), method])
            .unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(units(), baseline + 1);
    let result = call(object, b"retained", &[]);
    assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 53);
    unsafe { h.id.send(vt, key(b"removeMethod:"), &[key(b"retained")]) }.unwrap();
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    assert_eq!(units(), baseline);
}
