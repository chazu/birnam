//! Global resolution retains fetched owners and preserves live getter order.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Namespace"]
fn global_resolution_preserves_precedence_getter_order_rebinding_gc_and_failures() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let ns = host.namespace;
    let roots = host.roots;
    let gc = named(b"GC");
    let nil = named(b"nil");
    let globals = named(b"Globals");
    let object_vt = named(b"ObjectVt");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let name = key(b"ResolutionProbe");
    let missing = key(b"ResolutionMissing");
    let nil_name = key(b"nil");
    let globals_name = key(b"Globals");
    let at = key(b"at:");
    let put = key(b"at:put:");
    let resolve = key(b"resolveGlobal:");
    let add = key(b"addMethod");
    let remove = key(b"removeMethod:");
    let allocate = key(b"allocate");
    let retain = key(b"retain:");
    let release = key(b"release:");
    let identity = key(b"identityOf:");
    let collect = key(b"collectPrecisely");
    let present = key(b"present:");
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let a = call(named(b"Object"), b"clone", &[]);
    let b = call(named(b"Object"), b"clone", &[]);
    raw(ns, put, &[name, a]).unwrap();
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), a);
    raw(globals, put, &[name, b]).unwrap();
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), b);
    raw(globals, put, &[name, nil]).unwrap();
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), a);
    assert_eq!(raw(ns, resolve, &[missing]).unwrap(), nil);
    let ns_vt = call(host.id.root, b"behaviorOf:", &[ns]);
    let original = call(host.id.root, b"bind:to:", &[at, ns]);
    let alias = key(b"resolutionOriginalAt:");
    call(ns_vt, b"addMethod", &[alias, original]);
    let globals_vt = call(object_vt, b"delegated", &[]);
    let size_vt = call(host.id.root, b"sizeBehavior", &[]);
    let size_at = call(named(b"Execution"), b"directMethodFor:in:", &[at, size_vt]);
    let path = std::env::temp_dir().join(format!(
        "birnam-global-resolution-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/global-resolution.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let fault = |status, detail| {
        let e = raw(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(status).unwrap(), size(detail).unwrap()],
        )
        .unwrap_err();
        q.keep(e).unwrap()
    };
    let key_failure = fault(812, 19);
    let ns_failure = fault(813, 20);
    let globals_failure = fault(814, 21);
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let event_rows = call(host.id.root, b"sequence", &[std::ptr::null_mut(); 256]);
    let z = size(0).unwrap();
    let one = size(1).unwrap();
    let temporary_globals = raw(globals_vt, allocate, &[z]).unwrap();
    let gt = raw(roots, retain, &[temporary_globals]).unwrap();
    let temporary_nil = raw(object_vt, allocate, &[z]).unwrap();
    let nt = raw(roots, retain, &[temporary_nil]).unwrap();
    let temporary_globals_id = raw(host.id.root, identity, &[temporary_globals]).unwrap();
    let temporary_nil_id = raw(host.id.root, identity, &[temporary_nil]).unwrap();
    let nil_id = raw(host.id.root, identity, &[nil]).unwrap();
    let nil_name_id = raw(host.id.root, identity, &[nil_name]).unwrap();
    let globals_name_id = raw(host.id.root, identity, &[globals_name]).unwrap();
    let name_id = raw(host.id.root, identity, &[name]).unwrap();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            ns,
            gc,
            monitor,
            name,
            nil_name,
            globals_name,
            nil,
            globals,
            temporary_nil_id,
            temporary_globals_id,
            event_rows,
            z,
            one,
            z,
            z,
            z,
            z,
            key_failure,
            ns_failure,
            globals_failure,
            alias,
        ],
    );
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[context],
        )
    };
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        call(monitor_vt, b"addMethod", &[key(n), method(n)]);
    }
    let namespace_get = method(b"namespaceGet");
    let globals_get = method(b"globalsGet");
    raw(globals_vt, add, &[at, globals_get]).unwrap();
    raw(size_vt, add, &[at, globals_get]).unwrap();
    raw(ns_vt, add, &[at, namespace_get]).unwrap();
    let set = |field, v| {
        raw(context, put, &[size(field).unwrap(), v]).unwrap();
    };
    let events = || {
        let count = as_size(raw(context, at, &[size(11).unwrap()]).unwrap()).unwrap();
        (0..count)
            .map(|i| raw(event_rows, at, &[size(i).unwrap()]).unwrap())
            .collect::<Vec<_>>()
    };
    raw(ns, put, &[nil_name, temporary_nil]).unwrap();
    raw(ns, put, &[globals_name, temporary_globals]).unwrap();
    raw(roots, release, &[gt]).unwrap();
    raw(roots, release, &[nt]).unwrap();
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), a);
    assert_eq!(
        events(),
        vec![nil_name_id, globals_name_id, temporary_globals_id, name_id]
    );
    raw(gc, collect, &[]).unwrap();
    assert_eq!(raw(monitor, present, &[temporary_nil_id]).unwrap(), z);
    assert_eq!(
        raw(monitor, present, &[temporary_globals_id]).unwrap(),
        z,
        "resolver scopes must release fetched owners after returning"
    );
    set(12, z);
    set(8, nil_id);
    set(14, one);
    set(11, z);
    assert_eq!(raw(ns, resolve, &[name]).unwrap_err(), key_failure);
    set(14, z);
    set(11, z);
    set(15, one);
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), a);
    assert_eq!(events(), vec![nil_name_id, globals_name_id, name_id]);
    // An immediate Globals owner is valid when its current at: provides data.
    set(15, size(2).unwrap());
    set(16, size(2).unwrap());
    set(11, z);
    assert_eq!(raw(ns, resolve, &[name]).unwrap(), size(9).unwrap());
    assert_eq!(events(), vec![nil_name_id, globals_name_id, z]);
    set(15, size(3).unwrap());
    assert_eq!(raw(ns, resolve, &[name]).unwrap_err(), ns_failure);
    set(15, z);
    let failed_owner = raw(globals_vt, allocate, &[z]).unwrap();
    let ft = raw(roots, retain, &[failed_owner]).unwrap();
    raw(ns, put, &[globals_name, failed_owner]).unwrap();
    set(16, one);
    assert_eq!(raw(ns, resolve, &[name]).unwrap_err(), globals_failure);
    set(16, size(2).unwrap());
    assert_eq!(
        raw(ns, resolve, &[name]).unwrap(),
        size(9).unwrap(),
        "a getter result is an Oop, not a separate carrier category"
    );
    raw(ns, put, &[globals_name, globals]).unwrap();
    raw(roots, release, &[ft]).unwrap();
    raw(ns_vt, add, &[at, original]).unwrap();
    raw(ns_vt, remove, &[alias]).unwrap();
    if size_at == nil {
        raw(size_vt, remove, &[at]).unwrap();
    } else {
        raw(size_vt, add, &[at, size_at]).unwrap();
    }
    assert!(
        raw(ns, resolve, &[size(1).unwrap()]).is_err(),
        "source-global names still require Symbols under the ordinary owners"
    );
}
