//! Id batches allocation facts without reading the value owner's payload.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};

#[test]
#[ignore = "requires current native Id"]
fn batch_facts_are_current_bounded_and_decline_replaced_dependencies() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let base = h.named(b"ObjectVt").unwrap();
    let behavior = q.call(base, b"delegated", &[]).unwrap();
    let other = q.call(base, b"delegated", &[]).unwrap();
    let a = q.call(behavior, b"allocate", &[size(8).unwrap()]).unwrap();
    let z = q.call(behavior, b"allocate", &[size(8).unwrap()]).unwrap();
    let root_behavior = q.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    let batch = hooks.symbol(b"representationsMatch");
    let add = hooks.symbol(b"addMethod");
    let at = hooks.symbol(b"at:");
    let query = |values: &[Oop], behavior, bytes| {
        let mut args = vec![behavior, size(bytes).unwrap()];
        args.extend_from_slice(values);
        unsafe { h.id.send(h.id.root, batch, &args) }
    };
    let values = [a, z];
    assert_eq!(as_size(query(&values, behavior, 8).unwrap()), Some(1));
    assert_eq!(as_size(query(&values, behavior, 9).unwrap()), Some(0));
    assert_eq!(as_size(query(&[a, a], behavior, 8).unwrap()), Some(1));
    assert_eq!(as_size(query(&values, other, 8).unwrap()), Some(0));
    assert_eq!(
        as_size(query(&[a, size(3).unwrap()], behavior, 8).unwrap()),
        Some(0)
    );
    for count in [0, 1, 33] {
        assert!(query(&vec![a; count], behavior, 8).is_err());
    }
    assert!(unsafe { h.id.send(h.id.root, batch, &[behavior, a, a, z]) }.is_err());
    assert_eq!(as_size(query(&[a; 32], behavior, 8).unwrap()), Some(1));
    for name in [
        b"tryRepresentationIdentityOf:behavior:bytes:".as_slice(),
        b"behaviorOf:",
        b"allocationSizeOf:",
        b"identityOf:",
        b"representationsMatch",
    ] {
        let selector = hooks.symbol(name);
        let original = q
            .call(h.id.root, b"bind:to:", &[selector, h.id.root])
            .unwrap();
        let context = hooks.context(&[original, size(0).unwrap()]);
        let forwarding = hooks.method(b"counted", context);
        unsafe { h.id.send(root_behavior, add, &[selector, forwarding]) }.unwrap();
        assert!(query(&values, behavior, 8).unwrap().is_null());
        let calls = unsafe { h.id.send(context, at, &[size(1).unwrap()]) }.unwrap();
        assert_eq!(
            as_size(calls),
            Some(usize::from(name == b"representationsMatch")),
            "a declined batch does not call omitted getters"
        );
        unsafe { h.id.send(root_behavior, add, &[selector, original]) }.unwrap();
    }
    assert_eq!(as_size(query(&values, behavior, 8).unwrap()), Some(1));
    q.call(h.id.root, b"reassociateBehaviorOf:with:", &[z, other])
        .unwrap();
    assert_eq!(
        as_size(query(&values, behavior, 8).unwrap()),
        Some(0),
        "no shape verdict survives reassociation"
    );
}
