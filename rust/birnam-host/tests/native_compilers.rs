//! Real value seams: frozen bytes, collection, replacement and exact failures.
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
    once: bool,
}
unsafe extern "C" fn collect(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    if !h.once || h.calls.get() == 0 {
        if let Err(e) = unsafe { h.id.send_named(h.gc, b"collectPrecisely", &[]) } {
            return e;
        }
        h.calls.set(h.calls.get() + 1);
    }
    unsafe { h.original.method.unwrap()(h.original.context, r, a, n) }
}
unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(6, 7071)
}
#[test]
#[ignore = "requires installed native compilers"]
fn native_compilers_are_live_value_seams() {
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
    let no = h.named(b"false").unwrap();
    let nil = h.named(b"nil").unwrap();
    h.send(
        h.named(b"GC").unwrap(),
        b"precisePayloads:",
        &[Operand::word(1)],
    )
    .unwrap();
    let compiler = h.global(&q, b"AlienCompiler").unwrap();
    let name = q.string(b"sample_function").unwrap();
    let signature = q.string(b"l l").unwrap();
    let source = q
        .call(
            compiler,
            b"compile:signature:",
            &[Operand::object(name), Operand::object(signature)],
        )
        .unwrap();
    assert!(
        q.string_bytes(source)
            .unwrap()
            .windows(b"__alien_target".len())
            .any(|s| s == b"__alien_target")
    );
    for (name, sig) in [
        (b"not;identifier".as_slice(), b"l l".as_slice()),
        (b"valid", b"{bad"),
    ] {
        let name = q.string(name).unwrap();
        let sig = q.string(sig).unwrap();
        let error = q
            .call(
                compiler,
                b"compile:signature:",
                &[Operand::object(name), Operand::object(sig)],
            )
            .unwrap();
        assert!(q.check_result(error).is_err());
    }
    // Force one collection inside the authored compiler, while its parser and
    // output builder are live. Avoid collecting on every character allocation.
    let builder_vt = h.named(b"StringBuilderVt").unwrap();
    let add = unsafe { id.intern_name(b"add:") }.unwrap();
    let builder_entry = h
        .send(
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(add), Operand::object(builder_vt)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let mut builder_hook = Hook {
        id,
        gc: h.named(b"GC").unwrap(),
        original: builder_entry,
        calls: Cell::new(0),
        once: true,
    };
    h.send(
        builder_vt,
        b"addMethod",
        &[
            Operand::object(add),
            Operand::entry(NativeEntry::new(
                collect,
                (&mut builder_hook as *mut Hook).cast(),
            )),
        ],
    )
    .unwrap();
    let aggregate_signature = q.string(b"{l3i} {l3i}").unwrap();
    let collected_source = q
        .call(
            compiler,
            b"compile:signature:",
            &[Operand::object(name), Operand::object(aggregate_signature)],
        )
        .unwrap();
    h.send(
        builder_vt,
        b"addMethod",
        &[Operand::object(add), Operand::entry(builder_entry)],
    )
    .unwrap();
    assert_eq!(builder_hook.calls.get(), 1);
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
            &[Operand::bytes(fixture)],
        )
        .unwrap();
    let arch = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[Operand::word(1)],
        )
        .unwrap();
    let abi = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[Operand::word(0xcdb6_41f9_7854_0d45)],
        )
        .unwrap();
    let fields = q
        .call(
            format,
            b"inspect:architecture:abi:",
            &[
                Operand::object(bytes),
                Operand::object(arch),
                Operand::object(abi),
            ],
        )
        .unwrap();
    let mut input = q.call(h.named(b"Vector").unwrap(), b"new", &[]).unwrap();
    for v in [arch, abi] {
        input = q.call(input, b"add:", &[Operand::object(v)]).unwrap();
    }
    for i in 0..8 {
        let index = q
            .call(
                h.named(b"Integer").unwrap(),
                b"fromWord:",
                &[Operand::word(i)],
            )
            .unwrap();
        let part = q.call(fields, b"at:", &[Operand::object(index)]).unwrap();
        input = q.call(input, b"add:", &[Operand::object(part)]).unwrap();
    }
    let encoded = q
        .call(format, b"compose:", &[Operand::object(input)])
        .unwrap();
    let borrowed = h.send(encoded, b"bytes", &[]).unwrap().as_bytes().unwrap();
    assert_eq!(
        unsafe { birnam_abi::borrowed_bytes(borrowed.pointer, borrowed.length) }.unwrap(),
        fixture
    );
    let truncated = q
        .call(
            h.named(b"Bytes").unwrap(),
            b"fromBytes:",
            &[Operand::bytes(&fixture[..25])],
        )
        .unwrap();
    assert!(
        q.check_result(q.call(
            format,
            b"inspect:architecture:abi:",
            &[
                Operand::object(truncated),
                Operand::object(arch),
                Operand::object(abi)
            ]
        ).unwrap())
        .is_err()
    );
    let refused = q
        .call(
            h.named(b"System").unwrap(),
            b"loadImageBytes:rebuild:",
            &[Operand::object(truncated), Operand::object(no)],
        )
        .unwrap();
    assert_eq!(refused, nil);
    for (owner, vt, selector, args) in [
        (
            compiler,
            q.call(id.receiver, b"behaviorOf:", &[Operand::object(compiler)])
                .unwrap(),
            b"compile:signature:".as_slice(),
            vec![Operand::object(name), Operand::object(signature)],
        ),
        (
            format,
            q.call(id.receiver, b"behaviorOf:", &[Operand::object(format)]).unwrap(),
            b"inspect:architecture:abi:",
            vec![
                Operand::object(bytes),
                Operand::object(arch),
                Operand::object(abi),
            ],
        ),
    ] {
        let selector_object = unsafe { id.intern_name(selector) }.unwrap();
        let original = h
            .send(
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector_object), Operand::object(vt)],
            )
            .unwrap()
            .as_entry()
            .unwrap();
        // A compiled compiler Method owns its mapped entry. Preserve that owner
        // while replacing the binding and forcing collection through the hook.
        let retained = h
            .send(
                h.named(b"Execution").unwrap(),
                b"methodOfEntry:",
                &[Operand::entry(original)],
            )
            .unwrap();
        if retained.as_object().is_some() {
            q.keep(retained).unwrap();
        }
        let mut hook = Hook {
            id,
            gc: h.named(b"GC").unwrap(),
            original,
            calls: Cell::new(0),
            once: false,
        };
        h.send(
            vt,
            b"addMethod",
            &[
                Operand::object(selector_object),
                Operand::entry(NativeEntry::new(collect, (&mut hook as *mut Hook).cast())),
            ],
        )
        .unwrap();
        q.call(owner, selector, &args).unwrap();
        assert_eq!(hook.calls.get(), 1);
        h.send(
            vt,
            b"addMethod",
            &[
                Operand::object(selector_object),
                Operand::entry(NativeEntry::new(fail, std::ptr::null_mut())),
            ],
        )
        .unwrap();
        let result = if owner == compiler {
            let sig = q.string(b"v").unwrap();
            unsafe {
                id.send_named(
                    h.named(b"Alien").unwrap(),
                    b"callback:signature:",
                    &[Operand::object(nil), Operand::object(sig)],
                )
            }
        } else {
            unsafe {
                id.send_named(
                    h.named(b"System").unwrap(),
                    b"loadImageBytes:rebuild:",
                    &[Operand::object(truncated), Operand::object(no)],
                )
            }
        };
        let error = result.unwrap_err();
        assert_eq!(error.status, 6);
        assert_eq!(error.detail, 7071);
        h.send(
            vt,
            b"addMethod",
            &[Operand::object(selector_object), Operand::entry(original)],
        )
        .unwrap();
    }
}
