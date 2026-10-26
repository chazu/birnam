//! Exercise native module declarations and Id messages independently of owner bodies.
use birnam_host_support::Context;
use std::{env, os::unix::ffi::OsStrExt, path::PathBuf};
#[path = "../src/test_hooks.rs"]
#[allow(dead_code)] // Other Host fixtures use the remaining callback helpers.
mod test_hooks;
use test_hooks::Hooks;

#[test]
#[ignore = "requires installed native Host libraries; scripts/host-cli-test.sh"]
fn native_host_owners_use_descriptors_and_messages() {
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
    h.send(
        h.named(b"GC").unwrap(),
        b"precisePayloads:",
        &[birnam_abi::size(1).unwrap()],
    )
    .unwrap();
    let q = h.scope();
    let r = &h.resources;
    let mut init = vec![h.namespace];
    for p in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        init.push(q.string(p.as_os_str().as_bytes()).unwrap());
    }
    let mut owners = Vec::new();
    for (library, name) in [
        ("host_entry", b"HostEntry".as_slice()),
        ("host_project", b"HostProject"),
        ("project_build", b"ProjectBuild"),
        ("host_interaction", b"HostInteraction"),
    ] {
        let path = r.lib_dir.join(format!("libbirnam_{library}.dylib"));
        let owner = h.load_native(&q, &path, &init).unwrap();
        assert_eq!(owner, h.named(name).unwrap());
        assert_eq!(h.load_native(&q, &path, &init).unwrap(), owner);
        assert!(h.load_native(&q, &path, &init[..1]).is_err());
        owners.push((owner, path));
    }
    let entry = owners[0].0;
    let build = owners[2].0;
    let interaction = owners[3].0;
    q.call(
        entry,
        b"bootstrap:tools:",
        &[h.named(b"false").unwrap(), h.named(b"false").unwrap()],
    )
    .and_then(|o| q.check_result(o))
    .unwrap();
    assert_eq!(
        q.call(entry, b"imageHasMain", &[]).unwrap(),
        h.named(b"false").unwrap()
    );
    // Reloading an owner preserves its receiver and replaced method.
    let behavior = q.call(id.root, b"behaviorOf:", &[entry]).unwrap();
    let selector = fixtures.symbol(b"imageHasMain");
    let original = fixtures.capture(behavior, selector);
    h.send(
        behavior,
        b"addMethod",
        &[
            selector,
            fixtures.method(b"fixed", h.named(b"true").unwrap()),
        ],
    )
    .unwrap();
    assert_eq!(h.load_native(&q, &owners[0].1, &init).unwrap(), entry);
    assert_eq!(
        q.call(entry, b"imageHasMain", &[]).unwrap(),
        h.named(b"true").unwrap()
    );
    h.send(behavior, b"addMethod", &[selector, original])
        .unwrap();
    // An error survives collection while its message String and newly interned
    // error-kind Symbol are constructed.
    let string = q
        .call(id.root, b"behaviorOf:", &[h.named(b"String").unwrap()])
        .unwrap();
    let from_bytes = fixtures.symbol(b"fromBytes:");
    let original = fixtures.capture(string, from_bytes);
    let hook = fixtures.context(&[
        original,
        h.named(b"GC").unwrap(),
        birnam_abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    h.send(
        string,
        b"addMethod",
        &[from_bytes, fixtures.method(b"collectEvery", hook)],
    )
    .unwrap();
    let error = q.call(interaction, b"repl:", &[]).unwrap();
    h.send(string, b"addMethod", &[from_bytes, original])
        .unwrap();
    assert!(fixtures.count(hook, 2) > 0);
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        fixtures.symbol(b"hostError")
    );
    assert!(q.check_result(error).unwrap_err().0.contains("wrong arity"));
    let malformed = q.string(b"not json").unwrap();
    let version = q.string(b"fixture").unwrap();
    let error = q
        .call(build, b"run:version:", &[malformed, version])
        .unwrap();
    assert!(q.check_result(error).is_err());
    println!(
        "native Host owners: declarations, completed Namespace inputs, reload preservation, malformed requests and collecting error construction passed"
    );
}
