//! Authored signed arithmetic over the retained native Integer storage seed.
mod support;
use birnam_abi::{Oop, id_protocol::Operand};
use std::os::unix::ffi::OsStrExt;
use support::*;
#[test]
#[ignore = "production Integer policy artifact required"]
fn signed_integer_policy_preserves_bounds_errors_and_float_dispatch() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let (integer, roots, gc) = (named(b"Integer"), named(b"Roots"), named(b"GC"));
        let module = object(send(
            id,
            named(b"Globals"),
            b"at:",
            &[Operand::object(id.intern_name(b"Module").unwrap())],
        ));
        let policy = id.intern_name(b"integerValues").unwrap();
        assert_eq!(
            send(id, module, b"isLoaded:", &[Operand::object(policy)]).as_object(),
            Some(named(b"false")),
            "Integer policy must be opt-in until performance qualification"
        );
        // Raw bootstrap has no stdlib Module planning helpers. Admit through
        // Codegen's current Id artifact interface, as bootstrap itself does.
        let path = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(
                directory()
                    .join("libbirnam_integerValues.dylib")
                    .as_os_str()
                    .as_bytes(),
            )],
        ));
        let path_token = retain(id, roots, path);
        for _ in 0..2 {
            assert_eq!(
                send(id, named(b"Codegen"), b"load:", &[Operand::object(path)]).as_object(),
                Some(module)
            );
            assert_eq!(
                send(id, module, b"isLoaded:", &[Operand::object(policy)]).as_object(),
                Some(named(b"true"))
            );
        }
        release(id, roots, path_token);

        let number = |value: i64| {
            object(send(
                id,
                integer,
                b"fromWord:",
                &[Operand::word(value as usize)],
            ))
        };
        let read = |value: Oop| send(id, value, b"word", &[]).as_word().unwrap() as i64;
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
            let a = number(left);
            let at = retain(id, roots, a);
            for &right in &samples {
                let b = number(right);
                let bt = retain(id, roots, b);
                for (selector, operation) in [
                    (b"+".as_slice(), 0),
                    (b"-", 1),
                    (b"*", 2),
                    (b"/", 3),
                    (b"mod:", 4),
                ] {
                    let result = object(send(id, a, selector, &[Operand::object(b)]));
                    let rt = retain(id, roots, result);
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
                            read(result),
                            value,
                            "{left} {} {right}",
                            String::from_utf8_lossy(selector)
                        ),
                        None => assert_eq!(
                            object(send(id, result, b"kind", &[])),
                            id.intern_name(if right == 0 && operation >= 3 {
                                b"zeroDivide"
                            } else {
                                b"overflow"
                            })
                            .unwrap()
                        ),
                    }
                    release(id, roots, rt);
                }
                release(id, roots, bt);
            }
            release(id, roots, at);
        }
        let a = number(2);
        let at = retain(id, roots, a);
        let half = object(send(
            id,
            named(b"Float"),
            b"fromBits:",
            &[Operand::word(0.5f64.to_bits() as usize)],
        ));
        let ht = retain(id, roots, half);
        let result = object(send(id, a, b"+", &[Operand::object(half)]));
        let rt = retain(id, roots, result);
        assert_eq!(
            f64::from_bits(send(id, result, b"bits", &[]).as_word().unwrap() as u64),
            2.5
        );
        let error = object(send(
            id,
            named(b"Error"),
            b"kind:",
            &[Operand::object(id.intern_name(b"sentinel").unwrap())],
        ));
        let et = retain(id, roots, error);
        for selector in [b"+".as_slice(), b"-", b"*", b"/", b"mod:"] {
            assert_eq!(
                send(id, a, selector, &[Operand::object(error)]).as_object(),
                Some(error)
            );
        }
        send(id, gc, b"budget:", &[Operand::word(1)]);
        let result = object(send(id, a, b"*", &[Operand::object(a)]));
        let ct = retain(id, roots, result);
        assert_eq!(read(result), 4);
        collect(id, gc);
        send(id, gc, b"budget:", &[Operand::word(1024 * 1024)]);
        let entry = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"+").unwrap()),
                Operand::object(a),
            ],
        )
        .as_entry()
        .unwrap();
        assert!(
            send(
                id,
                named(b"Execution"),
                b"implementationOfEntry:",
                &[Operand::entry(entry)]
            )
            .as_object()
            .is_some()
        );
        for token in [ct, et, rt, ht, at] {
            release(id, roots, token);
        }
    }
}
