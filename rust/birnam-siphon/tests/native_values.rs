//! Primitive value owners and explicit native adapters remain replaceable.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::path::Path;

#[test]
#[ignore = "requires production Oop modules"]
fn primitive_ownership_and_explicit_abi_use_live_id_messages() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |name: &[u8]| h.named(name).unwrap();
    let key = |name: &[u8]| hooks.symbol(name);
    let call = |r, name: &[u8], args: &[Oop]| q.call(r, name, args).unwrap();
    let bits = |value| q.word_bits(call(value, b"bits", &[])).unwrap();
    let word = named(b"Word");
    let bytes = named(b"Bytes");
    let execution = named(b"Execution");
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let w = |value| call(word, b"fromBits:", &[q.word(value).unwrap()]);
    let zero = w(0);
    let one = w(1);
    let max = w(usize::MAX);
    let sixty_four = w(64);
    assert_eq!(bits(call(max, b"wrappingAdd:", &[one])), 0);
    assert_eq!(bits(call(zero, b"wrappingSubtract:", &[one])), usize::MAX);
    assert_eq!(bits(call(max, b"shiftRight:", &[sixty_four])), 0);
    assert_eq!(call(max, b"unsignedLessThan:", &[one]), named(b"false"));
    assert!(h.send(one, b"unsignedDivide:", &[zero]).is_err());
    let rendered = call(max, b"printString", &[]);
    assert_eq!(q.string_bytes(rendered).unwrap(), b"0xffffffffffffffff");
    let error = call(max, b"asInteger", &[]);
    assert_eq!(call(error, b"kind", &[]), key(b"overflow"));
    let data = call(bytes, b"fromBytes:", &[q.byte_values(b"ab").unwrap()]);
    assert_eq!(
        as_size(call(data, b"byteAtWord:", &[size(1).unwrap()])),
        Some(b'b' as usize)
    );
    assert!(call(data, b"byteAtWord:", &[q.word(usize::MAX).unwrap()]).is_null());

    let path =
        std::env::temp_dir().join(format!("birnam-native-values-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/native-values.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let callback = call(module, b"methodAt:", &[key(b"callback")]);
    let callback = call(callback, b"withContext:", &[hooks.context(&[data, gc])]);
    let probe = call(named(b"Object"), b"clone", &[]);
    let behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[probe]);
    call(behavior, b"addMethod", &[key(b"byteCallback"), callback]);
    assert_eq!(
        call(
            execution,
            b"objectResultOf:selector:byteArgument:",
            &[probe, key(b"byteCallback"), data]
        ),
        data
    );
    assert_eq!(q.read_bytes(data).unwrap(), b"abchanged");

    let word_vt = call(h.id.root, b"behaviorOf:", &[word]);
    let selector = key(b"fromBits:");
    let original = call(word_vt, b"lookup", &[selector]);
    let context = hooks.context(&[original, gc, size(0).unwrap()]);
    call(
        word_vt,
        b"addMethod",
        &[selector, hooks.method(b"collectEvery", context)],
    );
    let result = call(
        execution,
        b"wordResultOf:selector:",
        &[named(b"Integer"), key(b"word")],
    );
    assert_eq!(bits(result), 0);
    assert_eq!(hooks.count(context, 2), 1);
    call(word_vt, b"addMethod", &[selector, original]);
    let snapshot = call(
        execution,
        b"bytesResultOf:selector:",
        &[data, key(b"bytes")],
    );
    call(data, b"appendByteValues", &[size(b'!' as usize).unwrap()]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(q.read_bytes(snapshot).unwrap(), b"abchanged");
    assert_eq!(q.read_bytes(data).unwrap(), b"abchanged!");
    let fault = hooks.fault(987);
    let failed = hooks.context(&[fault, size(0).unwrap()]);
    call(
        behavior,
        b"addMethod",
        &[key(b"abiFailure"), hooks.method(b"deny", failed)],
    );
    assert_eq!(
        unsafe {
            h.id.send(
                execution,
                key(b"wordResultOf:selector:"),
                &[probe, key(b"abiFailure")],
            )
        }
        .unwrap_err(),
        fault
    );
    assert_eq!(hooks.count(failed, 1), 1);
}
