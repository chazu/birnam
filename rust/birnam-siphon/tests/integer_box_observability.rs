//! Integer arithmetic preserves observable Word construction and conversion.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Integer policy"]
fn constructors_and_getters_observe_and_change_temporary_words() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let integer = named(b"Integer");
    let word = named(b"Word");
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let a = call(integer, b"fromWord:", &[size(7).unwrap()]);
    let b = call(integer, b"fromWord:", &[size(9).unwrap()]);
    let run = || call(a, b"+", &[b]);
    let read = |value| q.word_bits(call(value, b"word", &[])).unwrap();
    assert_eq!(read(run()), 16);
    let path =
        std::env::temp_dir().join(format!("birnam-integer-box-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/integer-box.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let original = call(host.id.root, b"bind:to:", &[key(b"fromBits:"), word]);
    let vt = named(b"WordVt");
    let alias = key(b"integerBoxOriginalFactory:");
    call(vt, b"addMethod", &[alias, original]);
    let seen = call(host.id.root, b"sequence", &[std::ptr::null_mut(); 16]);
    let context = call(
        host.id.root,
        b"sequence",
        &[alias, size(0).unwrap(), size(0).unwrap(), seen],
    );
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[context],
        )
    };
    let constructor = method(b"constructor");
    let getter = method(b"getter");
    let count = || as_size(call(context, b"at:", &[size(2).unwrap()])).unwrap();
    call(vt, b"addMethod", &[key(b"fromBits:"), constructor]);
    assert_eq!(read(run()), 16);
    assert_eq!(count(), 2, "both input Word factories remain current sends");
    // Collect before reading observations into the caller scope. The factory
    // owns the only retained references to these temporary Words.
    call(gc, b"collectPrecisely", &[]);
    let first = call(seen, b"at:", &[size(0).unwrap()]);
    let second = call(seen, b"at:", &[size(1).unwrap()]);
    assert_ne!(
        first, second,
        "observable temporary Words have distinct identity"
    );
    assert_eq!(q.word_bits(call(first, b"bits", &[])).unwrap(), 7);
    assert_eq!(q.word_bits(call(second, b"bits", &[])).unwrap(), 9);
    call(context, b"at:put:", &[size(1).unwrap(), size(1).unwrap()]);
    call(context, b"at:put:", &[size(2).unwrap(), size(0).unwrap()]);
    assert_eq!(
        read(run()),
        20,
        "constructor replacement changes arithmetic inputs"
    );
    assert_eq!(count(), 2);
    call(vt, b"addMethod", &[key(b"fromBits:"), original]);
    let bits = call(host.id.root, b"bind:to:", &[key(b"bits"), word]);
    call(context, b"at:put:", &[size(2).unwrap(), size(0).unwrap()]);
    call(vt, b"addMethod", &[key(b"bits"), getter]);
    assert_eq!(
        read(run()),
        99,
        "result conversion must enter the current Word getter"
    );
    assert_eq!(count(), 1);
    call(vt, b"addMethod", &[key(b"bits"), bits]);
    assert_eq!(read(run()), 16);
    call(vt, b"removeMethod:", &[alias]);
}
