//! Shared operation receivers remain ordinary replaceable Id dispatch.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "fresh authored Integer operation objects required"]
fn operations_have_distinct_receivers_and_current_collecting_handler_dispatch() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let gc = h.named(b"GC").unwrap();
    q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let ops: Vec<_> = [
        b"intAddOperation".as_slice(),
        b"intSubtractOperation",
        b"intMultiplyOperation",
        b"intDivideOperation",
        b"intRemainderOperation",
    ]
    .iter()
    .map(|name| h.global(&q, name).unwrap())
    .collect();
    for (i, &op) in ops.iter().enumerate() {
        assert_ne!(op, h.named(b"nil").unwrap());
        for &other in &ops[i + 1..] {
            assert_ne!(op, other);
        }
    }
    let integer = h.named(b"Integer").unwrap();
    let a = q.call(integer, b"fromWord:", &[size(11).unwrap()]).unwrap();
    let b = q.call(integer, b"fromWord:", &[size(3).unwrap()]).unwrap();
    let answer = q
        .call(integer, b"fromWord:", &[size(777).unwrap()])
        .unwrap();
    let behavior = q.call(h.id.root, b"behaviorOf:", &[ops[0]]).unwrap();
    let selector = hooks.symbol(b"applyInteger:with:");
    // Retaining the captured Method also keeps its context and code owner alive.
    let original = hooks.capture(behavior, selector);
    assert_ne!(original, h.named(b"nil").unwrap());

    // An authored replacement enforces two arguments and collects before returning
    // its captured answer. The declared native wrapper counts without invoking
    // Integer addition recursively through the operation being replaced.
    let source = q
        .string(
            br#"[:answer |
        let holder := [Object clone];
        [Reflect defineMethod: #replacement:with: on: holder as: [:self :a :b |
            [System gc]; answer
        ]];
        [Execution directMethodFor: #replacement:with: in: [Reflect vtableOf: holder]]
    ]"#,
        )
        .unwrap();
    let factory = q
        .call(h.named(b"Codegen").unwrap(), b"eval:", &[source])
        .unwrap();
    q.check_result(factory).unwrap();
    let replacement = q.call(factory, b"value:", &[answer]).unwrap();
    q.check_result(replacement).unwrap();
    let context = hooks.context(&[replacement, size(0).unwrap()]);
    let method = hooks.method(b"counted", context);
    q.call(behavior, b"addMethod:as:", &[method, selector])
        .unwrap();

    assert_eq!(q.call(a, b"+", &[b]).unwrap(), answer);
    assert_eq!(hooks.count(context, 1), 1);
    let difference = q.call(a, b"-", &[b]).unwrap();
    assert_eq!(as_size(q.call(difference, b"word", &[]).unwrap()), Some(8));
    q.call(behavior, b"addMethod:as:", &[original, selector])
        .unwrap();
    let sum = q.call(a, b"+", &[b]).unwrap();
    assert_eq!(as_size(q.call(sum, b"word", &[]).unwrap()), Some(14));
    assert_eq!(hooks.count(context, 1), 1);
}
