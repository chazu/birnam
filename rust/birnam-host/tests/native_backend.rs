//! Live compiler-to-backend calls: refusal, collection, replacement and failures.
use birnam_host_support::Context;
use std::{env, os::unix::ffi::OsStrExt, path::PathBuf};
#[path = "../src/test_hooks.rs"]
#[allow(dead_code)] // Other Host fixtures use the remaining callback helpers.
mod test_hooks;
use test_hooks::Hooks;

#[test]
#[ignore = "requires installed NativeBackend; scripts/host-cli-test.sh"]
fn native_backend_is_the_live_compiler_seam() {
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
    let backend = h.named(b"NativeBackend").unwrap();
    let no = h.named(b"false").unwrap();
    let bad = q.string(b"not portable code").unwrap();
    let empty = q.string(b"").unwrap();
    let error = q
        .call(
            backend,
            b"emit:source:filename:persistent:synchronous:",
            &[bad, empty, empty, no, no],
        )
        .unwrap();
    assert!(q.check_result(error).is_err());
    assert!(
        q.call(
            backend,
            b"emit:source:filename:persistent:synchronous:",
            &[]
        )
        .is_err()
    );
    // Codegen eval:/compile: use resumable emission without a source filename.
    let selector = fixtures.symbol(b"emitResumable:source:");
    let vt = q.call(id.root, b"behaviorOf:", &[backend]).unwrap();
    let original = fixtures.capture(vt, selector);
    let gc = h.named(b"GC").unwrap();
    h.send(gc, b"precisePayloads:", &[birnam_abi::size(1).unwrap()])
        .unwrap();
    let hook = fixtures.context(&[
        original,
        gc,
        birnam_abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    h.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"collectEvery", hook)],
    )
    .unwrap();
    let source = q.string(b"[40 + 2]").unwrap();
    let result = q
        .call(h.named(b"Codegen").unwrap(), b"eval:", &[source])
        .unwrap();
    q.check_result(result).unwrap();
    assert_eq!(
        q.text(q.call(result, b"printString", &[]).unwrap())
            .unwrap(),
        "42"
    );
    assert!(
        fixtures.count(hook, 2) == 1,
        "source compilation bypassed NativeBackend"
    );
    let failure = fixtures.fault(4433);
    let fault = fixtures.context(&[failure, birnam_abi::size(0).unwrap()]);
    h.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"deny", fault)],
    )
    .unwrap();
    let failed = h
        .send(h.named(b"Codegen").unwrap(), b"compile:", &[source])
        .unwrap_err();
    h.send(vt, b"addMethod", &[selector, original]).unwrap();
    assert!(failed.0.contains(&format!("{failure:p}")), "{failed}");
    assert_eq!(fixtures.count(fault, 1), 1);
    println!(
        "NativeBackend: semantic/arity refusal, collecting emission, live replacement and exact failure passed"
    );
}
