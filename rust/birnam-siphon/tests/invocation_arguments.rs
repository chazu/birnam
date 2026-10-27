//! Reentrant calls preserve Oop argument arrays and explicit nullable adaptation.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop compiler and Execution"]
fn invocation_preserves_nullable_policy_and_reentrant_arguments() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let receiver = call(named(b"Object"), b"clone", &[]);
    let vt = call(host.id.root, b"ensurePrivateVtableFor:", &[receiver]);
    let selector = key(b"packet:with:bytes:zero:");
    let inspect = key(b"inspect:with:bytes:zero:");
    let data = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(b"unchanged").unwrap()],
    );
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            receiver,
            selector,
            size(0).unwrap(),
            size(0).unwrap(),
            data,
        ],
    );
    let path = std::env::temp_dir().join(format!(
        "birnam-invocation-arguments-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/invocation-arguments.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = call(
        call(fixture, b"methodAt:", &[key(b"inspect")]),
        b"withContext:",
        &[context],
    );
    call(vt, b"addMethod", &[selector, method]);
    call(vt, b"addMethod", &[inspect, method]);
    let globals = named(b"Globals");
    let global = key(b"InvocationProbe");
    call(globals, b"at:put:", &[global, receiver]);
    for authored in [false, true] {
        if authored {
            let source=q.string(br#"[Reflect defineMethod: #packet:with:bytes:zero: on: InvocationProbe as: [Execution block: [:self :first :other :data :zero | [self inspect: first with: other bytes: data zero: zero]] withNativeSignature: "ooooo"]]"#).unwrap();
            q.check_result(call(named(b"Codegen"), b"eval:", &[source]))
                .unwrap();
        }
        for first in [receiver, std::ptr::null_mut(), size(13).unwrap(), nil] {
            let arguments = [first, receiver, data, size(0).unwrap()];
            let before = arguments;
            let result = unsafe { host.id.send(receiver, selector, &arguments) }.unwrap();
            let expected = if authored && first.is_null() {
                nil
            } else {
                first
            };
            assert_eq!(result, expected);
            assert_eq!(
                arguments, before,
                "nested entry must not overwrite caller-owned arguments"
            );
            assert_eq!(q.read_bytes(data).unwrap(), b"unchanged");
        }
    }
    assert_eq!(
        as_size(call(context, b"at:", &[size(4).unwrap()])),
        Some(16),
        "each outer invocation must reenter the same native checkpoint"
    );
    call(vt, b"removeMethod:", &[selector]);
    call(vt, b"removeMethod:", &[inspect]);
    call(globals, b"remove:", &[global]);
}
