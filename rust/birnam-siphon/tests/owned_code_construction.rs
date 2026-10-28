//! The code producer establishes source identity and shape before publication.
#[path = "../../birnam-host/src/test_hooks.rs"]
#[allow(dead_code)]
mod hooks;
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{Oop, as_size, size};
use std::path::Path;

#[test]
#[ignore = "requires current native owners and the build-only Lang seed"]
fn resumable_code_construction_is_pure_and_owns_its_source_association() {
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
    let compile = |source: &[u8]| {
        let result = call(cg, b"compileResumable:", &[q.string(source).unwrap()]);
        assert_eq!(
            call(h.id.root, b"sameNativeMethod:as:", &[result, result]),
            size(1).unwrap()
        );
        result
    };
    let eval = |source: &[u8]| {
        let method = compile(source);
        let block = call(named(b"Block"), b"fromMethod:captureOperands:", &[method]);
        let result = call(block, b"value", &[]);
        call(result, b"captureCount", &[]);
        result
    };
    let baseline = compile(b"0");
    let baseline_block = call(named(b"Block"), b"fromMethod:captureOperands:", &[baseline]);
    assert_eq!(
        q.word_bits(call(call(baseline_block, b"value", &[]), b"word", &[]))
            .unwrap(),
        0
    );
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let before = epoch();
    let method = compile(b"let x := 37; [| x]");
    assert_eq!(
        epoch(),
        before,
        "compiled code must not create metadata vtables"
    );
    assert_eq!(
        call(h.id.root, b"behaviorOf:", &[method]),
        call(h.id.root, b"methodBehavior", &[])
    );
    let source = call(method, b"codeObject", &[]);
    assert_ne!(source, named(b"nil"));
    assert_eq!(call(method, b"isSynchronous", &[]), named(b"false"));
    let before = epoch();
    let captured = call(method, b"captured", &[]);
    assert_eq!(epoch(), before);
    assert_eq!(call(captured, b"codeObject", &[]), source);
    assert_eq!(call(captured, b"isSynchronous", &[]), named(b"false"));
    let other = compile(b"42");
    let other_code = call(other, b"codeObject", &[]);
    let failure = hooks.fault(881);
    let denied = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied);
    let factory = named(b"CodeObject");
    let factory_vt = call(h.id.root, b"behaviorOf:", &[factory]);
    let mut saved = Vec::new();
    for name in [b"parameterCountOf:".as_slice(), b"captureCountOf:"] {
        saved.push((key(name), call(factory_vt, b"lookup", &[key(name)])));
        call(factory_vt, b"addMethod", &[key(name), deny]);
    }
    let before = epoch();
    assert_eq!(call(method, b"admitCodeObject:", &[source]), method);
    assert!(h.send(method, b"admitCodeObject:", &[other_code]).is_err());
    assert_eq!(
        call(
            method,
            b"admitCodeObject:synchronous:",
            &[source, named(b"false")]
        ),
        method
    );
    assert!(
        h.send(
            method,
            b"admitCodeObject:synchronous:",
            &[source, named(b"true")]
        )
        .is_err()
    );
    assert_eq!(epoch(), before);
    assert_eq!(
        hooks.count(denied, 1),
        0,
        "existing code identity must not be re-admitted"
    );
    for (name, original) in saved {
        call(factory_vt, b"addMethod", &[name, original]);
    }

    // The compiler must no longer use the post-construction metadata mutation.
    let behavior = call(h.id.root, b"methodBehavior", &[]);
    let mut saved = Vec::new();
    for name in [
        b"admitCodeObject:".as_slice(),
        b"admitCodeObject:synchronous:",
    ] {
        saved.push((key(name), call(behavior, b"lookup", &[key(name)])));
        call(behavior, b"addMethod", &[key(name), deny]);
    }
    let before = epoch();
    let pure = compile(b"[| 19]");
    assert_eq!(epoch(), before);
    assert_ne!(call(pure, b"codeObject", &[]), named(b"nil"));
    assert_eq!(hooks.count(denied, 1), 0);
    for (name, original) in saved {
        call(behavior, b"addMethod", &[name, original]);
    }

    // Observe the producer's real descriptor and test rejection before creation.
    let path = std::env::temp_dir().join(format!("birnam-owned-code-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
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
    let name = key(b"fromStep:descriptor:selection:owner:callPlans:codeObject:");
    let vt = call(h.id.root, b"behaviorOf:", &[execution]);
    let original = call(vt, b"lookup", &[name]);
    let state = hooks.context(&[original, named(b"nil")]);
    let capture = call(
        call(fixture, b"methodAt:", &[key(b"capture")]),
        b"withContext:",
        &[state],
    );
    call(vt, b"addMethod", &[name, capture]);
    compile(b"42");
    call(vt, b"addMethod", &[name, original]);
    let arguments = call(state, b"at:", &[size(1).unwrap()]);
    let mut args: Vec<_> = (0..6)
        .map(|i| call(arguments, b"at:", &[size(i).unwrap()]))
        .collect();
    let original_code = args[5];
    // Root-level lets publish Globals; this isolated test needs a local capture.
    for wrong in [eval(b"[:x | x]"), eval(b"[[| let x := 7; [| x]] value]")] {
        args[5] = call(wrong, b"codeObject", &[]);
        let before = epoch();
        assert!(unsafe { h.id.send(execution, name, &args) }.is_err());
        assert_eq!(epoch(), before);
    }
    args[5] = original_code;
    let count_name = key(b"captureCountOf:");
    let count_method = call(factory_vt, b"lookup", &[count_name]);
    call(factory_vt, b"addMethod", &[count_name, deny]);
    let before = epoch();
    assert_eq!(
        unsafe { h.id.send(execution, name, &args) }.unwrap_err(),
        failure
    );
    assert_eq!(epoch(), before);
    call(factory_vt, b"addMethod", &[count_name, count_method]);
    let before = epoch();
    let created = call(
        execution,
        b"fromStep:descriptor:selection:owner:callPlans:codeObject:",
        &args,
    );
    assert_eq!(epoch(), before);
    assert_eq!(call(created, b"codeObject", &[]), original_code);

    // Equal portable descriptions bind a fresh instance; another program cannot.
    let encoded = call(factory, b"canonicalBytesOf:", &[original_code]);
    let equal = call(factory, b"fromCanonicalBytes:", &[encoded]);
    assert_ne!(equal, original_code);
    let captures = call(named(b"Vector"), b"new", &[]);
    let before = epoch();
    let block = call(
        named(b"Block"),
        b"fromMethod:codeObject:captures:",
        &[created, equal, captures],
    );
    assert_eq!(epoch(), before);
    assert_eq!(call(block, b"codeObject", &[]), equal);
    assert_eq!(call(created, b"codeObject", &[]), original_code);
    let different = call(compile(b"99"), b"codeObject", &[]);
    assert!(
        h.send(
            named(b"Block"),
            b"fromMethod:codeObject:captures:",
            &[created, different, captures]
        )
        .is_err()
    );
    call(named(b"GC"), b"collectPrecisely", &[]);
    assert_eq!(
        q.word_bits(call(call(block, b"value", &[]), b"word", &[]))
            .unwrap(),
        42
    );
}
