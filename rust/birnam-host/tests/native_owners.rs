//! Exercise the descriptor/message boundary, independently of Rust owner bodies.
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_host_support::{Context, object};
use std::{cell::Cell, env, ffi::c_void, os::unix::ffi::OsStrExt, path::PathBuf};
struct Collect {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Collect>() };
    if let Err(error) = unsafe { s.id.send_named(s.gc, b"collectPrecisely", &[]) } {
        return error;
    }
    s.calls.set(s.calls.get() + 1);
    unsafe { s.original.method.unwrap()(s.original.context, r, a, n) }
}
unsafe extern "C" fn answer_true(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
#[test]
#[ignore = "requires installed native Host libraries; scripts/host-cli-test.sh"]
fn native_host_owners_use_descriptors_and_messages() {
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
                    Operand::bytes(
                        resources
                            .lib_dir
                            .join("libbirnam_bootstrap.dylib")
                            .as_os_str()
                            .as_bytes(),
                    ),
                    Operand::bytes(resources.lib_dir.as_os_str().as_bytes()),
                ],
            )
        }
        .unwrap(),
    )
    .unwrap();
    let h = Context::from_namespace(id, ns, resources).unwrap();
    h.send(
        h.named(b"GC").unwrap(),
        b"precisePayloads:",
        &[Operand::word(1)],
    )
    .unwrap();
    let q = h.scope();
    let r = &h.resources;
    let mut init = vec![Operand::object(ns)];
    for p in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        init.push(Operand::object(q.string(p.as_os_str().as_bytes()).unwrap()));
    }
    let mut owners = Vec::new();
    for (library, name) in [
        ("host_entry", b"HostEntry".as_slice()),
        ("host_project", b"HostProject"),
        ("project_build", b"ProjectBuild"),
        ("host_interaction", b"HostInteraction"),
    ] {
        let path = r.lib_dir.join(format!("libbirnam_{library}.dylib"));
        let mut request = vec![Operand::bytes(path.as_os_str().as_bytes())];
        request.extend_from_slice(&init);
        let owner = q
            .keep(unsafe { id.send(id.receiver, id.load, &request) }.unwrap())
            .unwrap();
        assert_eq!(owner, h.named(name).unwrap());
        assert_eq!(
            q.keep(unsafe { id.send(id.receiver, id.load, &request) }.unwrap())
                .unwrap(),
            owner
        );
        assert!(unsafe { id.send(id.receiver, id.load, &request[..2]) }.is_err());
        owners.push((owner, path));
    }
    let entry = owners[0].0;
    let build = owners[2].0;
    let interaction = owners[3].0;
    q.call(
        entry,
        b"bootstrap:tools:",
        &[
            Operand::object(h.named(b"false").unwrap()),
            Operand::object(h.named(b"false").unwrap()),
        ],
    )
    .and_then(|o| q.check_result(o))
    .unwrap();
    assert_eq!(
        q.call(entry, b"imageHasMain", &[]).unwrap(),
        h.named(b"false").unwrap()
    );
    // Reloading a constructed owner preserves a replacement; it cannot reset
    // a retained receiver or install another callback context.
    let behavior = q
        .call(id.receiver, b"behaviorOf:", &[Operand::object(entry)])
        .unwrap();
    let selector = unsafe { id.intern_name(b"imageHasMain") }.unwrap();
    let original = h
        .send(
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(behavior)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    h.send(
        behavior,
        b"addMethod",
        &[
            Operand::object(selector),
            Operand::entry(NativeEntry::new(
                answer_true,
                h.named(b"true").unwrap().cast(),
            )),
        ],
    )
    .unwrap();
    let mut request = vec![Operand::bytes(owners[0].1.as_os_str().as_bytes())];
    request.extend_from_slice(&init);
    assert_eq!(
        q.keep(unsafe { id.send(id.receiver, id.load, &request) }.unwrap())
            .unwrap(),
        entry
    );
    assert_eq!(
        q.call(entry, b"imageHasMain", &[]).unwrap(),
        h.named(b"true").unwrap()
    );
    h.send(
        behavior,
        b"addMethod",
        &[Operand::object(selector), Operand::entry(original)],
    )
    .unwrap();
    // A native error must survive precise collection while its message String
    // is constructed, including a newly interned error-kind Symbol.
    let string = q
        .call(
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(h.named(b"String").unwrap())],
        )
        .unwrap();
    let from_bytes = unsafe { id.intern_name(b"fromBytes:") }.unwrap();
    let original = h
        .send(
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(from_bytes), Operand::object(string)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let mut hook = Collect {
        id,
        gc: h.named(b"GC").unwrap(),
        original,
        calls: Cell::new(0),
    };
    h.send(
        string,
        b"addMethod",
        &[
            Operand::object(from_bytes),
            Operand::entry(NativeEntry::new(
                collecting,
                (&mut hook as *mut Collect).cast(),
            )),
        ],
    )
    .unwrap();
    let error = q.call(interaction, b"repl:", &[]).unwrap();
    h.send(
        string,
        b"addMethod",
        &[Operand::object(from_bytes), Operand::entry(original)],
    )
    .unwrap();
    assert!(hook.calls.get() > 0);
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        unsafe { id.intern_name(b"hostError") }.unwrap()
    );
    assert!(q.check_result(error).unwrap_err().0.contains("wrong arity"));
    let malformed = q.string(b"not json").unwrap();
    let version = q.string(b"fixture").unwrap();
    let error = q
        .call(
            build,
            b"run:version:",
            &[Operand::object(malformed), Operand::object(version)],
        )
        .unwrap();
    assert!(q.check_result(error).is_err());
    println!(
        "native Host owners: descriptors, completed Namespace inputs, reload preservation, malformed requests and collecting error construction passed"
    );
}
