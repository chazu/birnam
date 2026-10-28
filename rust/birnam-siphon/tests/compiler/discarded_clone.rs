//! Discarded clone sends retain current effects and inspectable frame values.
use super::*;
pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let q = h.host.scope();
    let gc = h.named(b"GC");
    let model = h.named(b"ObjectModel");
    let behavior = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
    let prototype = h.call(model, b"allocate:bytes:", &[behavior, size(0).unwrap()]);
    let answer = h.eval(b"42");
    h.bind(b"CloneSource", prototype);
    h.bind(b"CloneAnswer", answer);
    let path = std::env::temp_dir().join(format!(
        "birnam-compiler-clone-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/compiler-clone.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let observer = q.call(h.named(b"Object"), b"clone", &[]).unwrap();
    let ovt = q
        .call(h.host.id.root, b"ensurePrivateVtableFor:", &[observer])
        .unwrap();
    for n in [
        b"allocation:bytes:identity:".as_slice(),
        b"instances",
        b"present:",
    ] {
        let method = q
            .call(
                q.call(fixture, b"methodAt:", &[h.key(n)]).unwrap(),
                b"withContext:",
                &[behavior],
            )
            .unwrap();
        q.call(ovt, b"addMethod", &[h.key(n), method]).unwrap();
    }
    let count = || as_size(h.host.send(observer, b"instances", &[]).unwrap()).unwrap();
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { h.host.id.send(r, h.key(n), a) };
    let candidate = h.eval(b"[:p :answer | [p clone]; answer]");
    h.bind(b"DiscardClone", candidate);
    let block = h.block(
        h.compile(b"let result := [DiscardClone value: CloneSource value: CloneAnswer]; result"),
    );
    let machine = h.named(b"Machine");
    let run = || {
        let target = h.call(machine, b"new", &[]);
        h.keep(raw(target, b"runBlock:", &[block]).unwrap())
    };
    h.collect();
    let before = count();
    assert_eq!(run(), answer);
    eprintln!("discarded clone: physical copies={}", count() - before);
    let clone = h.call(behavior, b"lookup", &[h.key(b"clone")]);
    for collecting in [false, true] {
        let state = if collecting {
            h.hooks.context(&[clone, gc, size(0).unwrap()])
        } else {
            h.hooks.context(&[clone, size(0).unwrap()])
        };
        let callback = h.hooks.method(
            if collecting {
                b"collectOnce"
            } else {
                b"counted"
            },
            state,
        );
        raw(behavior, b"addMethod", &[h.key(b"clone"), callback]).unwrap();
        let result = run();
        raw(behavior, b"addMethod", &[h.key(b"clone"), clone]).unwrap();
        assert_eq!(result, answer);
        assert_eq!(h.hooks.count(state, if collecting { 2 } else { 1 }), 1);
    }
    let permission = || raw(model, b"canDiscardClone:of:", &[clone, prototype]).unwrap();
    assert_eq!(permission(), size(1).unwrap());
    let wrong = h.hooks.method(b"fixed", prototype);
    assert_eq!(
        raw(model, b"canDiscardClone:of:", &[wrong, prototype]).unwrap(),
        size(0).unwrap()
    );
    // The explicit owner permission observes every declared dependency. Clone
    // itself already used a private allocation helper before the Oop migration;
    // public ObjectModel allocation is not an extra self-send in that operation.
    for (receiver, selector) in [
        (model, b"allocate:bytes:".as_slice()),
        (gc, b"prepareAllocation"),
        (behavior, b"allocate"),
    ] {
        let vt = h.vt(receiver);
        let key = h.key(selector);
        let original = h.call(h.host.id.root, b"bind:to:", &[key, receiver]);
        assert!(!original.is_null());
        let state = h.hooks.context(&[original, size(0).unwrap()]);
        let callback = h.hooks.method(b"counted", state);
        let target = h.call(machine, b"new", &[]);
        h.collect();
        let before = count();
        raw(vt, b"addMethod", &[key, callback]).unwrap();
        assert_eq!(permission(), size(0).unwrap());
        let result = raw(target, b"runBlock:", &[block]);
        raw(vt, b"addMethod", &[key, original]).unwrap();
        assert_eq!(result.unwrap(), answer);
        if receiver == model {
            assert_eq!(
                h.hooks.count(state, 1),
                0,
                "clone does not call the public allocation wrapper"
            );
        } else {
            assert!(
                h.hooks.count(state, 1) > 0,
                "current clone allocation operation must run"
            );
        }
        assert_eq!(permission(), size(1).unwrap());
        assert_eq!(
            count() - before,
            1,
            "replaced allocation path observes the discarded clone"
        );
    }
    raw(behavior, b"addMethod", &[h.key(b"discardableCopy"), clone]).unwrap();
    let alias = h.eval(b"[:p :answer | [p discardableCopy]; answer]");
    h.bind(b"DiscardClone", alias);
    assert_eq!(run(), answer);
    h.bind(b"DiscardClone", candidate);
    let nonempty = h.call(
        model,
        b"allocate:bytes:",
        &[behavior, size(std::mem::size_of::<Oop>()).unwrap()],
    );
    assert_eq!(
        raw(model, b"canDiscardClone:of:", &[clone, nonempty]).unwrap(),
        size(0).unwrap()
    );
    h.bind(b"CloneSource", nonempty);
    h.collect();
    let before = count();
    assert_eq!(run(), answer);
    assert_eq!(count() - before, 1);
    h.bind(b"CloneSource", prototype);
    let bound = h.eval(b"[:p :answer | let discarded := [p clone]; answer]");
    h.bind(b"DiscardClone", bound);
    h.collect();
    let before = count();
    assert_eq!(run(), answer);
    assert_eq!(count() - before, 1);
    h.bind(b"DiscardClone", candidate);
    h.bind(b"CloneAnswer", prototype);
    assert_eq!(run(), prototype);
    h.bind(b"CloneAnswer", answer);
    let escape = h.eval(b"[:p :answer | [p clone]]");
    h.bind(b"DiscardClone", escape);
    let copy = run();
    assert_ne!(copy, prototype);
    assert_ne!(copy, answer);
    assert_eq!(h.vt(copy), behavior);
    h.bind(b"DiscardClone", candidate);
    // Read frame slots without retaining their values in the test scope. The
    // paused frame must keep a discarded copy alive during precise collection.
    let target = h.call(machine, b"new", &[]);
    let one = h.eval(b"1");
    let mut result = h.call(target, b"runBlock:budget:", &[block, one]);
    let mut observed = false;
    for _ in 0..128 {
        if result != target {
            break;
        }
        let view = h.call(target, b"currentFrameView", &[]);
        if view != h.named(b"nil") {
            let count = h.integer(h.call(view, b"slotCount", &[]));
            for index in 0..count {
                let value = h
                    .host
                    .send(view, b"oopAt:", &[size(index as usize).unwrap()])
                    .unwrap();
                if !value.is_null()
                    && value != prototype
                    && h.host
                        .send(h.host.id.root, b"behaviorOf:", &[value])
                        .unwrap()
                        == behavior
                {
                    let identity = h
                        .host
                        .send(h.host.id.root, b"identityOf:", &[value])
                        .unwrap();
                    h.collect();
                    assert_eq!(
                        raw(observer, b"present:", &[identity]).unwrap(),
                        size(1).unwrap()
                    );
                    observed = true;
                }
            }
        }
        result = h.call(target, b"continueWithBudget:", &[one]);
    }
    assert_eq!(result, answer);
    assert!(observed, "bounded execution exposes its discarded clone");
    for name in [
        b"DiscardClone".as_slice(),
        b"CloneSource",
        b"CloneAnswer",
        b"result",
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
