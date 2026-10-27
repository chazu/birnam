//! Generated calls retain inputs and exact aliases across collection and failure.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop modules and synchronous compiler"]
fn generated_inputs_survive_reentry_collection_failure_aliases_and_new_results() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(811).unwrap(), size(97).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            host.roots,
            std::ptr::null_mut(),
            size(0).unwrap(),
            failure,
        ],
    );
    let path = std::env::temp_dir().join(format!(
        "birnam-invocation-inputs-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/invocation-inputs.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let checkpoint = call(named(b"Object"), b"clone", &[]);
    let vt = call(host.id.root, b"ensurePrivateVtableFor:", &[checkpoint]);
    for selector in [
        b"check:with:".as_slice(),
        b"fail:with:",
        b"allocation:bytes:identity:",
    ] {
        let method = call(
            call(fixture, b"methodAt:", &[key(selector)]),
            b"withContext:",
            &[context],
        );
        call(vt, b"addMethod", &[key(selector), method]);
    }
    call(
        named(b"Globals"),
        b"at:put:",
        &[key(b"InputCheckpoint"), checkpoint],
    );
    let source=br#"let probe := [Object clone];
      [Reflect defineMethod: #echo: on: probe as: [Execution block: [:self :value | [InputCheckpoint check: self with: value]; value] withNativeSignature: "oO"]];
      [Reflect defineMethod: #nested: on: probe as: [Execution block: [:self :value | [self echo: value]] withNativeSignature: "oO"]];
      [Reflect defineMethod: #fresh: on: probe as: [Execution block: [:self :value | let result := [Execution wordResultOf: value selector: #word]; [InputCheckpoint check: self with: result]; result] withNativeSignature: "oO"]];
      [Reflect defineMethod: #failure: on: probe as: [Execution block: [:self :value | [InputCheckpoint fail: self with: value]] withNativeSignature: "oO"]];
      [Globals at: #InputScopeProbe put: probe]; true"#;
    let text = q.string(source).unwrap();
    let path =
        std::env::temp_dir().join(format!("birnam-input-scope-{}.dylib", std::process::id()));
    let output = q.string(path.as_os_str().as_encoded_bytes()).unwrap();
    let result = call(
        named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[text, output, output, named(b"true")],
    );
    q.check_result(result).unwrap();
    assert_ne!(result, named(b"nil"));
    q.check_result(call(named(b"Codegen"), b"load:", &[output]))
        .unwrap();
    let probe = host.global(&q, b"InputScopeProbe").unwrap();
    for selector in [b"echo:".as_slice(), b"nested:", b"fresh:", b"failure:"] {
        // The checkpoint releases this sole external token during the call.
        let value = host
            .send(named(b"Integer"), b"fromWord:", &[size(123).unwrap()])
            .unwrap();
        let token = host.send(host.roots, b"retain:", &[value]).unwrap();
        call(context, b"at:put:", &[size(2).unwrap(), token]);
        let result = unsafe { host.id.send(probe, key(selector), &[value]) };
        // Retain a successful return before making another message send.
        let output_token = result
            .ok()
            .map(|value| host.send(host.roots, b"retain:", &[value]).unwrap());
        assert!(
            call(context, b"at:", &[size(2).unwrap()]).is_null(),
            "callback must release the external token"
        );
        if selector == b"failure:" {
            assert_eq!(result.unwrap_err(), failure);
        } else {
            let result = result.unwrap();
            let token = output_token.unwrap();
            if selector == b"fresh:" {
                assert_eq!(
                    q.word_bits(host.send(result, b"bits", &[]).unwrap())
                        .unwrap(),
                    123
                );
            } else {
                assert_eq!(result, value);
            }
            call(gc, b"collectPrecisely", &[]);
            assert!(
                as_size(host.send(host.id.root, b"identityOf:", &[result]).unwrap()).unwrap() > 0
            );
            host.send(host.roots, b"release:", &[token]).unwrap();
        }
    }
    let original = call(host.id.root, b"bind:to:", &[key(b"echo:"), probe]);
    let replacement = call(host.id.root, b"bind:to:", &[key(b"failure:"), probe]);
    let vt = call(host.id.root, b"behaviorOf:", &[probe]);
    call(vt, b"addMethod", &[key(b"echo:"), replacement]);
    let value = call(named(b"Integer"), b"fromWord:", &[size(55).unwrap()]);
    let args = call(call(named(b"Vector"), b"new", &[]), b"add:", &[value]);
    assert_eq!(
        call(original, b"invokeOn:arguments:", &[probe, args]),
        value
    );
    assert_eq!(
        unsafe { host.id.send(probe, key(b"echo:"), &[value]) }.unwrap_err(),
        failure
    );
    // A required object is a non-null Oop, including a supported immediate.
    let immediate = size(1).unwrap();
    let args = call(host.id.root, b"sequence", &[immediate]);
    assert_eq!(
        call(original, b"invokeOn:arguments:", &[probe, args]),
        immediate
    );
    let calls = as_size(call(context, b"at:", &[size(3).unwrap()])).unwrap();
    assert!(
        host.send(probe, b"nested:", &[std::ptr::null_mut()])
            .is_err()
    );
    assert_eq!(
        as_size(call(context, b"at:", &[size(3).unwrap()])).unwrap(),
        calls,
        "required non-null input must be rejected before the body"
    );
    std::fs::remove_file(path).unwrap();
}
