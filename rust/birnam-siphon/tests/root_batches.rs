//! Oop cleanup preserves reverse order, live replacement, and complete draining.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Roots and GC"]
fn oop_cleanup_preserves_order_and_current_release_methods() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let key = |n: &[u8]| hooks.symbol(n);
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let path =
        std::env::temp_dir().join(format!("birnam-root-batches-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/root-batches.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let roots = host.roots;
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let objects: Vec<_> = (0..4)
        .map(|_| call(named(b"Object"), b"clone", &[]))
        .collect();
    let state = hooks.context(&[roots, call(host.id.root, b"sequence", &objects)]);
    let method = |n: &[u8], ctx| {
        call(
            call(module, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[ctx],
        )
    };
    let observer = call(named(b"Object"), b"clone", &[]);
    let ovt = call(host.id.root, b"ensurePrivateVtableFor:", &[observer]);
    for n in [b"snapshot".as_slice(), b"visit:", b"log", b"reset"] {
        call(ovt, b"addMethod", &[key(n), method(n, state)]);
    }
    let sequence = |o| {
        let n = as_size(host.send(o, b"size", &[]).unwrap()).unwrap();
        (0..n)
            .map(|i| host.send(o, b"at:", &[size(i).unwrap()]).unwrap())
            .collect::<Vec<_>>()
    };
    let snapshot = || sequence(call(observer, b"snapshot", &[]));
    let baseline = snapshot();
    let retain = key(b"retain:");
    let release = key(b"release:");
    let cleanup = key(b"releaseTokensProtecting");
    let raw = |r, s, a: &[Oop]| unsafe { host.id.send(r, s, a) };
    let retain_shape =
        || [0, 1, 0, 2, 3, 1, 2, 0].map(|i| raw(roots, retain, &[objects[i]]).unwrap());
    let tokens = retain_shape();
    assert!(
        tokens
            .windows(2)
            .all(|w| as_size(w[1]).unwrap() == as_size(w[0]).unwrap() + 1)
    );
    let absent = size(usize::MAX >> 1).unwrap();
    let request = [tokens[1], tokens[4], tokens[0]];
    for token in request.iter().rev() {
        raw(roots, release, &[*token]).unwrap();
    }
    let expected = snapshot();
    for token in tokens {
        raw(roots, release, &[token]).unwrap();
    }
    assert_eq!(snapshot(), baseline);
    let tokens = retain_shape();
    let request = [named(b"nil"), tokens[1], tokens[4], tokens[0]];
    assert_eq!(raw(roots, cleanup, &request), Ok(size(1).unwrap()));
    assert_eq!(
        snapshot(),
        expected,
        "successful cleanup preserves sequential reverse removal order"
    );
    for token in tokens {
        raw(roots, release, &[token]).unwrap();
    }
    assert_eq!(snapshot(), baseline);

    // Validate all token shapes before entering any current release callback.
    let token = raw(roots, retain, &[objects[0]]).unwrap();
    let before = snapshot();
    assert!(raw(roots, cleanup, &[objects[1], objects[2], token]).is_err());
    assert_eq!(snapshot(), before);
    assert!(raw(roots, cleanup, &[]).is_err());
    assert_eq!(
        raw(roots, cleanup, &[objects[1]]).unwrap(),
        size(1).unwrap()
    );
    assert_eq!(raw(roots, release, &[token]).unwrap(), size(1).unwrap());
    assert_eq!(snapshot(), baseline);

    let rv = call(host.id.root, b"behaviorOf:", &[roots]);
    let original = call(rv, b"lookup", &[release]);
    let add = key(b"addMethod");
    let reset = key(b"reset");
    let log = key(b"log");
    call(rv, b"addMethod", &[key(b"batchOriginalRelease:"), original]);
    let failure = hooks.fault(53);
    let later_failure = hooks.fault(54);
    let second_state = hooks.context(&[rv, std::ptr::null_mut(), gc, std::ptr::null_mut()]);
    let second = method(b"second", second_state);
    let first_state = hooks.context(&[rv, second, gc, std::ptr::null_mut()]);
    let first = method(b"first", first_state);
    for (fail, duplicate) in [(false, false), (true, false), (false, true)] {
        hooks.put(first_state, 1, second);
        hooks.put(
            first_state,
            3,
            if fail { failure } else { std::ptr::null_mut() },
        );
        hooks.put(
            second_state,
            3,
            if fail {
                later_failure
            } else {
                std::ptr::null_mut()
            },
        );
        let tokens: Vec<_> = objects
            .iter()
            .map(|o| raw(roots, retain, &[*o]).unwrap())
            .collect();
        raw(observer, reset, &[]).unwrap();
        raw(rv, add, &[release, first]).unwrap();
        let mut args = vec![objects[0]];
        args.extend_from_slice(&tokens);
        if duplicate {
            args.extend_from_slice(&[tokens[1], absent]);
        }
        let result = raw(roots, cleanup, &args);
        // Restore before Host helpers create and release their own roots.
        raw(rv, add, &[release, original]).unwrap();
        if fail {
            assert_eq!(
                result,
                Err(failure),
                "first failure wins while later releases still run"
            );
        } else if duplicate {
            assert_eq!(result, Err(birnam_abi::object_reference(host.id.invalid)));
        } else {
            assert_eq!(result, Ok(size(1).unwrap()));
        }
        let actual = sequence(q.keep(raw(observer, log, &[]).unwrap()).unwrap());
        let expected: Vec<_> = args[1..]
            .iter()
            .rev()
            .enumerate()
            .flat_map(|(i, t)| [size(if i == 0 { 1 } else { 2 }).unwrap(), *t])
            .collect();
        assert_eq!(
            actual, expected,
            "each release observes replacement installed by the preceding release"
        );
        assert_eq!(
            snapshot(),
            baseline,
            "all tokens drain even when current release callbacks fail"
        );
    }
    for selector in [
        b"tryRetainMany:".as_slice(),
        b"tryReleaseMany:",
        b"tryReleaseManyForward:",
    ] {
        assert!(
            call(host.id.root, b"bind:to:", &[key(selector), roots]).is_null(),
            "raw pointer/token byte-buffer protocols remain absent"
        );
    }
}
