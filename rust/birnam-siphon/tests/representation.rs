//! Allocation facts decline when their getters, query or lookup are replaced.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "current native Id required"]
fn representation_facts_and_fallbacks() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let base = h.named(b"ObjectVt").unwrap();
    let behavior = q.call(base, b"delegated", &[]).unwrap();
    let other = q.call(base, b"delegated", &[]).unwrap();
    let value = q.call(behavior, b"allocate", &[size(8).unwrap()]).unwrap();
    let root_behavior = q.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    let selector = hooks.symbol(b"tryRepresentationIdentityOf:behavior:bytes:");
    let add = hooks.symbol(b"addMethod");
    let at = hooks.symbol(b"at:");
    let put = hooks.symbol(b"at:put:");
    let reassociate = hooks.symbol(b"reassociateBehaviorOf:with:");
    let raw = |r, s, a: &[_]| unsafe { h.id.send(r, s, a) };
    let query = |a: &[_]| raw(h.id.root, selector, a).unwrap();
    let args = [value, behavior, size(8).unwrap()];
    let identity = q.call(h.id.root, b"identityOf:", &[value]).unwrap();
    assert_eq!(query(&args), identity);
    assert_ne!(as_size(identity), Some(0));
    for wrong in [
        [value, other, size(8).unwrap()],
        [value, behavior, size(7).unwrap()],
        [size(3).unwrap(), behavior, size(8).unwrap()],
    ] {
        assert_eq!(as_size(query(&wrong)), Some(0));
    }
    assert!(raw(h.id.root, selector, &args[..2]).is_err());
    assert!(raw(h.id.root, selector, &[value, behavior, value]).is_err());
    for name in [
        b"behaviorOf:".as_slice(),
        b"allocationSizeOf:",
        b"identityOf:",
    ] {
        let selected = hooks.symbol(name);
        let original = q
            .call(h.id.root, b"bind:to:", &[selected, h.id.root])
            .unwrap();
        let failure = hooks.fault(19);
        let context = hooks.context(&[failure, size(0).unwrap()]);
        let replacement = hooks.method(b"deny", context);
        raw(root_behavior, add, &[selected, replacement]).unwrap();
        assert_eq!(raw(h.id.root, selected, &[value]).unwrap_err(), failure);
        raw(context, put, &[size(1).unwrap(), size(0).unwrap()]).unwrap();
        assert!(query(&args).is_null());
        assert_eq!(
            as_size(raw(context, at, &[size(1).unwrap()]).unwrap()),
            Some(0),
            "allocation fact query must not execute a changed getter"
        );
        raw(root_behavior, add, &[selected, original]).unwrap();
    }
    let original = q
        .call(h.id.root, b"bind:to:", &[selector, h.id.root])
        .unwrap();
    let context = hooks.context(&[original, size(0).unwrap()]);
    let forwarding = hooks.method(b"counted", context);
    raw(root_behavior, add, &[selector, forwarding]).unwrap();
    assert!(
        query(&args).is_null(),
        "a forwarding wrapper cannot claim the canonical answer"
    );
    assert_eq!(hooks.count(context, 1), 1);
    raw(root_behavior, add, &[selector, original]).unwrap();

    let meta = q.call(h.id.root, b"behaviorOf:", &[root_behavior]).unwrap();
    let lookup = hooks.symbol(b"lookup");
    let original_lookup = q.call(meta, b"lookup", &[lookup]).unwrap();
    let custom_meta = q.call(meta, b"delegated", &[]).unwrap();
    let alias = hooks.symbol(b"originalAllocationLookup:");
    raw(custom_meta, add, &[alias, original_lookup]).unwrap();
    let context = hooks.context(&[alias, size(0).unwrap()]);
    let forwarding = hooks.method(b"lookupForward", context);
    raw(custom_meta, add, &[lookup, forwarding]).unwrap();
    raw(h.id.root, reassociate, &[root_behavior, custom_meta]).unwrap();
    assert!(query(&args).is_null());
    assert_eq!(
        as_size(raw(context, at, &[size(1).unwrap()]).unwrap()),
        Some(1),
        "only the ordinary method lookup occurs"
    );
    raw(h.id.root, reassociate, &[root_behavior, meta]).unwrap();
    assert_eq!(query(&args), identity);
    raw(h.id.root, reassociate, &[value, other]).unwrap();
    assert_eq!(
        as_size(query(&args)),
        Some(0),
        "no shape grant survives reassociation"
    );
}
