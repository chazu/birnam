//! Generated calls observe current owners and their effects across repeated runs.
use super::*;
use std::path::Path;
pub(super) fn check(h: &H<'_>) {
    h.reclaimed();
    let q = h.host.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-compiler-dispatch-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/compiler-dispatch.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let selected = |r, n: &[u8]| h.call(h.host.id.root, b"bind:to:", &[h.key(n), r]);
    let set = |vt, n: &[u8], m| {
        h.call(vt, b"addMethod", &[h.key(n), m]);
    };
    let method = |n: &[u8], c| {
        q.call(
            q.call(fixture, b"methodAt:", &[h.key(n)]).unwrap(),
            b"withContext:",
            &[c],
        )
        .unwrap()
    };
    let run = |block| {
        let drive = h.host.scope();
        let machine = drive.call(h.named(b"Machine"), b"new", &[]).unwrap();
        h.keep(drive.call(machine, b"runBlock:", &[block]).unwrap())
    };
    // Guard the removed raw-entry constructor as well as ordinary call results.
    let method_vt = h.call(h.host.id.root, b"methodBehavior", &[]);
    let forbidden = h.key(b"fromNative:owner:");
    assert!(h.call(method_vt, b"lookup", &[forbidden]).is_null());
    let candidate_calls = h.hooks.context(&[h.hooks.fault(91), size(0).unwrap()]);
    set(
        method_vt,
        b"fromNative:owner:",
        h.hooks.method(b"deny", candidate_calls),
    );
    let block = h.block(h.compile(b"let compiledAnswer := [20 + 22]; compiledAnswer"));
    h.publish(b"compiledAnswer", h.named(b"nil"));
    let integer = h.named(b"Integer");
    let integer_vt = h.vt(integer);
    let factory_original = selected(integer, b"fromWord:");
    let factory = h.hooks.context(&[
        factory_original,
        h.named(b"GC"),
        size(0).unwrap(),
        size(0).unwrap(),
    ]);
    set(integer_vt, b"fromWord:", method(b"forward", factory));
    assert_eq!(h.integer(run(block)), 42);
    let cold_calls = h.hooks.count(factory, 2);
    assert!(cold_calls > 0);
    for _ in 0..40 {
        assert_eq!(h.integer(run(block)), 42);
    }
    h.hooks.put(factory, 2, size(0).unwrap());
    assert_eq!(h.integer(run(block)), 42);
    assert_eq!(
        h.hooks.count(factory, 2),
        cold_calls,
        "warm execution preserves constructor effects"
    );
    set(integer_vt, b"fromWord:", factory_original);
    let value = h.call(integer, b"fromWord:", &[size(20).unwrap()]);
    let value_vt = h.vt(value);
    let plus = selected(value, b"+");
    set(value_vt, b"+", method(b"first", h.named(b"nil")));
    assert_eq!(h.integer(run(block)), 22);
    set(value_vt, b"+", plus);
    let convert_original = selected(integer, b"wordOf:");
    let converter = h.hooks.context(&[
        convert_original,
        h.named(b"GC"),
        size(0).unwrap(),
        size(1).unwrap(),
    ]);
    set(integer_vt, b"wordOf:", method(b"forward", converter));
    let result = run(block);
    assert!(
        h.hooks.count(converter, 2) > 0,
        "generated computation observes the current conversion before the test reads the result"
    );
    assert_eq!(h.integer(result), 42);
    set(integer_vt, b"wordOf:", convert_original);
    let machine = h.named(b"Machine");
    let machine_vt = h.vt(machine);
    let reader_original = selected(machine, b"oopAt:");
    let reader = h.hooks.context(&[
        reader_original,
        h.named(b"GC"),
        size(0).unwrap(),
        size(1).unwrap(),
    ]);
    set(machine_vt, b"oopAt:", method(b"forward", reader));
    assert_eq!(h.integer(run(block)), 42);
    assert!(h.hooks.count(reader, 2) > 0);
    set(machine_vt, b"oopAt:", reader_original);
    assert_eq!(h.integer(run(block)), 42);
    h.reclaimed();
    let source = format!("{}42", "[1 + 2]; ".repeat(64));
    let block = h.block(h.compile(source.as_bytes()));
    assert_eq!(h.integer(run(block)), 42);
    h.reclaimed();
    let block = h.eval(b"let candidateCapture := 35; [ | let sum := [candidateCapture + 7]; sum]");
    for _ in 0..40 {
        assert_eq!(h.integer(run(block)), 42);
    }
    h.reclaimed();
    assert_eq!(
        h.hooks.count(candidate_calls, 1),
        0,
        "ordinary compilation must not reconstruct raw-entry candidates"
    );
    h.call(method_vt, b"removeMethod:", &[forbidden]);
}
