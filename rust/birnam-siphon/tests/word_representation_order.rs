//! Batch decline preserves live Word query order and left-side failure precedence.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Word owner"]
fn ordered_fallback_and_left_failure_precede_invalid_right_inputs() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let word = named(b"Word");
    let a = call(word, b"fromBits:", &[size(7).unwrap()]);
    let b = call(word, b"fromBits:", &[size(9).unwrap()]);
    let query = key(b"tryRepresentationIdentityOf:behavior:bytes:");
    let alias = key(b"wordOrderOriginalQuery:behavior:bytes:");
    let original = call(host.id.root, b"bind:to:", &[query, host.id.root]);
    let vt = call(host.id.root, b"behaviorOf:", &[host.id.root]);
    call(vt, b"addMethod", &[alias, original]);
    let path = std::env::temp_dir().join(format!("birnam-word-order-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/word-order.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let observations = call(host.id.root, b"sequence", &[std::ptr::null_mut(); 16]);
    let context = call(
        host.id.root,
        b"sequence",
        &[alias, observations, size(0).unwrap(), std::ptr::null_mut()],
    );
    let observer = call(
        call(fixture, b"methodAt:", &[key(b"observe")]),
        b"withContext:",
        &[context],
    );
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(812).unwrap(), size(19).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let at = key(b"at:");
    let put = key(b"at:put:");
    let add = key(b"wrappingAdd:");
    let bits = key(b"bits");
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    call(vt, b"addMethod", &[query, observer]);
    let answer = raw(a, add, &[b]).unwrap();
    q.keep(answer).unwrap();
    assert_eq!(raw(observations, at, &[size(0).unwrap()]).unwrap(), a);
    assert_eq!(raw(observations, at, &[size(1).unwrap()]).unwrap(), b);
    assert_eq!(as_size(raw(answer, bits, &[]).unwrap()), Some(16));
    raw(context, put, &[size(3).unwrap(), failure]).unwrap();
    for args in [
        vec![b],
        vec![size(9).unwrap()],
        vec![std::ptr::null_mut()],
        vec![],
    ] {
        raw(context, put, &[size(2).unwrap(), size(0).unwrap()]).unwrap();
        assert_eq!(raw(a, add, &args).unwrap_err(), failure);
        assert_eq!(
            raw(context, at, &[size(2).unwrap()]).unwrap(),
            size(1).unwrap(),
            "left validation must fail before reading or validating the right input"
        );
        assert_eq!(raw(observations, at, &[size(0).unwrap()]).unwrap(), a);
    }
    call(vt, b"addMethod", &[query, original]);
    call(vt, b"removeMethod:", &[alias]);
}
