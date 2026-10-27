//! Authored Boolean behavior preserves Oop identity and current dispatch.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};

fn require_failure(
    label: &str,
    result: Result<Oop, Oop>,
    expected: Option<Oop>,
    failures: &mut Vec<String>,
) {
    match result {
        Err(error) => {
            if let Some(expected) = expected {
                assert_eq!(error, expected, "{label}");
            }
        }
        Ok(_) => failures.push(label.to_owned()),
    }
}

#[test]
#[ignore = "requires production Oop Boolean artifact"]
fn authored_boolean_preserves_identity_branching_oops_and_replacements() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let mut failures = Vec::new();
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let raw = |r, s, a: &[Oop]| unsafe { h.id.send(r, s, a) };
    let truth = named(b"true");
    let falsity = named(b"false");
    let nil = named(b"nil");
    let gc = named(b"GC");
    let roots = named(b"Roots");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let vt = call(h.id.root, b"behaviorOf:", &[truth]);
    let other = call(vt, b"allocate", &[size(0).unwrap()]);
    let path = std::env::temp_dir().join(format!(
        "birnam-boolean-values-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/boolean-values.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let declared = |name: &[u8]| call(fixture, b"methodAt:", &[key(name)]);
    let method = |name: &[u8], context| call(declared(name), b"withContext:", &[context]);
    let state = hooks.context(&[gc, truth, size(0).unwrap(), ptr::null_mut()]);
    let cvt = call(named(b"ObjectVt"), b"delegated", &[]);
    call(cvt, b"addMethod", &[key(b"value"), method(b"value", state)]);
    let block = call(cvt, b"allocate", &[size(0).unwrap()]);
    let selection = b"selectIfTrue:ifFalse:otherwise:";
    for (receiver, expected) in [(truth, block), (falsity, truth), (other, nil)] {
        assert_eq!(call(receiver, selection, &[block, truth, nil]), expected);
    }
    assert_eq!(hooks.count(state, 2), 0);
    let original_selection = call(h.id.root, b"bind:to:", &[key(selection), truth]);
    call(
        vt,
        b"addMethod",
        &[key(selection), method(b"alternative", state)],
    );
    assert_eq!(call(truth, selection, &[block, falsity, nil]), falsity);
    let fault = hooks.fault(719);
    hooks.put(state, 3, fault);
    assert_eq!(
        raw(truth, key(selection), &[block, falsity, nil]).unwrap_err(),
        fault
    );
    hooks.put(state, 3, ptr::null_mut());
    call(vt, b"addMethod", &[key(selection), original_selection]);
    assert_eq!(call(other, selection, &[block, block, block]), block);
    require_failure(
        "selection arity",
        raw(truth, key(selection), &[]),
        None,
        &mut failures,
    );
    require_failure(
        "selection null argument",
        raw(truth, key(selection), &[block, truth, ptr::null_mut()]),
        None,
        &mut failures,
    );

    for (r, not, and, or) in [
        (truth, falsity, block, truth),
        (falsity, truth, falsity, block),
        (other, nil, nil, nil),
    ] {
        assert_eq!(call(r, b"not", &[]), not);
        assert_eq!(call(r, b"&", &[block]), and);
        assert_eq!(call(r, b"|", &[block]), or);
    }
    for (r, selector, expected) in [
        (falsity, b"ifTrue:".as_slice(), nil),
        (truth, b"ifFalse:", nil),
        (falsity, b"and:", falsity),
        (truth, b"or:", truth),
        (other, b"ifTrue:", nil),
        (other, b"and:", nil),
    ] {
        assert_eq!(call(r, selector, &[block]), expected);
    }
    assert_eq!(hooks.count(state, 2), 0);
    let bytes = q.byte_values(b"raw-bytes").unwrap();
    let native_method = method(b"value", state);
    for value in [
        falsity,
        ptr::null_mut(),
        q.word(usize::MAX).unwrap(),
        bytes,
        native_method,
        h.id.root,
    ] {
        hooks.put(state, 1, value);
        assert_eq!(call(truth, b"ifTrue:", &[block]), value);
    }
    assert_eq!(hooks.count(state, 2), 6);
    let callback_fault = hooks.fault(413);
    hooks.put(state, 3, callback_fault);
    require_failure(
        "selected callback failure",
        raw(falsity, key(b"ifFalse:"), &[block]),
        Some(callback_fault),
        &mut failures,
    );
    hooks.put(state, 3, ptr::null_mut());
    for selector in [b"ifTrue:".as_slice(), b"&", b"|"] {
        require_failure(
            &format!("{} arity", String::from_utf8_lossy(selector)),
            raw(truth, key(selector), &[]),
            None,
            &mut failures,
        );
        require_failure(
            &format!("{} null argument", String::from_utf8_lossy(selector)),
            raw(truth, key(selector), &[ptr::null_mut()]),
            None,
            &mut failures,
        );
    }
    hooks.put(state, 1, block);
    assert_eq!(call(truth, b"ifTrue:ifFalse:", &[block, nil]), block);
    let boolean_method = call(vt, b"lookup", &[key(b"ifTrue:")]);
    let body = call(named(b"Codegen"), b"implementationOf:", &[boolean_method]);
    assert_ne!(body, nil);
    assert_ne!(call(body, b"codeObject", &[]), nil);
    let module = h.global(&q, b"Module").unwrap();
    call(
        module,
        b"libDir:",
        &[q.string(h.resources.lib_dir.as_os_str().as_bytes())
            .unwrap()],
    );
    assert_eq!(call(module, b"isLoaded:", &[key(b"booleanValues")]), truth);
    q.check_result(call(module, b"load:", &[key(b"booleanValues")]))
        .unwrap();
    let after = call(vt, b"lookup", &[key(b"ifTrue:")]);
    assert_eq!(
        as_size(call(
            h.id.root,
            b"sameNativeMethod:as:",
            &[boolean_method, after]
        )),
        Some(1)
    );

    let word = named(b"Word");
    let select_name = b"selectIfSameObject:with:ifEqual:ifDifferent:";
    let selector = key(select_name);
    let word_vt = call(h.id.root, b"behaviorOf:", &[word]);
    let original = call(h.id.root, b"bind:to:", &[selector, word]);
    let w1 = call(word, b"fromBits:", &[size(7).unwrap()]);
    let w2 = call(word, b"fromBits:", &[size(7).unwrap()]);
    for (left, right, expected) in [
        (w1, w1, block),
        (w1, w2, nil),
        (nil, nil, block),
        (truth, falsity, nil),
    ] {
        assert_eq!(
            call(word, select_name, &[left, right, block, nil]),
            expected
        );
    }
    let valid = [truth, truth, block, nil];
    for length in 0..4 {
        assert!(raw(word, selector, &valid[..length]).is_err());
    }
    assert!(raw(word, selector, &[truth, truth, block, nil, nil]).is_err());
    for index in 0..4 {
        let mut args = valid;
        args[index] = ptr::null_mut();
        assert!(raw(word, selector, &args).is_err());
    }
    // Immediate sizes are Oops, unlike the old non-object Word carrier.
    assert_eq!(
        call(
            word,
            select_name,
            &[size(7).unwrap(), size(7).unwrap(), block, nil]
        ),
        block
    );
    let model = named(b"ObjectModel");
    let model_vt = call(h.id.root, b"behaviorOf:", &[model]);
    let roots_vt = call(h.id.root, b"behaviorOf:", &[roots]);
    let allocate = key(b"allocate:bytes:");
    let retain = key(b"retain:");
    let add = key(b"addMethod");
    let old_alloc = call(h.id.root, b"bind:to:", &[allocate, model]);
    let old_retain = call(h.id.root, b"bind:to:", &[retain, roots]);
    let rejected = hooks.context(&[hooks.fault(991), size(0).unwrap()]);
    let deny = hooks.method(b"deny", rejected);
    raw(model_vt, add, &[allocate, deny]).unwrap();
    raw(roots_vt, add, &[retain, deny]).unwrap();
    assert_eq!(raw(word, selector, &valid).unwrap(), block);
    raw(roots_vt, add, &[retain, old_retain]).unwrap();
    raw(model_vt, add, &[allocate, old_alloc]).unwrap();
    assert_eq!(hooks.count(rejected, 1), 0);
    let alias = key(b"originalIdentitySelection:with:ifEqual:ifDifferent:");
    call(word_vt, b"addMethod", &[alias, original]);
    let select_state = hooks.context(&[gc, alias, size(0).unwrap(), size(0).unwrap(), fault]);
    call(
        word_vt,
        b"addMethod",
        &[selector, method(b"select", select_state)],
    );
    assert_eq!(call(truth, b"not", &[]), falsity);
    assert_eq!(hooks.count(select_state, 2), 2);
    let answer = q.word(313).unwrap();
    hooks.put(state, 1, answer);
    assert_eq!(call(truth, b"ifTrue:ifFalse:", &[block, nil]), answer);
    hooks.put(select_state, 3, size(1).unwrap());
    assert_eq!(call(truth, b"not", &[]), nil);
    hooks.put(select_state, 3, size(2).unwrap());
    require_failure(
        "selection replacement failure",
        raw(truth, key(b"not"), &[]),
        Some(fault),
        &mut failures,
    );
    call(word_vt, b"addMethod", &[selector, original]);
    assert_eq!(call(truth, b"not", &[]), falsity);
    assert!(
        failures.is_empty(),
        "Boolean native boundary regressions: {failures:?}"
    );
}
