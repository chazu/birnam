//! Published arithmetic/comparison keeps current effects and inspectable intermediates.
use super::*;
fn number(h: &H<'_>, value: i64) -> Oop {
    let q = h.host.scope();
    h.call(
        h.named(b"Integer"),
        b"fromWord:",
        &[q.word(value as usize).unwrap()],
    )
}
fn inputs(h: &H<'_>, values: [i64; 3]) -> [Oop; 3] {
    let values = values.map(|v| number(h, v));
    for (name, value) in [b"PredicateA".as_slice(), b"PredicateB", b"PredicateC"]
        .into_iter()
        .zip(values)
    {
        h.bind(name, value);
    }
    values
}
fn source(first: &str, then: &str) -> Vec<u8> {
    format!("[:a :b :c | let answer := [[a {first} b] {then} c]; answer]").into_bytes()
}
pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let q = h.host.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-compiler-predicate-{}.dylib",
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
    let fixture = h.host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let forward = q.call(fixture, b"methodAt:", &[h.key(b"forward")]).unwrap();
    let integer = h.named(b"Integer");
    let ivt = h.named(b"IntegerVt");
    let gc = h.named(b"GC");
    let model = h.named(b"ObjectModel");
    let truth = h.named(b"true");
    let falsity = h.named(b"false");
    let candidate = h.eval(&source("+", "<"));
    h.bind(b"NumericPredicate", candidate);
    inputs(h, [4, 5, 20]);
    let block=h.block(h.compile(b"let predicateAnswer := [NumericPredicate value: PredicateA value: PredicateB value: PredicateC]; predicateAnswer"));
    let machine = h.named(b"Machine");
    let run = || h.call(h.call(machine, b"new", &[]), b"runBlock:", &[block]);
    let evaluate = h.key(b"evaluatePredicate:then:left:right:limit:in:epoch:with:");
    let original_evaluate = h.call(ivt, b"lookup", &[evaluate]);
    let evaluated = h.hooks.context(&[original_evaluate, size(0).unwrap()]);
    h.call(
        ivt,
        b"addMethod",
        &[evaluate, h.hooks.method(b"counted", evaluated)],
    );
    let result = run();
    h.call(ivt, b"addMethod", &[evaluate, original_evaluate]);
    assert_eq!(result, truth);
    eprintln!(
        "automatic Integer predicate calls={}",
        h.hooks.count(evaluated, 1)
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
        h.bind(b"NumericPredicate", h.eval(&source(first, then)));
        inputs(h, values);
        assert_eq!(run(), expected, "{first} then {then}: {values:?}");
    }
    for (first, values, kind) in [
        ("+", [i64::MAX, 1, 0], b"overflow".as_slice()),
        ("/", [1, 0, 0], b"zeroDivide"),
        ("/", [i64::MIN, -1, 0], b"overflow"),
    ] {
        h.bind(b"NumericPredicate", h.eval(&source(first, "<")));
        inputs(h, values);
        assert_eq!(h.call(run(), b"kind", &[]), h.key(kind));
    }
    h.bind(b"NumericPredicate", candidate);
    let original_inputs = inputs(h, [4, 5, 20]);
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { h.host.id.send(r, h.key(n), a) };
    for (receiver, selector) in [
        (original_inputs[0], b"+".as_slice()),
        (original_inputs[0], b"<"),
        (integer, b"fromWord:"),
        (integer, b"wordOf:"),
        (model, b"allocate:bytes:"),
        (gc, b"prepareAllocation"),
    ] {
        let vt = h.vt(receiver);
        let key = h.key(selector);
        let original = h.call(h.host.id.root, b"bind:to:", &[key, receiver]);
        let state = h.hooks.context(&[
            original,
            size(0).unwrap(),
            if selector == b"+" {
                original_inputs[0]
            } else {
                std::ptr::null_mut()
            },
        ]);
        let replacement = q.call(forward, b"withContext:", &[state]).unwrap();
        let target = h.call(machine, b"new", &[]);
        raw(vt, b"addMethod", &[key, replacement]).unwrap();
        let result = raw(target, b"runBlock:", &[block]);
        raw(vt, b"addMethod", &[key, original]).unwrap();
        assert_eq!(result.unwrap(), truth);
        let calls = h.hooks.count(state, 1);
        assert!(
            calls > 0,
            "current operation {} must run",
            String::from_utf8_lossy(selector)
        );
        if selector == b"+" || selector == b"<" {
            assert_eq!(
                calls,
                1,
                "source operation {}",
                String::from_utf8_lossy(selector)
            );
        }
    }
    let add = h.call(ivt, b"lookup", &[h.key(b"+")]);
    let less = h.call(ivt, b"lookup", &[h.key(b"<")]);
    for (selector, original) in [(b"+".as_slice(), add), (b"<", less)] {
        let state = h.hooks.context(&[original, gc, size(0).unwrap()]);
        let method = h.hooks.method(b"collectOnce", state);
        raw(ivt, b"addMethod", &[h.key(selector), method]).unwrap();
        let result = run();
        raw(ivt, b"addMethod", &[h.key(selector), original]).unwrap();
        assert_eq!(result, truth);
        assert_eq!(h.hooks.count(state, 2), 1);
    }
    let changed = h
        .hooks
        .context(&[add, size(0).unwrap(), original_inputs[0], number(h, 30)]);
    raw(
        ivt,
        b"addMethod",
        &[
            h.key(b"+"),
            q.call(
                q.call(fixture, b"methodAt:", &[h.key(b"replace")]).unwrap(),
                b"withContext:",
                &[changed],
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let result = run();
    raw(ivt, b"addMethod", &[h.key(b"+"), add]).unwrap();
    assert_eq!(result, falsity);
    assert_eq!(h.hooks.count(changed, 1), 1);
    h.bind(b"NumericPredicate", h.eval(&source("+", "*")));
    assert_eq!(h.integer(run()), 180);
    let bound = h.eval(b"[:a :b :c | let sum := [a + b]; let answer := [sum < c]; answer]");
    h.bind(b"NumericPredicate", bound);
    assert_eq!(run(), truth);
    h.call(ivt, b"addMethod", &[h.key(b"sum:"), add]);
    h.call(ivt, b"addMethod", &[h.key(b"below:"), less]);
    h.bind(b"NumericPredicate", h.eval(&source("sum:", "below:")));
    assert_eq!(run(), truth);
    h.call(ivt, b"removeMethod:", &[h.key(b"sum:")]);
    h.call(ivt, b"removeMethod:", &[h.key(b"below:")]);
    h.bind(b"NumericPredicate", candidate);
    let error_vt = h.named(b"ErrorVt");
    h.bind(b"ErrorVt", ivt);
    let result = run();
    h.bind(b"ErrorVt", error_vt);
    assert_eq!(result, original_inputs[2]);
    h.bind(b"PredicateA", h.eval(b"4.5"));
    assert_eq!(run(), truth);
    h.bind(b"PredicateA", original_inputs[0]);
    let target = h.call(machine, b"new", &[]);
    let one = number(h, 1);
    let mut result = h.call(target, b"runBlock:budget:", &[block, one]);
    let mut observed = false;
    for _ in 0..4096 {
        if result != target {
            break;
        }
        let view = h.call(target, b"currentFrameView", &[]);
        if view != h.named(b"nil") {
            let count = h.integer(h.call(view, b"slotCount", &[]));
            for index in 0..count {
                let index = size(index as usize).unwrap();
                let value = h.host.send(view, b"oopAt:", &[index]).unwrap();
                if !value.is_null()
                    && h.host
                        .send(h.host.id.root, b"behaviorOf:", &[value])
                        .unwrap()
                        == ivt
                    && h.integer(value) == 9
                {
                    let identity = h
                        .host
                        .send(h.host.id.root, b"identityOf:", &[value])
                        .unwrap();
                    h.collect();
                    let after = h.host.send(view, b"oopAt:", &[index]).unwrap();
                    assert_eq!(
                        h.host
                            .send(h.host.id.root, b"identityOf:", &[after])
                            .unwrap(),
                        identity,
                        "paused frame retains the actual intermediate"
                    );
                    observed = true;
                }
            }
        }
        result = h.call(target, b"continueWithBudget:", &[one]);
    }
    assert_eq!(result, truth);
    assert!(
        observed,
        "paused execution exposes the intermediate Integer"
    );
    for name in [
        b"NumericPredicate".as_slice(),
        b"PredicateA",
        b"PredicateB",
        b"PredicateC",
        b"predicateAnswer",
    ] {
        h.bind(name, h.named(b"nil"));
    }
    h.call(h.named(b"Codegen"), b"evictMaterializations", &[]);
    drop(q);
    h.clear();
    h.collect();
    assert_eq!(h.units(), baseline_units);
    assert_eq!(h.native_bytes(), baseline_bytes);
}
