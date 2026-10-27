//! Authored signed arithmetic over the native Integer storage owner.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Scope};
use std::os::unix::ffi::OsStrExt;

fn number(h: &Context, q: &Scope<'_>, value: i64) -> Oop {
    let word = q.word(value as usize).unwrap();
    q.call(h.named(b"Integer").unwrap(), b"fromWord:", &[word])
        .unwrap()
}
fn read(q: &Scope<'_>, value: Oop) -> i64 {
    q.word_bits(q.call(value, b"word", &[]).unwrap()).unwrap() as i64
}

#[test]
#[ignore = "requires production Integer policy artifact"]
fn signed_integer_policy_preserves_bounds_errors_and_float_dispatch() {
    let h = oop::bootstrap();
    let held = h.scope();
    let gc = h.named(b"GC").unwrap();
    held.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let module = h.global(&held, b"Module").unwrap();
    let policy = h.intern(b"integerValues").unwrap();
    assert_eq!(
        held.call(module, b"isLoaded:", &[policy]).unwrap(),
        h.named(b"true").unwrap(),
        "production bootstrap must install Integer policy"
    );
    let path = held
        .string(
            h.resources
                .lib_dir
                .join("libbirnam_integerValues.dylib")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            held.call(h.named(b"Codegen").unwrap(), b"load:", &[path])
                .unwrap(),
            module
        );
        assert_eq!(
            held.call(module, b"isLoaded:", &[policy]).unwrap(),
            h.named(b"true").unwrap()
        );
    }
    let samples = [
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
    for &left in &samples {
        let lhs = h.scope();
        let a = number(&h, &lhs, left);
        for &right in &samples {
            let rhs = h.scope();
            let b = number(&h, &rhs, right);
            for (selector, operation) in [
                (b"+".as_slice(), 0),
                (b"-", 1),
                (b"*", 2),
                (b"/", 3),
                (b"mod:", 4),
            ] {
                let q = h.scope();
                let result = q.call(a, selector, &[b]).unwrap();
                let expected = match operation {
                    0 => Some(left as i128 + right as i128),
                    1 => Some(left as i128 - right as i128),
                    2 => Some(left as i128 * right as i128),
                    3 if right != 0 => Some(left as i128 / right as i128),
                    4 if right != 0 => Some(left as i128 % right as i128),
                    _ => None,
                };
                match expected.and_then(|v| i64::try_from(v).ok()) {
                    Some(value) => assert_eq!(
                        read(&q, result),
                        value,
                        "{left} {} {right}",
                        String::from_utf8_lossy(selector)
                    ),
                    None => assert_eq!(
                        q.call(result, b"kind", &[]).unwrap(),
                        h.intern(if right == 0 && operation >= 3 {
                            b"zeroDivide"
                        } else {
                            b"overflow"
                        })
                        .unwrap()
                    ),
                }
            }
        }
    }
    let a = number(&h, &held, 2);
    let half = held
        .call(
            h.named(b"Float").unwrap(),
            b"fromBits:",
            &[held.word(0.5f64.to_bits() as usize).unwrap()],
        )
        .unwrap();
    for (selector, expected) in [
        (b"+".as_slice(), 2.5),
        (b"-", 1.5),
        (b"*", 1.0),
        (b"/", 4.0),
    ] {
        let q = h.scope();
        let result = q.call(a, selector, &[half]).unwrap();
        let bits = q.word_bits(q.call(result, b"bits", &[]).unwrap()).unwrap();
        assert_eq!(f64::from_bits(bits as u64), expected);
    }
    for (selector, other) in [(b"mod:".as_slice(), half), (b"+", h.named(b"nil").unwrap())] {
        let q = h.scope();
        let error = q.call(a, selector, &[other]).unwrap();
        let kind = q.call(error, b"kind", &[]).unwrap();
        let message = q
            .string_bytes(q.call(error, b"message", &[]).unwrap())
            .unwrap();
        assert_eq!(
            kind,
            h.intern(b"typeMismatch").unwrap(),
            "{} -> {:?}: {}",
            String::from_utf8_lossy(selector),
            q.symbol_text(kind),
            String::from_utf8_lossy(&message)
        );
    }
    let error = held
        .call(
            h.named(b"Error").unwrap(),
            b"kind:",
            &[h.intern(b"sentinel").unwrap()],
        )
        .unwrap();
    for selector in [b"+".as_slice(), b"-", b"*", b"/", b"mod:"] {
        assert_eq!(held.call(a, selector, &[error]).unwrap(), error);
    }
    held.call(gc, b"budget:", &[size(1).unwrap()]).unwrap();
    let result = held.call(a, b"*", &[a]).unwrap();
    assert_eq!(read(&held, result), 4);
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    held.call(gc, b"budget:", &[size(1024 * 1024).unwrap()])
        .unwrap();
    // Authored policy remains reflected through its named Method implementation.
    // No NativeEntry-to-implementation interface is needed in the Oop model.
    let method = held
        .call(h.id.root, b"bind:to:", &[h.intern(b"+").unwrap(), a])
        .unwrap();
    let implementation = held
        .call(
            h.named(b"Codegen").unwrap(),
            b"implementationOf:",
            &[method],
        )
        .unwrap();
    assert_ne!(implementation, h.named(b"nil").unwrap());
    let code = held.call(implementation, b"codeObject", &[]).unwrap();
    assert_ne!(code, h.named(b"nil").unwrap());
    held.check_result(code).unwrap();
}
