//! Root providers keep registration order and own their child references.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "requires production Oop modules"]
fn registered_root_enumerators_grow_and_keep_order_and_children_alive() {
    let h = oop::bootstrap();
    let held = h.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-root-providers-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/root-providers.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&held, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let symbol = |name: &[u8]| held.keep(h.intern(name).unwrap()).unwrap();
    let declared = |name: &[u8]| held.call(module, b"methodAt:", &[symbol(name)]).unwrap();
    let enumerate = declared(b"enumerate");
    let child = declared(b"child");
    let recorded = declared(b"recorded");
    let reset = declared(b"reset");
    let roots = h.named(b"Roots").unwrap();
    let gc = h.named(b"GC").unwrap();
    let parent = h.named(b"ObjectVt").unwrap();
    let nil = h.named(b"nil").unwrap();
    held.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let visit = symbol(b"rootsDo:");
    let extent = symbol(b"allocationSizeOf:");
    let collect = symbol(b"collectPrecisely");
    let mut providers = Vec::new();
    let mut children = Vec::new();
    for ordinal in 0..64 {
        let q = h.scope();
        let behavior = q.call(parent, b"delegated", &[]).unwrap();
        let value = q.call(parent, b"allocate", &[size(0).unwrap()]).unwrap();
        let method = q
            .call(enumerate, b"withContext:", &[size(ordinal).unwrap()])
            .unwrap();
        q.call(behavior, b"addMethod", &[visit, method]).unwrap();
        let provider = q.call(behavior, b"allocate", &[size(0).unwrap()]).unwrap();
        let args = q
            .call(h.id.root, b"sequence", &[size(ordinal).unwrap(), value])
            .unwrap();
        q.call(child, b"invokeOn:arguments:", &[module, args])
            .unwrap();
        q.call(roots, b"register:", &[provider]).unwrap();
        providers.push(provider);
        children.push(value);
    }
    // Rust vectors are deliberately not Id roots; the provider is the only edge.
    held.call(roots, b"register:", &[providers[3]]).unwrap();
    let empty = held.call(h.id.root, b"sequence", &[]).unwrap();
    held.call(reset, b"invokeOn:arguments:", &[module, empty])
        .unwrap();
    unsafe { h.id.send(gc, collect, &[]) }.unwrap();
    for &object in providers.iter().chain(&children) {
        assert_eq!(
            as_size(unsafe { h.id.send(h.id.root, extent, &[object]) }.unwrap()),
            Some(0)
        );
    }
    let order = held
        .call(recorded, b"invokeOn:arguments:", &[module, empty])
        .unwrap();
    let count = as_size(held.call(order, b"size", &[]).unwrap()).unwrap();
    let actual: Vec<_> = (0..count)
        .map(|i| as_size(held.call(order, b"at:", &[size(i).unwrap()]).unwrap()).unwrap())
        .collect();
    assert_eq!(actual, (0..64).chain([3]).collect::<Vec<_>>());
    for ordinal in 0..64 {
        let q = h.scope();
        let args = q
            .call(h.id.root, b"sequence", &[size(ordinal).unwrap(), nil])
            .unwrap();
        q.call(child, b"invokeOn:arguments:", &[module, args])
            .unwrap();
    }
    unsafe { h.id.send(gc, collect, &[]) }.unwrap();
    // Compare allocation identities without passing reclaimed references back
    // into Id. An address may already have been reused for another allocation.
    let visitor_type = held.call(parent, b"delegated", &[]).unwrap();
    held.call(
        visitor_type,
        b"addMethod",
        &[
            symbol(b"allocation:bytes:identity:"),
            declared(b"allocation:bytes:identity:"),
        ],
    )
    .unwrap();
    let visitor = held
        .call(visitor_type, b"allocate", &[size(0).unwrap()])
        .unwrap();
    held.call(h.id.root, b"allocationsDo:", &[visitor]).unwrap();
    let count = held
        .call(
            declared(b"remaining"),
            b"invokeOn:arguments:",
            &[module, empty],
        )
        .unwrap();
    assert_eq!(as_size(count), Some(0));
}
