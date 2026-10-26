//! Explicit Store publication preserves authorization and post-edit notification.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;

#[test]
#[ignore = "production native modules"]
fn authorization_commit_notifications_and_failure_are_distinct() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let store = h
        .load_native(
            &q,
            &h.resources.lib_dir.join("native/libbirnam_store.dylib"),
            &[h.namespace],
        )
        .unwrap();
    q.check_result(store).unwrap();
    let path = std::env::temp_dir().join(format!(
        "birnam-store-publication-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/store-publication.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let truth = h.named(b"true").unwrap();
    let falsity = h.named(b"false").unwrap();
    let nil = h.named(b"nil").unwrap();
    let gc = h.named(b"GC").unwrap();
    q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let target = q.call(h.named(b"Object").unwrap(), b"clone", &[]).unwrap();
    let vt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[target])
        .unwrap();
    let selector = hooks.symbol(b"publicationAudit");
    let old_value = hooks.symbol(b"oldPublication");
    let new_value = hooks.symbol(b"newPublication");
    let old = hooks.method(b"fixed", old_value);
    let new = hooks.method(b"fixed", new_value);
    let foreign = hooks.symbol(b"publicationForeign");
    let error = q
        .call(
            h.named(b"Error").unwrap(),
            b"kind:",
            &[hooks.symbol(b"observerErrorValue")],
        )
        .unwrap();
    let auth_fault = hooks.fault(1);
    let managed_fault = hooks.fault(3);
    let state = hooks.context(&[
        store,
        h.named(b"Execution").unwrap(),
        gc,
        nil,
        truth,
        falsity,
        foreign,
        error,
        auth_fault,
        managed_fault,
        size(0).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        nil,
        nil,
    ]);
    let events = || {
        (0..hooks.count(state, 12))
            .map(|i| hooks.field(state, 13 + i))
            .collect::<Vec<_>>()
    };
    let authorize = hooks.symbol(b"authorize");
    let managed = hooks.symbol(b"managed");
    for (name, public, registration) in [
        (
            authorize,
            b"value".as_slice(),
            b"setMethodMutationAuthorizer:".as_slice(),
        ),
        (
            managed,
            b"managedMethodChanged:on:",
            b"setMethodMutationObserver:",
        ),
    ] {
        let method = q.call(module, b"methodAt:", &[name]).unwrap();
        let method = q.call(method, b"withContext:", &[state]).unwrap();
        let receiver = q.call(h.named(b"Object").unwrap(), b"clone", &[]).unwrap();
        let behavior = q
            .call(h.id.root, b"ensurePrivateVtableFor:", &[receiver])
            .unwrap();
        q.call(behavior, b"addMethod", &[hooks.symbol(public), method])
            .unwrap();
        assert_eq!(q.call(store, registration, &[receiver]).unwrap(), store);
    }
    let publish = hooks.symbol(b"addMethod:on:with:");
    for mode in 0..=4 {
        hooks.put(state, 11, size(0).unwrap());
        assert_eq!(
            q.call(store, b"addMethod:on:with:", &[selector, vt, old])
                .unwrap(),
            store
        );
        hooks.put(state, 12, size(0).unwrap());
        hooks.put(state, 10, size(mode).unwrap());
        hooks.put(state, 11, size(1).unwrap());
        let result = unsafe { h.id.send(store, publish, &[selector, vt, new]) };
        let committed = mode != 1 && mode != 2;
        assert_eq!(
            q.call(target, b"publicationAudit", &[]).unwrap(),
            if committed { new_value } else { old_value }
        );
        assert_eq!(
            events(),
            if committed {
                vec![authorize, managed]
            } else {
                vec![authorize]
            }
        );
        match mode {
            1 => assert_eq!(result.unwrap(), falsity),
            2 => assert_eq!(result.unwrap_err(), auth_fault),
            3 => {
                assert_eq!(result.unwrap_err(), managed_fault);
                assert_eq!(
                    q.call(store, b"currentValue:attr:", &[foreign, selector])
                        .unwrap(),
                    truth
                );
            }
            _ => assert_eq!(result.unwrap(), store),
        }
        assert_eq!(
            q.call(store, b"rawCurrentValue:attr:", &[vt, selector])
                .unwrap(),
            if committed { new } else { old }
        );
    }
    hooks.put(state, 10, size(3).unwrap());
    hooks.put(state, 12, size(0).unwrap());
    let remove = hooks.symbol(b"removeMethod:on:");
    assert_eq!(
        unsafe { h.id.send(store, remove, &[selector, vt]) }.unwrap_err(),
        managed_fault
    );
    assert_eq!(events(), vec![authorize, managed]);
    assert_eq!(hooks.capture(vt, selector), nil);
    hooks.put(state, 10, size(0).unwrap());
    hooks.put(state, 12, size(0).unwrap());
    let missing = q.call(store, b"removeMethod:on:", &[selector, vt]).unwrap();
    assert_eq!(
        q.call(missing, b"kind", &[]).unwrap(),
        hooks.symbol(b"missingDirectMethod")
    );
    assert_eq!(events(), vec![authorize]);
}
