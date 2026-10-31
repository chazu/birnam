//! Exercise the native seed directly: authored booleanValues is never loaded.
#[allow(dead_code)]
#[path = "support/minimal.rs"]
mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use std::ptr;

#[test]
#[ignore = "requires native modules"]
fn native_boolean_preserves_validation_selection_gc_and_live_dispatch() {
    let h = minimal::native_floor();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let yes = h.named(b"true").unwrap();
    let no = h.named(b"false").unwrap();
    let nil = h.named(b"nil").unwrap();
    let vt = call(h.id.root, b"behaviorOf:", &[yes]);
    assert_eq!(call(h.id.root, b"behaviorOf:", &[no]), vt);
    let other = call(vt, b"allocate", &[size(0).unwrap()]);
    let make_branch = |answer| {
        let behavior = call(h.named(b"ObjectVt").unwrap(), b"delegated", &[]);
        let context = hooks.context(&[answer, size(0).unwrap()]);
        let method = hooks.method(b"fixedCount", context);
        call(behavior, b"addMethod", &[hooks.symbol(b"value"), method]);
        (
            call(behavior, b"allocate", &[size(0).unwrap()]),
            behavior,
            context,
            method,
        )
    };
    let (left, left_vt, left_count, left_method) = make_branch(yes);
    let (right, _, right_count, _) = make_branch(no);
    for receiver in [yes, no, other] {
        assert_eq!(call(receiver, b"==", &[receiver]), yes);
        assert_eq!(call(receiver, b"==", &[nil]), no);
        assert_eq!(
            call(receiver, b"not", &[]),
            if receiver == yes {
                no
            } else if receiver == no {
                yes
            } else {
                nil
            }
        );
        assert_eq!(
            call(receiver, b"&", &[left]),
            if receiver == yes {
                left
            } else if receiver == no {
                no
            } else {
                nil
            }
        );
        assert_eq!(
            call(receiver, b"|", &[left]),
            if receiver == no {
                left
            } else if receiver == yes {
                yes
            } else {
                nil
            }
        );
        // Eager Boolean operations return data; they must not invoke it.
        assert_eq!(hooks.count(left_count, 1), 0);
        for (selector, arity) in [
            (b"==".as_slice(), 1),
            (b"not", 0),
            (b"&", 1),
            (b"|", 1),
            (b"ifTrue:ifFalse:", 2),
            (b"ifTrue:", 1),
            (b"ifFalse:", 1),
            (b"and:", 1),
            (b"or:", 1),
        ] {
            for count in 0..4 {
                if count != arity {
                    assert!(h.send(receiver, selector, &vec![left; count]).is_err());
                }
            }
            for index in 0..arity {
                let mut args = vec![left; arity];
                args[index] = ptr::null_mut();
                assert!(
                    h.send(receiver, selector, &args).is_err(),
                    "null argument to {:?}",
                    selector
                );
            }
        }
        for (selector, args, expected, lc, rc) in [
            (
                b"ifTrue:ifFalse:".as_slice(),
                vec![left, right],
                if receiver == yes {
                    yes
                } else if receiver == no {
                    no
                } else {
                    nil
                },
                usize::from(receiver == yes),
                usize::from(receiver == no),
            ),
            (
                b"ifTrue:",
                vec![left],
                if receiver == yes { yes } else { nil },
                usize::from(receiver == yes),
                0,
            ),
            (
                b"ifFalse:",
                vec![left],
                if receiver == no { yes } else { nil },
                usize::from(receiver == no),
                0,
            ),
            (
                b"and:",
                vec![left],
                if receiver == yes {
                    yes
                } else if receiver == no {
                    no
                } else {
                    nil
                },
                usize::from(receiver == yes),
                0,
            ),
            (
                b"or:",
                vec![left],
                if receiver == no {
                    yes
                } else if receiver == yes {
                    yes
                } else {
                    nil
                },
                usize::from(receiver == no),
                0,
            ),
        ] {
            hooks.put(left_count, 1, size(0).unwrap());
            hooks.put(right_count, 1, size(0).unwrap());
            assert_eq!(call(receiver, selector, &args), expected);
            assert_eq!(hooks.count(left_count, 1), lc);
            assert_eq!(hooks.count(right_count, 1), rc);
        }
        hooks.put(left_count, 1, size(0).unwrap());
    }
    let captured = call(
        call(vt, b"lookup", &[hooks.symbol(b"ifTrue:")]),
        b"captured",
        &[],
    );
    let gc = h.named(b"GC").unwrap();
    let collecting = hooks.context(&[left_method, gc, size(0).unwrap()]);
    call(
        left_vt,
        b"addMethod",
        &[
            hooks.symbol(b"value"),
            hooks.method(b"collectEvery", collecting),
        ],
    );
    for answer in [nil, no, other, size(123).unwrap(), ptr::null_mut()] {
        hooks.put(left_count, 0, answer);
        assert_eq!(call(yes, b"ifTrue:", &[left]), answer);
    }
    let fault = hooks.fault(417);
    let denied = hooks.context(&[fault, size(0).unwrap()]);
    call(
        left_vt,
        b"addMethod",
        &[hooks.symbol(b"value"), hooks.method(b"deny", denied)],
    );
    assert_eq!(
        unsafe { h.id.send(yes, hooks.symbol(b"ifTrue:"), &[left]) }.unwrap_err(),
        fault
    );
    assert_eq!(call(no, b"ifTrue:", &[left]), nil);
    assert_eq!(hooks.count(denied, 1), 1, "unchosen branch was invoked");
    let replacement = hooks.method(b"fixedCount", hooks.context(&[other, size(0).unwrap()]));
    call(vt, b"addMethod", &[hooks.symbol(b"ifTrue:"), replacement]);
    assert_eq!(call(yes, b"ifTrue:", &[left]), other);
    let args = call(h.id.root, b"sequence", &[left]);
    assert_eq!(
        unsafe {
            h.id.send(captured, hooks.symbol(b"invokeOn:arguments:"), &[yes, args])
        }
        .unwrap_err(),
        fault
    );
    call(yes, b"installPrinting:", &[h.named(b"String").unwrap()]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        q.string_bytes(call(yes, b"printString", &[])).unwrap(),
        b"true"
    );
    assert_eq!(
        q.string_bytes(call(no, b"printString", &[])).unwrap(),
        b"false"
    );
    assert_eq!(call(other, b"printString", &[]), nil);
    assert!(
        h.send(yes, b"installPrinting:", &[h.named(b"String").unwrap()])
            .is_err()
    );
}
