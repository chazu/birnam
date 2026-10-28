//! Explicit Integer/Execution numeric evaluation; ordinary Id calls enter Methods directly.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
#[test]
#[ignore = "requires production Oop owners"]
fn numeric_owner_query_preserves_selected_meanings_and_current_visitors() {
    let native = match std::env::var("BIRNAM_NUMERIC_OWNER_CASE") {
        Ok(case) => match case.as_str() {
            "native" => true,
            "published" => false,
            _ => panic!("unknown numeric owner case"),
        },
        Err(_) => {
            let mut passed = true;
            for case in ["native", "published"] {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "numeric_owner_query_preserves_selected_meanings_and_current_visitors",
                        "--nocapture",
                    ])
                    .env("BIRNAM_NUMERIC_OWNER_CASE", case)
                    .status()
                    .unwrap();
                passed &= status.success();
            }
            assert!(passed, "both isolated numeric owner contexts must pass");
            return;
        }
    };
    let host = if native {
        minimal::native_floor()
    } else {
        oop::bootstrap()
    };
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let key = |n: &[u8]| hooks.symbol(n);
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let integer = named(b"Integer");
    let ivt = named(b"IntegerVt");
    let execution = named(b"Execution");
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let number = |n: i64| call(integer, b"fromWord:", &[q.word(n as usize).unwrap()]);
    let value = |v| q.word_bits(call(integer, b"wordOf:", &[v])).unwrap() as i64;
    let query = key(b"evaluate:receiver:argument:with:");
    let original = call(ivt, b"lookup", &[query]);
    let fault = hooks.fault(917);
    let denied = hooks.context(&[fault, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied);
    let add = key(b"addMethod");
    let raw = |r, s, a: &[Oop]| unsafe { host.id.send(r, s, a) };
    assert_eq!(
        call(integer, b"hasBootstrapArithmetic:", &[key(b"+")]),
        named(if native { b"true" } else { b"false" })
    );
    let cases = [
        (b"+".as_slice(), 42, 7, 49),
        (b"-", 42, 7, 35),
        (b"*", 42, 7, 294),
        (b"/", 42, 7, 6),
        (b"mod:", i64::MIN, -1, 0),
    ];
    for (selector, l, r, expected) in cases {
        let left = number(l);
        let right = number(r);
        let selected = call(host.id.root, b"bind:to:", &[key(selector), left]);
        raw(ivt, add, &[query, deny]).unwrap();
        for _ in 0..70 {
            assert_eq!(
                value(q.keep(raw(left, key(selector), &[right]).unwrap()).unwrap()),
                expected
            );
        }
        raw(ivt, add, &[query, original]).unwrap();
        assert_eq!(
            hooks.count(denied, 1),
            0,
            "ordinary hot sends must not add an implicit numeric-owner query"
        );
        let result = raw(integer, query, &[selected, left, right, execution]).unwrap();
        if native {
            assert_eq!(value(q.keep(result).unwrap()), expected);
        } else {
            assert!(
                result.is_null(),
                "native consumer must decline authored arithmetic"
            );
        }
    }
    let left = number(42);
    let right = number(7);
    let plus = key(b"+");
    let selected = call(host.id.root, b"bind:to:", &[plus, left]);
    let changed = hooks.context(&[right, size(0).unwrap()]);
    let replacement = hooks.method(b"fixedCount", changed);
    raw(ivt, add, &[plus, replacement]).unwrap();
    assert_eq!(raw(left, plus, &[right]).unwrap(), right);
    let current = call(host.id.root, b"bind:to:", &[plus, left]);
    assert!(
        raw(integer, query, &[current, left, right, execution])
            .unwrap()
            .is_null()
    );
    assert_eq!(
        hooks.count(changed, 1),
        1,
        "refusal must not invoke the replacement again"
    );
    let saved = raw(integer, query, &[selected, left, right, execution]).unwrap();
    raw(ivt, add, &[plus, selected]).unwrap();
    if native {
        assert_eq!(value(q.keep(saved).unwrap()), 49);
    } else {
        assert!(saved.is_null());
    }
    let visitor = named(b"Object");
    let visitor = call(visitor, b"clone", &[]);
    let vt = call(host.id.root, b"ensurePrivateVtableFor:", &[visitor]);
    let operation = key(b"numericOperation:left:right:");
    let op = call(host.id.root, b"bind:to:", &[operation, execution]);
    // The explicit native consumer observes Execution's current evaluator;
    // authored arithmetic declines before entering that evaluator.
    let forwarding = hooks.context(&[op, gc, size(0).unwrap()]);
    let ev = call(host.id.root, b"behaviorOf:", &[execution]);
    hooks.put(forwarding, 2, size(0).unwrap());
    raw(
        ev,
        add,
        &[operation, hooks.method(b"collectOnce", forwarding)],
    )
    .unwrap();
    let result = raw(integer, query, &[selected, left, right, execution]);
    raw(ev, add, &[operation, op]).unwrap();
    if native {
        assert_eq!(value(q.keep(result.unwrap()).unwrap()), 49);
        assert_eq!(hooks.count(forwarding, 2), 1);
    } else {
        assert!(result.unwrap().is_null());
        assert_eq!(hooks.count(forwarding, 2), 0);
    }
    for fail in [false, true] {
        let state = hooks.context(&[
            if fail { fault } else { std::ptr::null_mut() },
            size(0).unwrap(),
        ]);
        let method = hooks.method(if fail { b"deny" } else { b"fixedCount" }, state);
        raw(vt, add, &[operation, method]).unwrap();
        let result = raw(integer, query, &[selected, left, right, visitor]);
        if !native {
            assert!(result.unwrap().is_null());
            assert_eq!(hooks.count(state, 1), 0);
        } else if fail {
            assert_eq!(result, Err(fault));
            assert_eq!(hooks.count(state, 1), 1);
        } else {
            assert_eq!(value(q.keep(result.unwrap()).unwrap()), 49);
            assert_eq!(hooks.count(state, 1), 1);
        }
    }
    let changed_context = call(selected, b"withContext:", &[named(b"nil")]);
    assert!(
        raw(integer, query, &[changed_context, left, right, visitor])
            .unwrap()
            .is_null()
    );
    eprintln!(
        "numeric owner context {}: direct sends, explicit evaluation, selected meaning and live visitors passed",
        if native { "native" } else { "published" }
    );
}
