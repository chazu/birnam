//! Direct Word definitions retain entered code and observe live captures and bindings.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{size, Oop};
#[test]
#[ignore = "requires production Oop compiler and Word owner"]
fn authored_word_helpers_preserve_current_and_entered_method_meanings() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let key = |n: &[u8]| hooks.symbol(n);
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let source=br#"[Module named: #wordHelperProbe requires: #[]]; let mask := [Execution wordResultOf: 15 selector: #word]; let probe := [Object clone]; [Reflect defineMethod: #compute:with: on: probe as: [:self :a :b | let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; [[sum bitXor: mask] wrappingSubtract: sum]]]; [Globals at: #WordHelperProbe put: probe]; true"#;
    let path =
        std::env::temp_dir().join(format!("birnam-word-helper-{}.dylib", std::process::id()));
    let output = q.string(path.as_os_str().as_encoded_bytes()).unwrap();
    q.check_result(call(
        named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[q.string(source).unwrap(), output, output, named(b"true")],
    ))
    .unwrap();
    q.check_result(call(named(b"Codegen"), b"load:", &[output]))
        .unwrap();
    let probe = host.global(&q, b"WordHelperProbe").unwrap();
    let word = named(b"Word");
    let vt = named(b"WordVt");
    let a = call(word, b"fromBits:", &[size(10).unwrap()]);
    let b = call(word, b"fromBits:", &[size(6).unwrap()]);
    let run = || q.word_bits(call(probe, b"compute:with:", &[a, b])).unwrap();
    let fixture_path = path.with_extension("fixture.dylib");
    assert!(std::process::Command::new("clang")
        .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/oop-host/word-helper.c")
        )
        .arg("-o")
        .arg(&fixture_path)
        .status()
        .unwrap()
        .success());
    let fixture = host.load_native(&q, &fixture_path, &[]).unwrap();
    std::fs::remove_file(fixture_path).unwrap();
    let method = |n: &[u8], c| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[c],
        )
    };
    let set = |v, n: &[u8], m| {
        call(v, b"addMethod", &[key(n), m]);
    };
    let plan = call(vt, b"lookup", &[key(b"tryPrimitiveExpressionAtEpoch:")]);
    let planned = hooks.context(&[plan, size(0).unwrap()]);
    set(
        vt,
        b"tryPrimitiveExpressionAtEpoch:",
        hooks.method(b"counted", planned),
    );
    assert_eq!(run(), 3);
    eprintln!(
        "automatic Word helper plan calls={}",
        hooks.count(planned, 1)
    );
    let selected = call(host.id.root, b"bind:to:", &[key(b"compute:with:"), probe]);
    let implementation = call(named(b"Execution"), b"bodyOfMethod:", &[selected]);
    q.check_result(implementation).unwrap();
    assert_ne!(implementation, named(b"nil"));
    let body_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[implementation]);
    let value = call(body_vt, b"lookup", &[key(b"value:value:value:")]);
    let called = hooks.context(&[size(0).unwrap()]);
    let argument = method(b"argument", called);
    set(body_vt, b"value:value:value:", argument);
    assert_eq!(
        run(),
        3,
        "a direct Method must not forward through Block value messages"
    );
    assert_eq!(hooks.count(called, 0), 0);
    set(body_vt, b"value:value:value:", value);
    let capture = call(body_vt, b"lookup", &[key(b"captureWordAt:")]);
    let projected = hooks.context(&[capture, size(0).unwrap()]);
    set(
        body_vt,
        b"captureWordAt:",
        hooks.method(b"counted", projected),
    );
    assert_eq!(run(), 3);
    assert_eq!(hooks.count(projected, 1), 1);
    set(body_vt, b"captureWordAt:", capture);
    let xor = call(vt, b"lookup", &[key(b"bitXor:")]);
    hooks.put(called, 0, size(0).unwrap());
    set(vt, b"bitXor:", method(b"receiver", called));
    assert_eq!(run(), 0);
    assert_eq!(hooks.count(called, 0), 2);
    set(vt, b"bitXor:", xor);
    // Replacing the binding while its capture is being read affects the next send,
    // never the already entered Method. Collection cannot change that meaning.
    call(
        body_vt,
        b"addMethod",
        &[key(b"helperOriginalCapture:"), capture],
    );
    let target_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
    let during = hooks.context(&[size(0).unwrap(), gc, target_vt, argument]);
    hooks.put(called, 0, size(0).unwrap());
    set(body_vt, b"captureWordAt:", method(b"capture", during));
    assert_eq!(
        run(),
        3,
        "entered body must finish using its selected implementation"
    );
    assert_eq!(hooks.count(during, 0), 1);
    assert_eq!(hooks.count(called, 0), 0);
    assert_eq!(run(), 10, "the next helper send must enter the replacement");
    assert_eq!(hooks.count(called, 0), 1);
    set(target_vt, b"compute:with:", selected);
    set(body_vt, b"captureWordAt:", capture);
    set(body_vt, b"value:value:value:", value);
    call(gc, b"budget:", &[size(1).unwrap()]);
    assert_eq!(run(), 3);
    set(vt, b"tryPrimitiveExpressionAtEpoch:", plan);
    std::fs::remove_file(path).unwrap();
}
