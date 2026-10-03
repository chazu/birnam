//! Live compiler-to-backend seam: refusal, collection, replacement and outcomes.
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_host_support::{Context, object};
use std::{cell::Cell, env, ffi::c_void, os::unix::ffi::OsStrExt, path::PathBuf};
struct Hook {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn collect(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    if let Err(e) = unsafe { h.id.send_named(h.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    h.calls.set(h.calls.get() + 1);
    unsafe { h.original.method.unwrap()(h.original.context, r, a, n) }
}
unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(6, 4433)
}
#[test]
#[ignore = "requires installed NativeBackend; scripts/host-cli-test.sh"]
fn native_backend_is_the_live_compiler_seam() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = birnam_host::resolve_from_exe_dir(
        &lib.parent().unwrap().join("bin"),
        birnam_host::Overrides::default(),
    )
    .unwrap();
    let id = birnam_siphon::load(resources.id_dylib.as_os_str().as_bytes()).unwrap();
    let ns = object(
        unsafe {
            id.send(
                id.receiver,
                id.load,
                &[
                    Operand::bytes(lib.join("libbirnam_bootstrap.dylib").as_os_str().as_bytes()),
                    Operand::bytes(lib.as_os_str().as_bytes()),
                ],
            )
        }
        .unwrap(),
    )
    .unwrap();
    let h = Context::from_namespace(id, ns, resources).unwrap();
    let q = h.scope();
    let backend = h.named(b"NativeBackend").unwrap();
    let no = h.named(b"false").unwrap();
    let bad = q.string(b"not portable code").unwrap();
    let empty = q.string(b"").unwrap();
    let error = q
        .call(
            backend,
            b"emit:source:filename:persistent:synchronous:",
            &[
                Operand::object(bad),
                Operand::object(empty),
                Operand::object(empty),
                Operand::object(no),
                Operand::object(no),
            ],
        )
        .unwrap();
    assert!(q.check_result(error).is_err());
    assert!(q.call(backend, b"descriptorFor:", &[]).is_err());
    let selector =
        unsafe { id.intern_name(b"emit:source:filename:persistent:synchronous:") }.unwrap();
    let vt = h.named(b"NativeBackendVt").unwrap();
    let original = h
        .send(
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let mut hook = Hook {
        id,
        gc: h.named(b"GC").unwrap(),
        original,
        calls: Cell::new(0),
    };
    h.send(hook.gc, b"precisePayloads:", &[Operand::word(1)])
        .unwrap();
    h.send(
        vt,
        b"addMethod",
        &[
            Operand::object(selector),
            Operand::entry(NativeEntry::new(collect, (&mut hook as *mut Hook).cast())),
        ],
    )
    .unwrap();
    let source = q.string(b"[40 + 2]").unwrap();
    let result = q
        .call(
            h.named(b"Codegen").unwrap(),
            b"eval:",
            &[Operand::object(source)],
        )
        .unwrap();
    q.check_result(result).unwrap();
    assert_eq!(
        q.text(q.call(result, b"printString", &[]).unwrap())
            .unwrap(),
        "42"
    );
    assert!(
        hook.calls.get() > 0,
        "source compilation bypassed NativeBackend"
    );
    h.send(
        vt,
        b"addMethod",
        &[
            Operand::object(selector),
            Operand::entry(NativeEntry::new(fail, std::ptr::null_mut())),
        ],
    )
    .unwrap();
    let failed = h
        .send(
            h.named(b"Codegen").unwrap(),
            b"compile:",
            &[Operand::object(source)],
        )
        .unwrap_err();
    h.send(
        vt,
        b"addMethod",
        &[Operand::object(selector), Operand::entry(original)],
    )
    .unwrap();
    assert!(failed.0.contains("native status=6 detail=4433"), "{failed}");
    println!(
        "NativeBackend: semantic/arity refusal, collecting emission, live replacement and exact dependency outcome passed"
    );
}
