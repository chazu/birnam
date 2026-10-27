//! Portable materialization retains its nil/lastError convention and retry behavior.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn materialization_rejection_and_retry_preserve_owner_results() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let cg = named(b"Codegen");
    let gc = named(b"GC");
    let tcc = named(b"TCC");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let bytes = |b: &[u8]| call(named(b"Bytes"), b"fromBytes:", &[q.byte_values(b).unwrap()]);
    let code = call(
        named(b"CodeObject"),
        b"fromCanonicalBytes:",
        &[bytes(include_bytes!(
            "../../../tests/fixtures/portable-code-v1/program-empty.bin"
        ))],
    );
    q.check_result(code).unwrap();
    let captures = call(named(b"Vector"), b"new", &[]);
    let extra = call(captures, b"add:", &[nil]);
    let error = call(
        named(b"Error"),
        b"kind:message:",
        &[
            key(b"materializationRejected"),
            q.string(b"fixture refusal").unwrap(),
        ],
    );
    let path = std::env::temp_dir().join(format!(
        "birnam-materialization-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/materialization.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let probe = |answer| {
        let state = hooks.context(&[gc, answer, size(0).unwrap(), ptr::null_mut()]);
        let method = call(
            call(fixture, b"methodAt:", &[key(b"result")]),
            b"withContext:",
            &[state],
        );
        (method, state)
    };
    let bind = |r, n: &[u8], m| {
        let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[r]);
        call(vt, b"addMethod", &[key(n), m]);
    };
    let capture = |r, n: &[u8]| call(h.id.root, b"bind:to:", &[key(n), r]);
    let collect = || {
        call(gc, b"collectPrecisely", &[]);
        call(tcc, b"checkpoint", &[]);
    };
    let count = || as_size(call(call(cg, b"materializationCount", &[]), b"word", &[])).unwrap();
    let units = || as_size(call(tcc, b"liveNativeUnits", &[])).unwrap();
    call(cg, b"evictMaterializations", &[]);
    collect();
    let baseline = units();
    let fake = call(named(b"Object"), b"clone", &[]);
    let (initializer, initial_state) = probe(nil);
    bind(fake, b"initialize", initializer);
    for resumable in [false, true] {
        let selector = if resumable {
            b"materializeResumable:captures:".as_slice()
        } else {
            b"materializeSynchronous:captures:"
        };
        let missing = if resumable {
            b"RustCodegen resumable materialization needs an admitted CodeObject".as_slice()
        } else {
            b"RustCodegen materialization needs an admitted CodeObject"
        };
        let mismatch = if resumable {
            b"RustCodegen resumable capture count disagrees with CodeObject".as_slice()
        } else {
            b"RustCodegen materialization capture count disagrees with CodeObject"
        };
        let semantics = if resumable {
            b"RustCodegen resumable materialization rejected PortableProgram semantics".as_slice()
        } else {
            b"RustCodegen materialization rejected PortableProgram semantics"
        };
        let compilation = if resumable {
            b"RustCodegen could not compile portable resumable materialization".as_slice()
        } else {
            b"RustCodegen could not compile PortableProgram materialization"
        };
        let reject = |input, environment, diagnostic: &[u8]| {
            assert_eq!(call(cg, selector, &[input, environment]), nil);
            assert_eq!(
                q.string_bytes(call(cg, b"lastError", &[])).unwrap(),
                diagnostic
            );
            assert_eq!(count(), 0);
            collect();
            assert_eq!(units(), baseline);
        };
        reject(nil, captures, missing);
        reject(code, extra, mismatch);
        reject(code, nil, mismatch);
        let factory = named(b"CodeObject");
        let original = capture(factory, b"canonicalBytesOf:");
        let (malformed, _) = probe(bytes(b"not a program"));
        bind(factory, b"canonicalBytesOf:", malformed);
        reject(code, captures, semantics);
        bind(factory, b"canonicalBytesOf:", original);
        for (receiver, name, answer) in [
            (
                named(b"NativeBackend"),
                b"emit:source:filename:persistent:synchronous:".as_slice(),
                error,
            ),
            (tcc, b"compile:", error),
            (tcc, b"compile:", fake),
        ] {
            let original = capture(receiver, name);
            let (replacement, state) = probe(answer);
            bind(receiver, name, replacement);
            for initial in [nil, error] {
                hooks.put(initial_state, 1, initial);
                reject(code, captures, compilation);
            }
            assert_eq!(hooks.count(state, 2), 2);
            let failure = hooks.fault(73);
            hooks.put(state, 3, failure);
            assert_eq!(
                unsafe { h.id.send(cg, key(selector), &[code, captures]) }.unwrap_err(),
                failure
            );
            assert_eq!(call(cg, b"lastError", &[]), nil);
            assert_eq!(count(), 0);
            hooks.put(state, 3, ptr::null_mut());
            if answer == fake {
                hooks.put(initial_state, 3, failure);
                assert_eq!(
                    unsafe { h.id.send(cg, key(selector), &[code, captures]) }.unwrap_err(),
                    failure
                );
                hooks.put(initial_state, 3, ptr::null_mut());
                assert_eq!(count(), 0);
            }
            bind(receiver, name, original);
        }
        // Refusing the final Block does not publish the compiled candidate in the cache.
        let block_factory = named(b"Block");
        let constructor = b"fromMethod:codeObject:captures:";
        let original = capture(block_factory, constructor);
        let (replacement, state) = probe(nil);
        bind(block_factory, constructor, replacement);
        for answer in [nil, error] {
            hooks.put(state, 1, answer);
            assert_eq!(call(cg, selector, &[code, captures]), answer);
            assert_eq!(count(), 0);
            collect();
            assert_eq!(units(), baseline);
        }
        bind(block_factory, constructor, original);
        // A successful retry clears the diagnostic and is the only path that publishes a cache entry.
        {
            let result = h.scope();
            let block = result.call(cg, selector, &[code, captures]).unwrap();
            result.check_result(block).unwrap();
            assert_eq!(call(cg, b"lastError", &[]), nil);
            assert_eq!(count(), 1);
            assert_eq!(result.call(block, b"value", &[]).unwrap(), nil);
            call(cg, b"evictMaterializations", &[]);
            collect();
            assert_eq!(units(), baseline + 1);
        }
        collect();
        assert_eq!(units(), baseline);
    }
}
