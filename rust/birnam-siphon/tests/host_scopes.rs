//! Host scopes report native references without per-result tokens or argument copies.
#[allow(dead_code)]
#[path = "support/minimal.rs"]
mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;
#[test]
#[ignore = "requires native composition"]
fn scopes_preserve_live_references_and_release_in_any_order() {
    let h = minimal::native_floor();
    {
        let q = h.scope();
        q.read_bytes(q.string(b"initialize").unwrap()).unwrap();
    }
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let gc = h.named(b"GC").unwrap();
    let object = h.named(b"Object").unwrap();
    let input = q.string(b"kept").unwrap();
    let roots_vt = q.call(h.id.root, b"behaviorOf:", &[h.roots]).unwrap();
    let retain = hooks.symbol(b"retain:");
    let release = hooks.symbol(b"release:");
    let add = hooks.symbol(b"addMethod");
    let saved_retain = q.call(roots_vt, b"lookup", &[retain]).unwrap();
    let saved_release = q.call(roots_vt, b"lookup", &[release]).unwrap();
    let denied = hooks.context(&[hooks.fault(71), size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied);
    q.call(roots_vt, b"addMethod", &[retain, deny]).unwrap();
    q.call(roots_vt, b"addMethod", &[release, deny]).unwrap();
    {
        let scope = h.scope();
        for _ in 0..100 {
            assert_eq!(scope.call(input, b"printString", &[]).unwrap(), input);
        }
        scope.call(gc, b"collectPrecisely", &[]).unwrap();
    }
    unsafe { h.id.send(roots_vt, add, &[retain, saved_retain]) }.unwrap();
    unsafe { h.id.send(roots_vt, add, &[release, saved_release]) }.unwrap();
    assert_eq!(
        hooks.count(denied, 1),
        0,
        "Host calls must not retain/release individual results"
    );

    let first = h.scope();
    let second = h.scope();
    let dead = first.call(object, b"clone", &[]).unwrap();
    let live = second.call(object, b"clone", &[]).unwrap();
    drop(first); // Scopes need not close in stack order.
    q.call(gc, b"collectPrecisely", &[]).unwrap();
    let behavior = hooks.symbol(b"behaviorOf:");
    assert!(unsafe { h.id.send(h.id.root, behavior, &[live]) }.is_ok());
    assert!(unsafe { h.id.send(h.id.root, behavior, &[dead]) }.is_err());
    drop(second);
    q.call(gc, b"collectPrecisely", &[]).unwrap();
    assert!(unsafe { h.id.send(h.id.root, behavior, &[live]) }.is_err());
}
