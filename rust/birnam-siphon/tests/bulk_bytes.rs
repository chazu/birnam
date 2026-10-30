//! Whole-buffer consumers never fall back to indexed byte sends.
#[allow(dead_code)]
#[path = "support/minimal.rs"]
mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop modules"]
fn bulk_bytes_preserve_data_lifetimes_and_current_operations() {
    let h = minimal::native_floor();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = h.named(b"GC").unwrap();
    let bytes = h.named(b"Bytes").unwrap();
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let path = std::env::temp_dir().join(format!("birnam-bulk-bytes-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/bulk-bytes.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let provider = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[provider]);
    let state = hooks.context(&[
        size(0).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        std::ptr::null_mut(),
        size(0).unwrap(),
        gc,
        bytes,
        q.byte_values(b"nested\0\xff").unwrap(),
    ]);
    for selector in [
        b"sizeWord".as_slice(),
        b"copyByteValuesFrom:count:to:",
        b"atWord:",
    ] {
        let method = call(
            call(module, b"methodAt:", &[hooks.symbol(selector)]),
            b"withContext:",
            &[state],
        );
        call(behavior, b"addMethod", &[hooks.symbol(selector), method]);
    }
    for n in [0, 1, 8192, 8193, 1 << 20] {
        hooks.put(state, 0, size(n).unwrap());
        hooks.put(state, 1, size(0).unwrap());
        let value = call(bytes, b"fromBytes:", &[provider]);
        assert_eq!(
            q.read_bytes(value).unwrap(),
            (0..n).map(|i| ((i ^ 165) & 255) as u8).collect::<Vec<_>>()
        );
        assert_eq!(hooks.count(state, 1), usize::from(n != 0));
        assert_eq!(hooks.count(state, 2), 0);
        if n != 0 {
            assert!(
                h.send(
                    hooks.field(state, 3),
                    b"appendByteValues",
                    &[size(1).unwrap()]
                )
                .is_err(),
                "a retained sink expires when the read returns"
            );
        }
    }
    hooks.put(state, 0, size(8193).unwrap());
    hooks.put(state, 4, size(3).unwrap());
    let copied = call(bytes, b"fromBytes:", &[provider]);
    assert_eq!(
        q.read_bytes(copied).unwrap(),
        (0..8193)
            .map(|i| ((i ^ 165) & 255) as u8)
            .collect::<Vec<_>>()
    );
    for flags in [4, 8, 16, 32, 64] {
        hooks.put(state, 4, size(flags).unwrap());
        assert!(
            h.send(bytes, b"fromBytes:", &[provider]).is_err(),
            "flags {flags} must reject incomplete, invalid, ignored-failure or oversized data"
        );
        assert!(
            h.send(
                hooks.field(state, 3),
                b"appendByteValues",
                &[size(1).unwrap()]
            )
            .is_err()
        );
    }
    hooks.put(state, 4, size(0).unwrap());
    call(gc, b"automaticCollection:", &[size(0).unwrap()]);
    let before = as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    call(bytes, b"fromBytes:", &[provider]);
    assert_eq!(as_size(call(h.id.root, b"bindingEpoch", &[])), Some(before));
    call(gc, b"automaticCollection:", &[size(1).unwrap()]);
    // Name, Symbol, Sequence, Bytes and String all provide grouped data.
    let sequence = q.byte_values(b"abc").unwrap();
    let name = call(
        h.id.root,
        b"nameWithBytes",
        &[size(97).unwrap(), size(98).unwrap(), size(99).unwrap()],
    );
    let symbol = hooks.symbol(b"abc");
    let string = q.string(b"abc").unwrap();
    for source in [
        sequence,
        name,
        symbol,
        string,
        call(bytes, b"fromBytes:", &[sequence]),
    ] {
        assert_eq!(
            q.read_bytes(call(bytes, b"fromBytes:", &[source])).unwrap(),
            b"abc"
        );
    }
    // Producers release owner borrows before callbacks and preserve one snapshot.
    let original: Vec<_> = (0..8193).map(|i| (i & 255) as u8).collect();
    for sequence_source in [false, true] {
        let sequence = q.byte_values(&original).unwrap();
        let source = if sequence_source {
            sequence
        } else {
            call(bytes, b"fromBytes:", &[sequence])
        };
        let sink = call(h.named(b"Object").unwrap(), b"clone", &[]);
        let sink_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[sink]);
        let state = hooks.context(&[
            source,
            size(8193).unwrap(),
            size(0).unwrap(),
            size(usize::from(sequence_source)).unwrap(),
            gc,
        ]);
        let entry = call(
            call(module, b"methodAt:", &[hooks.symbol(b"appendByteValues")]),
            b"withContext:",
            &[state],
        );
        call(
            sink_vt,
            b"addMethod",
            &[hooks.symbol(b"appendByteValues"), entry],
        );
        call(
            source,
            b"copyByteValuesFrom:count:to:",
            &[size(0).unwrap(), size(8193).unwrap(), sink],
        );
        assert_eq!(hooks.count(state, 2), 8193);
    }
}
