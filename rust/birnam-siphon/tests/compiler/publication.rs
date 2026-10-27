//! Ordinary Reflect edits stay independent of explicit Store publication hooks.
use super::*;
use std::{path::Path, ptr};
pub(super) fn check(h: &H<'_>) {
    h.reclaimed();
    let q = h.host.scope();
    let store = h.named(b"Store");
    let reflect = h.named(b"Reflect");
    let library = std::env::temp_dir().join(format!(
        "birnam-compiler-publication-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/compiler-publication.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &library, &[]).unwrap();
    std::fs::remove_file(library).unwrap();
    let target = h.call(h.named(b"Object"), b"clone", &[]);
    h.bind(b"PublicationTarget", target);
    let selector = h.key(b"publicationAnswer");
    let store_vt = h.named(b"StoreVt");
    let alias = h.key(b"savedCompilerAddMethod:on:with:");
    let add = h.key(b"addMethod:on:with:");
    let original = h.call(store_vt, b"lookup", &[add]);
    h.call(store_vt, b"addMethod", &[alias, original]);
    let truth = h.named(b"true");
    let nil = h.named(b"nil");
    let context = h.hooks.context(&[
        h.named(b"GC"),
        store,
        target,
        h.named(b"Integer"),
        alias,
        truth,
        size(0).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        size(41).unwrap(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    ]);
    let method = |n: &[u8]| {
        q.call(
            q.call(fixture, b"methodAt:", &[h.key(n)]).unwrap(),
            b"withContext:",
            &[context],
        )
        .unwrap()
    };
    let hook = |selector: &[u8], implementation: &[u8]| {
        let receiver = h.call(h.named(b"Object"), b"clone", &[]);
        let vt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[receiver]);
        h.call(vt, b"addMethod", &[h.key(selector), method(implementation)]);
        (receiver, vt)
    };
    let (authorizer, authorizer_vt) = hook(b"value", b"authorize");
    let (observer, observer_vt) = hook(b"managedMethodChanged:on:", b"observe");
    assert_eq!(
        h.call(store, b"setMethodMutationAuthorizer:", &[authorizer]),
        store
    );
    assert_eq!(
        h.call(store, b"setMethodMutationObserver:", &[observer]),
        store
    );
    h.call(store_vt, b"addMethod", &[add, method(b"publish")]);
    assert_eq!(
        h.eval(
            b"[Reflect defineMethod: #publicationAnswer on: PublicationTarget as: [:self | 41]]"
        ),
        target
    );
    for i in [6, 7, 8] {
        assert_eq!(h.hooks.count(context, i), 0);
    }
    let auth_fault = h.hooks.fault(931);
    let notice_fault = h.hooks.fault(932);
    h.hooks.put(context, 11, auth_fault);
    h.hooks.put(context, 5, h.named(b"false"));
    assert_eq!(
        h.eval(b"[Reflect defineMethod: #ordinaryOnly on: PublicationTarget as: [:self | 42]]"),
        target
    );
    assert_eq!(
        h.eval(b"[Reflect removeMethod: #ordinaryOnly on: PublicationTarget]"),
        target
    );
    for i in [6, 7, 8] {
        assert_eq!(h.hooks.count(context, i), 0);
    }
    h.hooks.put(context, 11, ptr::null_mut());
    h.hooks.put(context, 5, truth);
    let vt = h.vt(target);
    let old = h.call(target, b"methodFor:", &[selector]);
    let mirror = || h.call(store, b"rawCurrentValue:attr:", &[vt, selector]);
    let binding = || h.call(vt, b"lookup", &[selector]);
    assert!(mirror().is_null());
    assert_eq!(
        h.call(store, b"addMethod:on:with:", &[selector, vt, old]),
        store
    );
    for i in [6, 7, 8] {
        assert_eq!(h.hooks.count(context, i), 1);
    }
    let old_mirror = mirror();
    let old_binding = binding();
    let implementation = h.eval(b"[:self | 42]");
    let arity = h.call(h.named(b"Integer"), b"fromWord:", &[size(0).unwrap()]);
    let selected = h.call(
        reflect,
        b"methodForArity:implementation:",
        &[arity, implementation],
    );
    let args = [selector, vt, selected];
    h.collect();
    let before = h.units();
    let error = h.call(
        h.named(b"Error"),
        b"kind:message:",
        &[
            h.key(b"publicationRefused"),
            h.string(b"publication refused"),
        ],
    );
    for denied in [h.named(b"false"), nil, error] {
        h.hooks.put(context, 5, denied);
        assert_eq!(h.call(store, b"addMethod:on:with:", &args), denied);
        assert_eq!(binding(), old_binding);
        assert_eq!(mirror(), old_mirror);
        assert_eq!(h.hooks.count(context, 7), 1);
        h.collect();
        assert_eq!(h.units(), before, "refusal retains no additional code");
    }
    h.hooks.put(context, 11, auth_fault);
    assert_eq!(
        unsafe { h.host.id.send(store, add, &args) }.unwrap_err(),
        auth_fault
    );
    assert_eq!(binding(), old_binding);
    assert_eq!(h.hooks.count(context, 7), 1);
    h.collect();
    assert_eq!(h.units(), before);
    h.hooks.put(context, 11, ptr::null_mut());
    h.hooks.put(context, 5, truth);
    h.hooks.put(context, 9, size(42).unwrap());
    h.hooks.put(context, 12, notice_fault);
    assert_eq!(
        unsafe { h.host.id.send(store, add, &args) }.unwrap_err(),
        notice_fault
    );
    assert_eq!(h.hooks.count(context, 7), 2);
    assert_eq!(h.integer(h.call(target, b"publicationAnswer", &[])), 42);
    assert_ne!(mirror(), old_mirror);
    assert_eq!(h.integer(h.call(old, b"invokeOn:", &[target])), 41);
    h.call(
        authorizer_vt,
        b"addMethod",
        &[h.key(b"value"), h.hooks.method(b"fixed", truth)],
    );
    h.call(
        observer_vt,
        b"addMethod",
        &[
            h.key(b"managedMethodChanged:on:"),
            h.hooks.method(b"fixed", ptr::null_mut()),
        ],
    );
    assert_eq!(h.call(store, b"addMethod:on:with:", &args), store);
    assert_eq!(h.hooks.count(context, 8), 7);
    assert_eq!(h.hooks.count(context, 6), 6);
    assert_eq!(h.hooks.count(context, 7), 2);
    h.call(store_vt, b"addMethod", &[add, original]);
    h.call(store_vt, b"removeMethod:", &[alias]);
    h.call(store, b"removeMethod:on:", &[selector, vt]);
    assert!(mirror().is_null());
    h.hooks.put(context, 10, ptr::null_mut());
    h.bind(b"PublicationTarget", nil);
    drop(q);
    h.reclaimed();
}
