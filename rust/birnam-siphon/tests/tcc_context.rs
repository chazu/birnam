//! A code provider binds opaque Oop contexts without replacing code ownership.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::path::Path;

#[test]
#[ignore = "requires production Oop TCC"]
fn declared_code_binds_contexts_purely_and_keeps_their_lifetimes_separate() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| {
        q.call(r, n, a)
            .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(n)))
    };
    let source = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/oop-continuation/tcc-context.c"),
    )
    .unwrap();
    let tcc = named(b"TCC");
    let gc = named(b"GC");
    let unit = call(tcc, b"compile:", &[q.string(&source).unwrap()]);
    q.check_result(unit).unwrap();
    assert!(
        h.send(
            unit,
            b"declaredMethodAt:context:",
            &[size(0).unwrap(), named(b"nil")]
        )
        .is_err()
    );
    assert!(h.send(unit, b"initializeWith:", &[named(b"nil")]).is_err());
    assert!(
        h.send(
            unit,
            b"declaredMethodAt:context:",
            &[size(0).unwrap(), named(b"nil")]
        )
        .is_err()
    );
    let escaped = call(unit, b"declaredMethodAt:", &[size(0).unwrap()]);
    assert_eq!(call(unit, b"initialize", &[]), unit);
    let object = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[object]);
    let original = call(unit, b"declaredMethodAt:", &[size(0).unwrap()]);
    call(vt, b"addMethod", &[key(b"original"), original]);
    assert_eq!(call(object, b"original", &[]), unit);
    call(vt, b"addMethod", &[key(b"escaped"), escaped]);
    assert_eq!(call(object, b"escaped", &[]), unit);
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let one = call(named(b"Object"), b"clone", &[]);
    let two = call(named(b"Object"), b"clone", &[]);
    let before = epoch();
    let first = call(unit, b"declaredMethodAt:context:", &[size(0).unwrap(), one]);
    let second = call(unit, b"declaredMethodAt:context:", &[size(0).unwrap(), two]);
    let triple = call(unit, b"declaredMethodAt:context:", &[size(1).unwrap(), one]);
    let continuing = call(unit, b"declaredMethodAt:context:", &[size(2).unwrap(), two]);
    assert_eq!(epoch(), before);
    assert_ne!(first, second);
    assert_eq!(call(tcc, b"contextOfMethod:", &[first]), one);
    assert_eq!(call(tcc, b"contextOfMethod:", &[second]), two);
    assert_eq!(call(tcc, b"contextOfMethod:", &[original]), unit);
    let captured = call(first, b"captured", &[]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(call(tcc, b"contextOfMethod:", &[captured]), one);
    let foreign = hooks.method(b"fixed", two);
    assert!(
        unsafe { h.id.send(tcc, key(b"contextOfMethod:"), &[foreign]) }
            .unwrap()
            .is_null()
    );
    assert!(h.send(tcc, b"contextOfMethod:", &[]).is_err());
    // No context accessor was added to the bootstrap Method behavior.
    let method_vt = call(h.id.root, b"methodBehavior", &[]);
    assert!(
        unsafe { h.id.send(method_vt, key(b"lookup"), &[key(b"context")]) }
            .unwrap()
            .is_null()
    );
    for (name, method) in [
        (b"first".as_slice(), first),
        (b"second", second),
        (b"triple:", triple),
        (b"continuing", continuing),
    ] {
        call(
            named(b"Reflect"),
            b"defineMethod:on:as:",
            &[key(name), object, method],
        );
    }
    assert_eq!(call(object, b"first", &[]), one);
    assert_eq!(call(object, b"second", &[]), two);
    assert_eq!(
        call(object, b"continuing", &[]),
        two,
        "continuation calling convention must survive binding"
    );
    let tuple = call(object, b"triple:", &[two]);
    for (i, expected) in [one, object, two].into_iter().enumerate() {
        assert_eq!(call(tuple, b"at:", &[size(i).unwrap()]), expected);
    }
    let before = epoch();
    assert!(
        h.send(unit, b"declaredMethodAt:context:", &[size(4).unwrap(), one])
            .is_err()
    );
    assert!(
        h.send(unit, b"declaredMethodAt:context:", &[size(0).unwrap()])
            .is_err()
    );
    assert_eq!(epoch(), before);
    // The context is opaque Oop data, including immediate and null values.
    for context in [size(17).unwrap(), std::ptr::null_mut()] {
        let method = call(
            unit,
            b"declaredMethodAt:context:",
            &[size(0).unwrap(), context],
        );
        call(vt, b"addMethod", &[key(b"opaque"), method]);
        assert_eq!(
            unsafe { h.id.send(object, key(b"opaque"), &[]) }.unwrap(),
            context
        );
    }

    // A captured alias alone keeps the representative used by owner reflection
    // valid even after the originally returned Method loses its caller roots.
    let (alias, environment) = {
        let local = h.scope();
        let environment = local.call(h.id.root, b"sequence", &[two]).unwrap();
        let method = local
            .call(
                unit,
                b"declaredMethodAt:context:",
                &[size(0).unwrap(), environment],
            )
            .unwrap();
        let alias = local.call(method, b"captured", &[]).unwrap();
        local
            .call(vt, b"addMethod", &[key(b"capturedOnly"), alias])
            .unwrap();
        (alias, environment)
    };
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(call(tcc, b"contextOfMethod:", &[alias]), environment);
    assert_eq!(call(object, b"capturedOnly", &[]), environment);
    assert_eq!(call(environment, b"at:", &[size(0).unwrap()]), two);

    // Keeping the Unit alive must not retain every bound environment.
    let identity;
    {
        let local = h.scope();
        let environment = local.call(named(b"Object"), b"clone", &[]).unwrap();
        identity = call(h.id.root, b"identityOf:", &[environment]);
        local
            .call(
                unit,
                b"declaredMethodAt:context:",
                &[size(0).unwrap(), environment],
            )
            .unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    let state = call(h.id.root, b"sequence", &[identity, size(0).unwrap()]);
    let observer = call(named(b"Object"), b"clone", &[]);
    let observer_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[observer]);
    let visit = call(
        unit,
        b"declaredMethodAt:context:",
        &[size(3).unwrap(), state],
    );
    call(
        observer_vt,
        b"addMethod",
        &[key(b"allocation:bytes:identity:"), visit],
    );
    call(h.id.root, b"allocationsDo:", &[observer]);
    assert_eq!(call(state, b"at:", &[size(1).unwrap()]), size(0).unwrap());
    let live_identity = call(h.id.root, b"identityOf:", &[one]);
    call(state, b"at:put:", &[size(0).unwrap(), live_identity]);
    call(h.id.root, b"allocationsDo:", &[observer]);
    assert_eq!(
        call(state, b"at:", &[size(1).unwrap()]),
        size(1).unwrap(),
        "the observer must find a live control object"
    );

    // An installed binding is the sole owner of its generated unit and environment.
    call(gc, b"collectPrecisely", &[]);
    call(tcc, b"checkpoint", &[]);
    let units = || as_size(call(tcc, b"liveNativeUnits", &[])).unwrap();
    let baseline = units();
    {
        let local = h.scope();
        let source = local.string(&source).unwrap();
        let unit = local.call(tcc, b"compile:", &[source]).unwrap();
        local.check_result(unit).unwrap();
        local.call(unit, b"initialize", &[]).unwrap();
        let environment = local.call(h.id.root, b"sequence", &[two]).unwrap();
        let method = local
            .call(
                unit,
                b"declaredMethodAt:context:",
                &[size(1).unwrap(), environment],
            )
            .unwrap();
        local
            .call(vt, b"addMethod", &[key(b"retained:"), method])
            .unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(units(), baseline + 1);
    {
        let local = h.scope();
        let tuple = local.call(object, b"retained:", &[one]).unwrap();
        let environment = local.call(tuple, b"at:", &[size(0).unwrap()]).unwrap();
        assert_eq!(
            local
                .call(environment, b"at:", &[size(0).unwrap()])
                .unwrap(),
            two
        );
        assert_eq!(
            local.call(tuple, b"at:", &[size(1).unwrap()]).unwrap(),
            object
        );
        assert_eq!(local.call(tuple, b"at:", &[size(2).unwrap()]).unwrap(), one);
    }
    unsafe { h.id.send(vt, key(b"removeMethod:"), &[key(b"retained:")]) }.unwrap();
    call(gc, b"collectPrecisely", &[]);
    call(tcc, b"checkpoint", &[]);
    assert_eq!(units(), baseline);
}
