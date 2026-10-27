//! Results remain alive through current collecting cleanup and caller handoff.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::native_error;
#[test]
#[ignore = "requires production Oop compiler and owners"]
fn fresh_result_survives_collecting_cleanup_until_caller_handoff() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    let roots = host.roots;
    let word = named(b"Word");
    let reflect = named(b"Reflect");
    let system = named(b"System");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let source=br#"let probe := [Object clone]; [Reflect defineMethod: #make: on: probe as: [Execution block: [:self :input | let unused := [Execution wordResultOf: input selector: #word]; [Execution wordResultOf: input selector: #word]] withNativeSignature: "oO"]]; [Globals at: #CleanupProbe put: probe]; true"#;
    let text = q.string(source).unwrap();
    let path = std::env::temp_dir().join(format!(
        "birnam-cleanup-result-{}.dylib",
        std::process::id()
    ));
    let output = q.string(path.as_os_str().as_encoded_bytes()).unwrap();
    let compiled = call(
        named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[text, output, output, named(b"true")],
    );
    q.check_result(compiled).unwrap();
    assert_ne!(compiled, named(b"nil"));
    q.check_result(call(named(b"Codegen"), b"load:", &[output]))
        .unwrap();
    let probe = host.global(&q, b"CleanupProbe").unwrap();
    let input = call(named(b"Integer"), b"fromWord:", &[size(123).unwrap()]);
    host.send(probe, b"make:", &[input]).unwrap();
    let fixture_path = path.with_extension("fixture.dylib");
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/result-cleanup.c")
            )
            .arg("-o")
            .arg(&fixture_path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &fixture_path, &[]).unwrap();
    std::fs::remove_file(fixture_path).unwrap();
    let fault = |status, detail| {
        let error = unsafe {
            host.id.send(
                named(b"Execution"),
                key(b"failNative:detail:"),
                &[size(status).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(error).unwrap()
    };
    let batch_failure = fault(811, 35);
    let inventory_failure = fault(812, 36);
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let context = call(
        host.id.root,
        b"sequence",
        &[gc, monitor, batch_failure, inventory_failure],
    );
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[context],
        )
    };
    for n in [
        b"watch:failure:".as_slice(),
        b"lastIdentity",
        b"lost",
        b"calls",
        b"allocation:bytes:identity:",
        b"present:",
    ] {
        call(monitor_vt, b"addMethod", &[key(n), method(n)]);
    }
    let factory = method(b"factory");
    let cleanup = method(b"cleanup");
    let inventory_add = method(b"add");
    let rv = call(host.id.root, b"behaviorOf:", &[roots]);
    let wv = named(b"WordVt");
    let vv = named(b"VectorVt");
    let constructor = call(host.id.root, b"bind:to:", &[key(b"fromBits:"), word]);
    let releaser = call(host.id.root, b"bind:to:", &[key(b"release:"), roots]);
    let adder = call(vv, b"lookup", &[key(b"add:")]);
    let add = key(b"addMethod");
    let release = key(b"release:");
    let retain = key(b"retain:");
    let from_bits = key(b"fromBits:");
    let vector_add = key(b"add:");
    let identity = key(b"identityOf:");
    let collect = key(b"collectPrecisely");
    let protect = key(b"releaseTokensProtecting");
    let bits = key(b"bits");
    let watch = key(b"watch:failure:");
    let last = key(b"lastIdentity");
    let lost = key(b"lost");
    let calls = key(b"calls");
    let present = key(b"present:");
    let z = size(0).unwrap();
    let factory_alias = key(b"cleanupOriginalFactory:");
    let add_alias = key(b"cleanupOriginalAdd:");
    let release_alias = key(b"cleanupOriginalRelease:");
    call(wv, b"addMethod", &[factory_alias, constructor]);
    call(vv, b"addMethod", &[add_alias, adder]);
    call(rv, b"addMethod", &[release_alias, releaser]);
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let set = |vt, selector, implementation| {
        raw(vt, add, &[selector, implementation]).unwrap();
    };
    let observe = |id, mode| {
        raw(monitor, watch, &[id, size(mode).unwrap()]).unwrap();
    };
    let survived = || {
        assert!(
            as_size(raw(monitor, calls, &[]).unwrap()).unwrap() > 0,
            "the current cleanup must actually collect"
        );
        assert_eq!(
            raw(monitor, lost, &[]).unwrap(),
            z,
            "result was collected before caller handoff"
        );
    };
    observe(z, 0);
    set(wv, from_bits, factory);
    set(rv, release, cleanup);
    let result = host.send(probe, b"make:", &[input]).unwrap();
    let result_token = raw(roots, retain, &[result]).unwrap();
    survived();
    assert_eq!(
        raw(monitor, last, &[]).unwrap(),
        raw(host.id.root, identity, &[result]).unwrap()
    );
    set(rv, release, releaser);
    set(wv, from_bits, constructor);
    assert_eq!(as_size(raw(result, bits, &[]).unwrap()), Some(123));

    let x = raw(word, from_bits, &[size(1).unwrap()]).unwrap();
    let xt = raw(roots, retain, &[x]).unwrap();
    let xid = raw(host.id.root, identity, &[x]).unwrap();
    let y = raw(word, from_bits, &[size(2).unwrap()]).unwrap();
    let yt = raw(roots, retain, &[y]).unwrap();
    let yid = raw(host.id.root, identity, &[y]).unwrap();
    observe(yid, 1);
    set(rv, release, cleanup);
    assert_eq!(
        raw(roots, protect, &[y, xt, yt]).unwrap_err(),
        batch_failure
    );
    survived();
    assert_eq!(raw(monitor, calls, &[]).unwrap(), size(2).unwrap());
    set(rv, release, releaser);
    assert_eq!(raw(roots, release, &[xt]).unwrap(), z);
    assert_eq!(raw(roots, release, &[yt]).unwrap(), z);
    raw(gc, collect, &[]).unwrap();
    assert_eq!(
        raw(monitor, present, &[yid]).unwrap(),
        z,
        "failed cleanup must release private result protection"
    );
    assert_eq!(raw(monitor, present, &[xid]).unwrap(), z);
    raw(roots, release, &[result_token]).unwrap();

    for (selector, args) in [
        (b"directMethodsOf:".as_slice(), vec![system]),
        (b"allMethodsOf:", vec![system]),
        (b"allGlobalNames", vec![]),
    ] {
        observe(z, 0);
        set(vv, vector_add, inventory_add);
        set(rv, release, cleanup);
        let result = host.send(reflect, selector, &args).unwrap();
        let token = raw(roots, retain, &[result]).unwrap();
        survived();
        assert_eq!(
            raw(monitor, last, &[]).unwrap(),
            raw(host.id.root, identity, &[result]).unwrap()
        );
        set(rv, release, releaser);
        set(vv, vector_add, adder);
        assert!(
            q.word_bits(call(
                named(b"Integer"),
                b"wordOf:",
                &[call(result, b"size", &[])]
            ))
            .unwrap()
                > 0
        );
        raw(roots, release, &[token]).unwrap();
    }
    observe(z, 2);
    set(vv, vector_add, inventory_add);
    set(rv, release, cleanup);
    let error = host.send(reflect, b"allGlobalNames", &[]).unwrap_err();
    assert_eq!(error, native_error("allGlobalNames", inventory_failure));
    survived();
    let failed = raw(monitor, last, &[]).unwrap();
    assert_ne!(failed, z);
    set(rv, release, releaser);
    set(vv, vector_add, adder);
    raw(gc, collect, &[]).unwrap();
    assert_eq!(
        raw(monitor, present, &[failed]).unwrap(),
        z,
        "failed inventory handoff must release its result"
    );
    for (vt, alias) in [(wv, factory_alias), (vv, add_alias), (rv, release_alias)] {
        call(vt, b"removeMethod:", &[alias]);
    }
    std::fs::remove_file(path).unwrap();
}
