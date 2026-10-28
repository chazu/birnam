//! Generated-call roots preserve locals/captures and reject failed acquisitions.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires rebuilt Oop synchronous compiler and production owners"]
fn local_and_capture_aliases_keep_live_call_protection() {
    let host = oop::bootstrap();
    let q = host.scope();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    let roots = host.roots;
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let path = std::env::temp_dir().join(format!("birnam-root-reuse-{}.dylib", std::process::id()));
    let fixture_path = path.with_extension("fixture.dylib");
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/root-reuse.c")
            )
            .arg("-o")
            .arg(&fixture_path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &fixture_path, &[]).unwrap();
    std::fs::remove_file(fixture_path).unwrap();
    let temporary = host.scope();
    let marker = temporary.call(named(b"Object"), b"clone", &[]).unwrap();
    let marker_id = call(host.id.root, b"identityOf:", &[marker]);
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(814).unwrap(), size(23).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let state = call(
        host.id.root,
        b"sequence",
        &[named(b"Object"), gc, marker_id, failure],
    );
    let method = |n: &[u8]| {
        call(
            call(module, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[state],
        )
    };
    let hook = call(named(b"Object"), b"clone", &[]);
    let hv = call(host.id.root, b"ensurePrivateVtableFor:", &[hook]);
    for n in [
        b"fresh".as_slice(),
        b"echo:",
        b"checkpoint",
        b"reset:",
        b"counts",
        b"remaining",
        b"allocation:bytes:identity:",
    ] {
        call(hv, b"addMethod", &[key(n), method(n)]);
    }
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"ReuseHook"), hook]);
    host.send(globals, b"at:put:", &[key(b"ReuseMarker"), marker])
        .unwrap();
    let mut source = String::from(
        "let hook := [Globals at: #ReuseHook]; let marker := [Globals at: #ReuseMarker]; let probe := [Object clone];\n",
    );
    source.push_str("[Reflect defineMethod: #run on: probe as: [Execution block: [:self | let a := [hook fresh]; [hook echo: a]; [hook echo: a]; [hook echo: marker]; [hook echo: marker]; [hook checkpoint]; #done] withNativeSignature: \"o\"]];\n");
    source.push_str("[Reflect defineMethod: #evict on: probe as: [Execution block: [:self |\n");
    for i in 0..40 {
        source.push_str(&format!("let a{i} := [hook fresh];\n"));
    }
    source.push_str("[hook echo: a0]; [hook echo: a39]; [hook checkpoint]; #done] withNativeSignature: \"o\"]]; [Globals at: #ReuseProbe put: probe]; true");
    let text = q.string(source.as_bytes()).unwrap();
    let output = q.string(path.as_os_str().as_encoded_bytes()).unwrap();
    q.check_result(call(
        named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[text, output, output, named(b"true")],
    ))
    .unwrap();
    q.check_result(call(named(b"Codegen"), b"load:", &[output]))
        .unwrap();
    let probe = host.global(&q, b"ReuseProbe").unwrap();
    call(globals, b"at:put:", &[key(b"ReuseMarker"), named(b"nil")]);
    drop(temporary);
    let rv = call(host.id.root, b"behaviorOf:", &[roots]);
    let retain = key(b"retain:");
    let add = key(b"addMethod");
    let original = call(rv, b"lookup", &[retain]);
    call(rv, b"addMethod", &[key(b"reuseOriginalRetain:"), original]);
    let replacement = method(b"retain");
    let run = key(b"run");
    let many = key(b"evict");
    let reset = key(b"reset:");
    let counts = key(b"counts");
    let remaining = key(b"remaining");
    let collect = key(b"collectPrecisely");
    let done = key(b"done");
    let raw = |r, s, a: &[Oop]| unsafe { host.id.send(r, s, a) };
    let stats = || {
        let values = q.keep(raw(hook, counts, &[]).unwrap()).unwrap();
        (0..4)
            .map(|i| as_size(host.send(values, b"at:", &[size(i).unwrap()]).unwrap()).unwrap())
            .collect::<Vec<_>>()
    };
    for selector in [run, run, many] {
        raw(hook, reset, &[size(0).unwrap()]).unwrap();
        raw(rv, add, &[retain, replacement]).unwrap();
        let result = raw(probe, selector, &[]);
        raw(rv, add, &[retain, original]).unwrap();
        assert_eq!(result, Ok(done));
        let values = stats();
        eprintln!(
            "root registrations: created={}, local={}, capture={}, checkpoints={}",
            values[0], values[1], values[2], values[3]
        );
        assert_eq!(values[0], if selector == many { 40 } else { 1 });
        assert_eq!(values[3], 1);
        raw(gc, collect, &[]).unwrap();
        assert_eq!(
            raw(hook, remaining, &[]).unwrap(),
            size(0).unwrap(),
            "private roots drain at call exit"
        );
    }
    for mode in [1, 2, 3] {
        raw(hook, reset, &[size(mode).unwrap()]).unwrap();
        raw(rv, add, &[retain, replacement]).unwrap();
        let result = raw(probe, run, &[]);
        raw(rv, add, &[retain, original]).unwrap();
        assert_eq!(
            result,
            Err(if mode == 1 {
                failure
            } else {
                birnam_abi::object_reference(host.id.invalid)
            })
        );
        let values = stats();
        assert_eq!(
            values[3], 0,
            "failed, zero or absent protection token must stop before the checkpoint; mode={mode}"
        );
        raw(gc, collect, &[]).unwrap();
        assert_eq!(raw(hook, remaining, &[]).unwrap(), size(0).unwrap());
    }
    // A failed call must not leave stale lifetime assumptions for its successor.
    raw(hook, reset, &[size(0).unwrap()]).unwrap();
    raw(rv, add, &[retain, replacement]).unwrap();
    let result = raw(probe, run, &[]);
    raw(rv, add, &[retain, original]).unwrap();
    assert_eq!(result, Ok(done));
    raw(gc, collect, &[]).unwrap();
    assert_eq!(raw(hook, remaining, &[]).unwrap(), size(0).unwrap());
    std::fs::remove_file(path).unwrap();
}
