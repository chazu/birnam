//! Byte arguments/results are copied before reentrant target/factory callbacks.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop outbound adapters"]
fn typed_byte_and_discard_calls_preserve_copies_factories_and_failures() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    let roots = host.roots;
    let bytes = named(b"Bytes");
    let execution = named(b"Execution");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let behavior = call(named(b"ObjectVt"), b"delegated", &[]);
    let target = call(behavior, b"allocate", &[size(0).unwrap()]);
    let factory = call(behavior, b"allocate", &[size(0).unwrap()]);
    let path =
        std::env::temp_dir().join(format!("birnam-byte-adapters-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/byte-adapters.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let data = |b: &[u8]| call(bytes, b"fromBytes:", &[q.byte_values(b).unwrap()]);
    let read = |o| q.read_bytes(call(o, b"bytes", &[])).unwrap();
    let fault = |status, detail| {
        let f = unsafe {
            host.id.send(
                execution,
                key(b"failNative:detail:"),
                &[size(status).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(f).unwrap()
    };
    let target_failure = fault(821, 71);
    let factory_failure = fault(822, 73);
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let z = size(0).unwrap();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            roots,
            bytes,
            monitor,
            z,
            std::ptr::null_mut(),
            z,
            z,
            z,
            data(b"changed-buffer"),
            target_failure,
            factory_failure,
            nil,
            data(b"ignored"),
        ],
    );
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[context],
        )
    };
    for n in [
        b"consume:".as_slice(),
        b"fromBytes:",
        b"discard:",
        b"record:",
    ] {
        call(behavior, b"addMethod", &[key(n), method(n)]);
    }
    call(
        monitor_vt,
        b"addMethod",
        &[
            key(b"allocation:bytes:identity:"),
            method(b"allocation:bytes:identity:"),
        ],
    );
    let at = key(b"at:");
    let put = key(b"at:put:");
    let retain = key(b"retain:");
    let identity = key(b"identityOf:");
    let from_bytes = key(b"fromBytes:");
    let consume = key(b"consume:");
    let bytes_selector = key(b"bytes");
    let discard = key(b"discard:");
    let record = key(b"record:");
    let object_adapter = key(b"objectResultOf:selector:byteArgument:");
    let byte_adapter = key(b"bytesResultOf:selector:resultFactory:");
    let discard_adapter = key(b"discardResultOf:selector:wordArgument:");
    let ab = q.byte_values(b"ab").unwrap();
    let xy = q.byte_values(b"xy").unwrap();
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let set = |i, v| {
        raw(context, put, &[size(i).unwrap(), v]).unwrap();
    };
    let arm = |source, mode| {
        let input = raw(bytes, from_bytes, &[source]).unwrap();
        let token = raw(roots, retain, &[input]).unwrap();
        let id = raw(host.id.root, identity, &[input]).unwrap();
        set(4, id);
        set(5, token);
        set(7, size(mode).unwrap());
        input
    };
    for mode in 0..=4 {
        let input = arm(ab, mode);
        let result = raw(execution, object_adapter, &[target, consume, input]);
        match mode {
            0 => {
                let returned = q.keep(result.unwrap()).unwrap();
                assert_eq!(returned, input);
                assert_eq!(read(returned), b"abchanged-buffer");
            }
            1 | 2 => assert_eq!(result.unwrap(), nil),
            3 => assert_eq!(result.unwrap_err(), target_failure),
            _ => assert_eq!(
                result.unwrap(),
                size(9).unwrap(),
                "an immediate result remains an Oop"
            ),
        }
        assert!(
            raw(context, at, &[size(5).unwrap()]).unwrap().is_null(),
            "callback releases external input protection"
        );
    }
    for mode in [0, 3] {
        let input = arm(xy, mode);
        let result = raw(execution, byte_adapter, &[input, bytes_selector, factory]);
        if mode == 0 {
            let result = q.keep(result.unwrap()).unwrap();
            assert_eq!(read(result), b"xy");
        } else {
            assert_eq!(result.unwrap_err(), factory_failure);
        }
        assert!(raw(context, at, &[size(5).unwrap()]).unwrap().is_null());
    }
    assert_eq!(
        as_size(raw(context, at, &[size(6).unwrap()]).unwrap()),
        Some(7),
        "all mutation callbacks must run"
    );
    let value = call(named(b"Word"), b"fromBits:", &[size(5).unwrap()]);
    assert_eq!(
        raw(execution, discard_adapter, &[target, discard, value]).unwrap(),
        nil
    );
    for adapter in [object_adapter, byte_adapter, discard_adapter] {
        let args = [target, consume, value];
        for count in 0..3 {
            assert_eq!(
                raw(execution, adapter, &args[..count]).unwrap_err(),
                birnam_abi::object_reference(host.id.invalid)
            );
        }
        let mut extra = args.to_vec();
        extra.push(nil);
        assert!(raw(execution, adapter, &extra).is_err());
        for index in 0..3 {
            let mut bad = args;
            bad[index] = std::ptr::null_mut();
            assert_eq!(
                raw(execution, adapter, &bad).unwrap_err(),
                birnam_abi::object_reference(host.id.invalid)
            );
        }
    }
    assert!(
        raw(
            execution,
            byte_adapter,
            &[named(b"Word"), key(b"bits"), factory]
        )
        .is_err(),
        "byte conversion requires the owner's byte protocol"
    );
    assert_eq!(
        raw(context, at, &[size(6).unwrap()]).unwrap(),
        size(7).unwrap(),
        "invalid byte results must fail before the factory is entered"
    );
    for (boolean, expected) in [(named(b"true"), 1), (named(b"false"), 0)] {
        assert_eq!(
            call(
                named(b"Boolean"),
                b"sendWordOf:to:selector:",
                &[boolean, target, record]
            ),
            target
        );
        assert_eq!(
            raw(context, at, &[size(8).unwrap()]).unwrap(),
            size(expected).unwrap()
        );
    }
    let snapshot_source = data(b"policy snapshot");
    let snapshot = call(
        bytes,
        b"fromByteResultOf:selector:",
        &[snapshot_source, bytes_selector],
    );
    call(snapshot_source, b"appendBytes:", &[data(b" changed")]);
    assert_eq!(read(snapshot), b"policy snapshot");
    let integer = named(b"Integer");
    let zero = call(
        integer,
        b"fromWordResultOf:selector:",
        &[integer, key(b"word")],
    );
    assert_eq!(q.word_bits(call(zero, b"word", &[])).unwrap(), 0);
    for old in [
        b"sendNativeTo:selector:arguments:signature:resultFactory:".as_slice(),
        b"tryNativeArguments:",
    ] {
        assert!(
            call(host.id.root, b"bind:to:", &[key(old), execution]).is_null(),
            "retired general adapter must not be reintroduced"
        );
    }
}
