//! Existing numeric values remain readable when identity lookup is replaced.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "production numeric owners required"]
fn identity_replacement_preserves_numeric_read_and_allocator_failure_order() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let word = h.named(b"Word").unwrap();
    let value = q.call(word, b"fromBits:", &[size(91).unwrap()]).unwrap();
    let selector = hooks.symbol(b"identityOf:");
    let behavior = q.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    let original = q
        .call(h.id.root, b"bind:to:", &[selector, h.id.root])
        .unwrap();
    assert_ne!(original, h.named(b"nil").unwrap());
    let failure = hooks.fault(19);
    let context = hooks.context(&[failure, size(0).unwrap()]);
    let replacement = hooks.method(b"deny", context);
    q.call(behavior, b"addMethod", &[selector, replacement])
        .unwrap();
    assert_eq!(
        unsafe { h.id.send(h.id.root, selector, &[value]) }.unwrap_err(),
        failure
    );
    assert_eq!(hooks.count(context, 1), 1);
    hooks.put(context, 1, size(0).unwrap());

    assert_eq!(as_size(h.send(value, b"bits", &[]).unwrap()), Some(91));
    assert_eq!(
        hooks.count(context, 1),
        0,
        "existing numeric reads do not query identity"
    );
    let failed = unsafe {
        h.id.send(word, hooks.symbol(b"fromBits:"), &[size(92).unwrap()])
    }
    .unwrap_err();
    assert_eq!(failed, failure);
    assert_eq!(hooks.count(context, 1), 1);
    q.call(behavior, b"addMethod", &[selector, original])
        .unwrap();
}
