//! Compiler values: frozen bytes, collection, replacement and exact failures.
use birnam_host_support::Context;
use std::{env, os::unix::ffi::OsStrExt, path::PathBuf};
#[path = "../src/test_hooks.rs"]
#[allow(dead_code)] // Other Host fixtures use the remaining callback helpers.
mod test_hooks;
use test_hooks::Hooks;

#[test]
#[ignore = "requires installed native compilers"]
fn native_compilers_are_live_value_seams() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = birnam_host::resolve_from_exe_dir(
        &lib.parent().unwrap().join("bin"),
        birnam_host::Overrides::default(),
    )
    .unwrap();
    let bootstrap = lib.join("libbirnam_bootstrap.dylib");
    let id = birnam_siphon::load(
        resources.id_dylib.as_os_str().as_bytes(),
        &[
            bootstrap.as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
        ],
    )
    .unwrap();
    let h = unsafe {
        let constructor = id.send(id.literal(2).unwrap(), id.intern, &[]).unwrap();
        let sequence = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let args = id
            .send(id.root, sequence, &[id.literal(1).unwrap()])
            .unwrap();
        let ns = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        Context::from_namespace(id, ns, resources, constructor).unwrap()
    };

    let fixtures = Hooks::new(&h);
    let q = h.scope();
    let no = h.named(b"false").unwrap();
    let nil = h.named(b"nil").unwrap();
    let gc = h.named(b"GC").unwrap();
    h.send(gc, b"precisePayloads:", &[birnam_abi::size(1).unwrap()])
        .unwrap();
    let compiler = h.global(&q, b"AlienCompiler").unwrap();
    let name = q.string(b"sample_function").unwrap();
    let signature = q.string(b"l l").unwrap();
    let source = q
        .call(compiler, b"compile:signature:", &[name, signature])
        .unwrap();
    assert!(
        q.string_bytes(source)
            .unwrap()
            .windows(b"__alien_target".len())
            .any(|s| s == b"__alien_target")
    );
    for (name, signature) in [
        (b"not;identifier".as_slice(), b"l l".as_slice()),
        (b"valid", b"{bad"),
    ] {
        let name = q.string(name).unwrap();
        let signature = q.string(signature).unwrap();
        let error = q
            .call(compiler, b"compile:signature:", &[name, signature])
            .unwrap();
        assert!(q.check_result(error).is_err());
    }
    // Collect once inside the authored compiler while its parser and builder
    // remain live; collecting on every character allocation is unnecessary.
    let builder_vt = h.named(b"StringBuilderVt").unwrap();
    let add = fixtures.symbol(b"add:");
    let builder_entry = fixtures.capture(builder_vt, add);
    let builder_hook = fixtures.context(&[builder_entry, gc, birnam_abi::size(0).unwrap()]);
    h.send(
        builder_vt,
        b"addMethod",
        &[add, fixtures.method(b"collectOnce", builder_hook)],
    )
    .unwrap();
    let aggregate_signature = q.string(b"{l3i} {l3i}").unwrap();
    let collected_source = q
        .call(
            compiler,
            b"compile:signature:",
            &[name, aggregate_signature],
        )
        .unwrap();
    h.send(builder_vt, b"addMethod", &[add, builder_entry])
        .unwrap();
    assert_eq!(fixtures.count(builder_hook, 2), 1);
    assert!(
        q.string_bytes(collected_source)
            .unwrap()
            .windows(5)
            .any(|s| s == b"__unm")
    );
    let format = h.global(&q, b"ImageFormat").unwrap();
    let fixture =
        include_bytes!("../../../tests/fixtures/portable-code-v1/image-mixed-stripped.bin");
    let bytes = q
        .call(
            h.named(b"Bytes").unwrap(),
            b"fromBytes:",
            &[q.byte_values(fixture).unwrap()],
        )
        .unwrap();
    let arch = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[q.word(1).unwrap()],
        )
        .unwrap();
    let abi = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[q.word(0xcdb6_41f9_7854_0d45).unwrap()],
        )
        .unwrap();
    let fields = q
        .call(format, b"inspect:architecture:abi:", &[bytes, arch, abi])
        .unwrap();
    let mut input = q.call(h.named(b"Vector").unwrap(), b"new", &[]).unwrap();
    for value in [arch, abi] {
        input = q.call(input, b"add:", &[value]).unwrap();
    }
    assert_eq!(q.vector_size(fields).unwrap(), 9);
    for i in 0..9 {
        let part = q.vector_at(fields, i).unwrap();
        input = q.call(input, b"add:", &[part]).unwrap();
    }
    let encoded = q.call(format, b"compose:", &[input]).unwrap();
    q.check_result(encoded).unwrap();
    assert_eq!(
        q.read_bytes(q.call(encoded, b"bytes", &[]).unwrap())
            .unwrap(),
        fixture
    );
    let truncated = q
        .call(
            h.named(b"Bytes").unwrap(),
            b"fromBytes:",
            &[q.byte_values(&fixture[..25]).unwrap()],
        )
        .unwrap();
    assert!(
        q.check_result(
            q.call(
                format,
                b"inspect:architecture:abi:",
                &[truncated, arch, abi]
            )
            .unwrap()
        )
        .is_err()
    );
    let refused = q
        .call(
            h.named(b"System").unwrap(),
            b"loadImageBytes:rebuild:",
            &[truncated, no],
        )
        .unwrap();
    assert_eq!(refused, nil);
    for (owner, selector, args) in [
        (
            compiler,
            b"compile:signature:".as_slice(),
            vec![name, signature],
        ),
        (format, b"inspect:architecture:abi:", vec![bytes, arch, abi]),
    ] {
        let vt = q.call(id.root, b"behaviorOf:", &[owner]).unwrap();
        let key = fixtures.symbol(selector);
        // The captured Method itself keeps its mapped code owner alive.
        let original = fixtures.capture(vt, key);
        let hook = fixtures.context(&[
            original,
            gc,
            birnam_abi::size(0).unwrap(),
            std::ptr::null_mut(),
        ]);
        h.send(
            vt,
            b"addMethod",
            &[key, fixtures.method(b"collectEvery", hook)],
        )
        .unwrap();
        q.call(owner, selector, &args).unwrap();
        assert_eq!(fixtures.count(hook, 2), 1);
        let failure = fixtures.fault(7071);
        let fault = fixtures.context(&[failure, birnam_abi::size(0).unwrap()]);
        h.send(vt, b"addMethod", &[key, fixtures.method(b"deny", fault)])
            .unwrap();
        let result = if owner == compiler {
            let sig = q.string(b"v").unwrap();
            unsafe {
                id.send(
                    h.named(b"Alien").unwrap(),
                    fixtures.symbol(b"callback:signature:"),
                    &[nil, sig],
                )
            }
        } else {
            unsafe {
                id.send(
                    h.named(b"System").unwrap(),
                    fixtures.symbol(b"loadImageBytes:rebuild:"),
                    &[truncated, no],
                )
            }
        };
        assert_eq!(result.unwrap_err(), failure);
        assert_eq!(fixtures.count(fault, 1), 1);
        h.send(vt, b"addMethod", &[key, original]).unwrap();
    }
}
