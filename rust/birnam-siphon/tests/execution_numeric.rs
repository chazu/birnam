//! Explicit Execution arithmetic over the current Oop Word protocols.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};

#[test]
#[ignore = "requires production Oop modules"]
fn explicit_numeric_evaluation_preserves_words_declines_and_current_owners() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let execution = named(b"Execution");
    let selector = key(b"numericOperation:left:right:");
    let values = [
        i64::MIN,
        i64::MIN + 1,
        -3_037_000_500,
        -9,
        -1,
        0,
        1,
        9,
        3_037_000_500,
        i64::MAX,
    ];
    let mut checked = 0;
    for &a in &values {
        for &b in &values {
            let pair = h.scope();
            let left = pair.word(a as usize).unwrap();
            let right = pair.word(b as usize).unwrap();
            for op in 1..=5 {
                let (a, b) = (a as i128, b as i128);
                let expected = match op {
                    1 => Some(a + b),
                    2 => Some(a - b),
                    3 => Some(a * b),
                    4 if b != 0 => Some(a / b),
                    5 if b != 0 => Some(a % b),
                    _ => None,
                }
                .and_then(|n| i64::try_from(n).ok());
                let result = pair
                    .call(
                        execution,
                        b"numericOperation:left:right:",
                        &[size(op).unwrap(), left, right],
                    )
                    .unwrap();
                let actual = if result.is_null() {
                    None
                } else {
                    Some(pair.word_bits(result).unwrap() as i64)
                };
                assert_eq!(actual, expected, "operation {op}: {a}, {b}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 500);
    let zero = size(0).unwrap();
    let one = size(1).unwrap();
    for op in [0, 6, u32::MAX as usize + 1] {
        assert!(
            h.send(
                execution,
                b"numericOperation:left:right:",
                &[size(op).unwrap(), zero, one]
            )
            .is_err()
        );
    }
    assert!(
        h.send(execution, b"numericOperation:left:right:", &[one, zero])
            .is_err()
    );
    let other = call(execution, b"clone", &[]);
    assert!(
        h.send(other, b"numericOperation:left:right:", &[one, zero, one])
            .is_err()
    );

    // Word representations may supply collecting current getters. Execution
    // must not read the foreign object's payload or infer a fixed Word family.
    let word = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[word]);
    let low = hooks.context(&[
        hooks.method(b"fixed", size(u32::MAX as usize).unwrap()),
        gc,
        zero,
    ]);
    let high = hooks.context(&[
        hooks.method(b"fixed", size(u32::MAX as usize).unwrap()),
        gc,
        zero,
    ]);
    for (name, state) in [(b"low32".as_slice(), low), (b"high32", high)] {
        call(
            vt,
            b"addMethod",
            &[key(name), hooks.method(b"collectEvery", state)],
        );
    }
    let answer = call(
        execution,
        b"numericOperation:left:right:",
        &[one, word, one],
    );
    assert_eq!(q.word_bits(answer).unwrap(), 0);
    assert_eq!(hooks.count(low, 2), 1);
    assert_eq!(hooks.count(high, 2), 1);
    for name in [b"low32".as_slice(), b"high32"] {
        let original = call(vt, b"lookup", &[key(name)]);
        call(
            vt,
            b"addMethod",
            &[
                key(name),
                hooks.method(b"fixed", size(u32::MAX as usize + 1).unwrap()),
            ],
        );
        assert!(
            h.send(
                execution,
                b"numericOperation:left:right:",
                &[one, word, one]
            )
            .is_err()
        );
        call(vt, b"addMethod", &[key(name), original]);
    }
    let fault = hooks.fault(917);
    let failed = hooks.context(&[fault, zero]);
    call(
        vt,
        b"addMethod",
        &[key(b"low32"), hooks.method(b"deny", failed)],
    );
    assert_eq!(
        unsafe { h.id.send(execution, selector, &[one, word, one]) }.unwrap_err(),
        fault
    );
    assert_eq!(hooks.count(failed, 1), 1);

    // Negative results use the current Word constructor and Namespace binding.
    let factory = named(b"Word");
    let factory_vt = call(h.id.root, b"behaviorOf:", &[factory]);
    let constructor = key(b"fromUnsignedLow:high:");
    let original = call(factory_vt, b"lookup", &[constructor]);
    let state = hooks.context(&[original, gc, zero]);
    call(
        factory_vt,
        b"addMethod",
        &[constructor, hooks.method(b"collectEvery", state)],
    );
    let answer = call(
        execution,
        b"numericOperation:left:right:",
        &[size(2).unwrap(), zero, one],
    );
    assert_eq!(q.word_bits(answer).unwrap(), usize::MAX);
    assert_eq!(hooks.count(state, 2), 1);
    call(factory_vt, b"addMethod", &[constructor, original]);
    let replacement = call(named(b"Object"), b"clone", &[]);
    let replacement_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[replacement]);
    let alternate = q.word((-123i64) as usize).unwrap();
    let count = hooks.context(&[alternate, zero]);
    call(
        replacement_vt,
        b"addMethod",
        &[constructor, hooks.method(b"fixedCount", count)],
    );
    call(h.namespace, b"at:put:", &[key(b"Word"), replacement]);
    assert_eq!(
        call(
            execution,
            b"numericOperation:left:right:",
            &[size(2).unwrap(), zero, one]
        ),
        alternate
    );
    assert_eq!(hooks.count(count, 1), 1);
    let absent = hooks.context(&[std::ptr::null_mut(), zero]);
    call(
        replacement_vt,
        b"addMethod",
        &[constructor, hooks.method(b"fixedCount", absent)],
    );
    assert!(
        h.send(
            execution,
            b"numericOperation:left:right:",
            &[size(2).unwrap(), zero, one]
        )
        .is_err()
    );
    assert_eq!(
        hooks.count(absent, 1),
        1,
        "constructor failure must not become arithmetic refusal"
    );
    call(
        replacement_vt,
        b"addMethod",
        &[constructor, hooks.method(b"deny", failed)],
    );
    assert_eq!(
        unsafe {
            h.id.send(execution, selector, &[size(2).unwrap(), zero, one])
        }
        .unwrap_err(),
        fault
    );
    call(h.namespace, b"at:put:", &[key(b"Word"), factory]);

    // The Integer owner refuses authored arithmetic without calling the visitor.
    let integer = named(b"Integer");
    let left = call(integer, b"fromWord:", &[size(42).unwrap()]);
    let right = call(integer, b"fromWord:", &[size(7).unwrap()]);
    assert_eq!(
        call(integer, b"hasBootstrapArithmetic:", &[key(b"+")]),
        named(b"false")
    );
    let selected = call(h.id.root, b"bind:to:", &[key(b"+"), left]);
    let execution_vt = call(h.id.root, b"behaviorOf:", &[execution]);
    let evaluator = call(execution_vt, b"lookup", &[selector]);
    let not_entered = hooks.context(&[fault, zero]);
    call(
        execution_vt,
        b"addMethod",
        &[selector, hooks.method(b"deny", not_entered)],
    );
    assert!(
        call(
            integer,
            b"evaluate:receiver:argument:with:",
            &[selected, left, right, execution]
        )
        .is_null()
    );
    assert_eq!(hooks.count(not_entered, 1), 0);
    call(execution_vt, b"addMethod", &[selector, evaluator]);
}
