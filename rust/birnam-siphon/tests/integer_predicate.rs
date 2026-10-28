//! Explicit Integer predicate evaluation on the native owner floor.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
#[path = "support/oop.rs"]
#[allow(dead_code)]
mod oop;
use birnam_abi::{Oop, size};
#[test]
#[ignore = "requires production Oop native owners"]
fn explicit_predicate_observes_permissions_and_commits_once() {
    let host = minimal::native_floor();
    let h = minimal::H::new(&host);
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let integer = h.named(b"Integer");
    let gc = h.named(b"GC");
    let truth = h.named(b"true");
    let falsity = h.named(b"false");
    let path = std::env::temp_dir().join(format!(
        "birnam-integer-predicate-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/integer-predicate.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = hooks.context(&[gc, size(0).unwrap(), h.named(b"nil"), std::ptr::null_mut()]);
    let visitor = h.call(h.named(b"Object"), b"clone", &[]);
    let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[visitor]);
    let commit = h.key(b"commitNumericPredicate:");
    let method = h.call(
        h.call(fixture, b"methodAt:", &[commit]),
        b"withContext:",
        &[state],
    );
    h.call(vt, b"addMethod", &[commit, method]);
    let expression = h.key(b"evaluatePredicate:then:left:right:limit:in:epoch:with:");
    let epoch = h.key(b"behaviorEpoch");
    let raw = |r, s, a: &[Oop]| unsafe { host.id.send(r, s, a) };
    let number = |n: i64| h.call(integer, b"fromWord:", &[q.word(n as usize).unwrap()]);
    let evaluate = |first: &[u8], then: &[u8], inputs: [Oop; 3], stamp| {
        raw(
            integer,
            expression,
            &[
                h.key(first),
                h.key(then),
                inputs[0],
                inputs[1],
                inputs[2],
                host.namespace,
                stamp,
                visitor,
            ],
        )
    };
    assert_eq!(
        h.call(integer, b"hasBootstrapArithmetic:", &[h.key(b"+")]),
        truth
    );
    for (first, then, values, expected) in [
        ("+", "<", [4, 5, 20], truth),
        ("-", ">", [4, 5, 0], falsity),
        ("*", "<=", [-3, 7, -21], truth),
        ("/", ">=", [-11, 3, -3], truth),
        ("mod:", "=", [i64::MIN, -1, 0], truth),
        ("+", "<", [1 << 53, 0, (1 << 53) + 1], falsity),
        ("+", "=", [1 << 53, 0, (1 << 53) + 1], falsity),
    ] {
        let inputs = values.map(number);
        hooks.put(state, 1, size(0).unwrap());
        let stamp = raw(host.id.root, epoch, &[]).unwrap();
        assert!(
            evaluate(first.as_bytes(), then.as_bytes(), inputs, stamp)
                .unwrap()
                .is_null()
        );
        assert_eq!(
            hooks.count(state, 1),
            1,
            "accepted predicate commits exactly once"
        );
        assert_eq!(hooks.field(state, 2), expected);
    }
    for (op, values) in [
        (b"+".as_slice(), [i64::MAX, 1, 0]),
        (b"/", [1, 0, 0]),
        (b"/", [i64::MIN, -1, 0]),
    ] {
        let inputs = values.map(number);
        hooks.put(state, 1, size(0).unwrap());
        let stamp = raw(host.id.root, epoch, &[]).unwrap();
        assert!(evaluate(op, b"<", inputs, stamp).unwrap().is_null());
        assert_eq!(
            hooks.count(state, 1),
            0,
            "refused arithmetic cannot publish"
        );
    }
    let inputs = [4, 5, 20].map(number);
    hooks.put(state, 1, size(0).unwrap());
    let stamp = raw(host.id.root, epoch, &[]).unwrap();
    assert!(
        evaluate(
            b"+",
            b"<",
            inputs,
            size(birnam_abi::as_size(stamp).unwrap() + 1).unwrap()
        )
        .unwrap()
        .is_null()
    );
    assert_eq!(hooks.count(state, 1), 0);
    let ivt = h.named(b"IntegerVt");
    let add = h.key(b"addMethod");
    for selector in [b"+".as_slice(), b"<", b"fromWord:", b"wordOf:"] {
        let key = h.key(selector);
        let original = h.call(ivt, b"lookup", &[key]);
        let denied = hooks.context(&[hooks.fault(41), size(0).unwrap()]);
        raw(ivt, add, &[key, hooks.method(b"deny", denied)]).unwrap();
        let stamp = raw(host.id.root, epoch, &[]).unwrap();
        let result = evaluate(b"+", b"<", inputs, stamp);
        raw(ivt, add, &[key, original]).unwrap();
        assert!(result.unwrap().is_null());
        assert_eq!(hooks.count(state, 1), 0);
        assert_eq!(
            hooks.count(denied, 1),
            0,
            "refusal must not enter changed numeric meanings"
        );
    }
    let old_meta = h.call(host.id.root, b"behaviorOf:", &[vt]);
    let custom = h.call(old_meta, b"delegated", &[]);
    let lookup = h.key(b"lookup");
    let original = h.call(old_meta, b"lookup", &[lookup]);
    let alias = h.key(b"predicateOriginalLookup:");
    h.call(custom, b"addMethod", &[alias, original]);
    let lookup_state = hooks.context(&[alias, size(0).unwrap()]);
    h.call(
        custom,
        b"addMethod",
        &[lookup, hooks.method(b"lookupForward", lookup_state)],
    );
    h.call(host.id.root, b"reassociateBehaviorOf:with:", &[vt, custom]);
    let stamp = raw(host.id.root, epoch, &[]).unwrap();
    assert!(evaluate(b"+", b"<", inputs, stamp).unwrap().is_null());
    assert_eq!(hooks.count(state, 1), 0);
    assert_eq!(hooks.count(lookup_state, 1), 0);
    h.call(
        host.id.root,
        b"reassociateBehaviorOf:with:",
        &[vt, old_meta],
    );
    let failure = hooks.fault(73);
    hooks.put(state, 3, failure);
    let stamp = raw(host.id.root, epoch, &[]).unwrap();
    assert_eq!(evaluate(b"+", b"<", inputs, stamp), Err(failure));
    assert_eq!(hooks.count(state, 1), 1);
    hooks.put(state, 3, std::ptr::null_mut());
    hooks.put(state, 1, size(0).unwrap());
    let stamp = raw(host.id.root, epoch, &[]).unwrap();
    assert!(evaluate(b"+", b"<", inputs, stamp).unwrap().is_null());
    assert_eq!(hooks.count(state, 1), 1);
    assert_eq!(hooks.field(state, 2), truth);
}
