//! Store descriptions are snapshots with explicit, replaceable invalidation.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "requires production Oop modules"]
fn explicit_invalidation_preserves_user_facts_without_a_core_subscription() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let subscription = hooks.symbol(b"methodChangesTo:");
    let absent = || {
        h.send(h.id.root, b"bind:to:", &[subscription, h.id.root])
            .unwrap()
            .is_null()
    };
    assert!(absent());
    let store = h
        .load_native(
            &q,
            &h.resources.lib_dir.join("native/libbirnam_store.dylib"),
            &[h.namespace],
        )
        .unwrap();
    q.check_result(store).unwrap();
    assert!(
        absent(),
        "loading Store must not install a universal subscription"
    );
    let other = q.call(store, b"new", &[]).unwrap();
    let target = h.named(b"IntegerVt").unwrap();
    let selector = hooks.symbol(b"explicitMirrorProbe");
    let first = hooks.method(b"fixed", h.named(b"true").unwrap());
    let second = hooks.method(b"fixed", h.named(b"false").unwrap());
    let nil = h.named(b"nil").unwrap();
    let read = |owner| {
        q.call(owner, b"currentValue:attr:", &[target, selector])
            .unwrap()
    };
    for owner in [store, other] {
        assert_eq!(
            q.call(owner, b"addMethod:on:with:", &[selector, target, first])
                .unwrap(),
            owner
        );
    }
    q.call(target, b"addMethod:as:", &[second, selector])
        .unwrap();
    q.call(h.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    assert_eq!(read(store), first);
    assert_eq!(read(other), first);
    assert_eq!(hooks.capture(target, selector), second);
    assert_eq!(
        q.call(store, b"methodChanged:on:", &[selector, target])
            .unwrap(),
        store
    );
    assert_eq!(read(store), nil);
    assert_eq!(read(other), nil);

    // Explicit invalidation preserves an identical user fact and removes only
    // the description row owned by Store's method publication operation.
    q.call(store, b"addMethod:on:with:", &[selector, target, first])
        .unwrap();
    q.call(store, b"assert:attr:value:", &[target, selector, first])
        .unwrap();
    let facts = || {
        let count = q.call(store, b"retainedFactCount", &[]).unwrap();
        as_size(q.call(count, b"word", &[]).unwrap()).unwrap()
    };
    let before = facts();
    q.call(store, b"assert:attr:value:", &[target, selector, first])
        .unwrap();
    assert_eq!(
        facts(),
        before,
        "repeated explicit assertions remain idempotent"
    );
    q.call(target, b"addMethod:as:", &[second, selector])
        .unwrap();
    q.call(store, b"methodChanged:on:", &[selector, target])
        .unwrap();
    assert_eq!(facts() + 1, before);
    assert_eq!(read(store), first);
    assert_eq!(hooks.capture(target, selector), second);

    // The handler itself remains a current message, not an automatic core path.
    q.call(other, b"addMethod:on:with:", &[selector, target, first])
        .unwrap();
    let store_vt = h.named(b"StoreVt").unwrap();
    let changed = hooks.symbol(b"methodChanged:on:");
    let original = hooks.capture(store_vt, changed);
    let context = hooks.context(&[store, size(0).unwrap()]);
    let replacement = hooks.method(b"fixedCount", context);
    q.call(store_vt, b"addMethod", &[changed, replacement])
        .unwrap();
    q.call(target, b"addMethod:as:", &[second, selector])
        .unwrap();
    assert_eq!(hooks.count(context, 1), 0);
    q.call(store, b"methodChanged:on:", &[selector, target])
        .unwrap();
    assert_eq!(hooks.count(context, 1), 1);
    assert_eq!(read(other), first);
    q.call(store_vt, b"addMethod", &[changed, original])
        .unwrap();
    q.call(store, b"methodChanged:on:", &[selector, target])
        .unwrap();
    assert_eq!(read(other), nil);
    assert_eq!(read(store), first);
    q.call(target, b"removeMethod:", &[selector]).unwrap();

    let many = hooks.symbol(b"explicitManyMirrorProbe");
    q.call(
        store,
        b"assert:attr:value:",
        &[many, hooks.symbol(b"cardinality"), hooks.symbol(b"many")],
    )
    .unwrap();
    q.call(store, b"addMethod:on:with:", &[many, target, first])
        .unwrap();
    q.call(store, b"assert:attr:value:", &[target, many, first])
        .unwrap();
    let before = facts();
    q.call(store, b"assert:attr:value:", &[target, many, first])
        .unwrap();
    assert_eq!(facts(), before);
    q.call(store, b"methodChanged:on:", &[many, target])
        .unwrap();
    assert_eq!(facts() + 1, before);
    assert_eq!(
        q.call(store, b"currentValue:attr:", &[target, many])
            .unwrap(),
        first
    );
    q.call(target, b"removeMethod:", &[many]).unwrap();
    assert!(absent());
}
