//! Explicit Store publication and authored native-failure boundaries.
//! Superseded adapter-comparison experiments are documented in publication-failure-boundary.md.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{Oop, as_size, size};
use support::H;

impl H<'_> {
    fn synchronous(&self, block: Oop) -> Oop {
        let code = self.call(block, b"codeObject", &[]);
        let count = self.number(self.call(block, b"captureCount", &[]));
        let captures = (0..count)
            .map(|i| self.call(block, b"captureAt:", &[self.integer(i)]))
            .collect::<Vec<_>>();
        self.call(
            self.named(b"Codegen"),
            b"materializeSynchronous:captures:",
            &[code, self.vector(&captures)],
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResultKind {
    Success,
    True,
    False,
    Error(Oop),
    Native(u32, usize),
}
fn classify(h: &H<'_>, result: Result<Oop, Oop>, success: Oop, store: Oop) -> ResultKind {
    match result {
        Err(e) => {
            h.keep(e);
            assert_eq!(h.call(e, b"faultKind", &[]), h.key(b"nativeFailure"));
            let details = h.call(e, b"detail", &[]);
            let status = as_size(h.call(details, b"at:", &[size(0).unwrap()])).unwrap();
            let detail = as_size(h.call(details, b"at:", &[size(1).unwrap()])).unwrap();
            ResultKind::Native(status as u32, detail)
        }
        Ok(v) => {
            h.keep(v);
            if v == success || v == store || v == h.named(b"nil") {
                ResultKind::Success
            } else if v == h.named(b"true") {
                ResultKind::True
            } else if v == h.named(b"false") {
                ResultKind::False
            } else {
                assert_eq!(h.call(v, b"isError", &[]), h.named(b"true"));
                let kind = h.call(v, b"kind", &[]);
                ResultKind::Error(kind)
            }
        }
    }
}
#[test]
#[ignore = "production Oop modules"]
fn authored_boundaries_do_not_silently_change_native_failure_or_callback_handling() {
    let host = support::bootstrap();
    let h = H::with_fixture(&host, "publication-boundary");
    let store = host
        .load_native(
            &h.q,
            &host.resources.lib_dir.join("native/libbirnam_store.dylib"),
            &[host.namespace],
        )
        .unwrap();
    h.q.check_result(store).unwrap();
    let target = h.call(h.named(b"Object"), b"clone", &[]);
    let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[target]);
    let selector = h.key(b"failureBoundaryMethod");
    let old_value = h.key(b"oldBoundary");
    let new_value = h.key(b"newBoundary");
    let old = h.method(b"answer", old_value);
    let new = h.method(b"answer", new_value);
    let failure = |detail| {
        let e = unsafe {
            host.id.send(
                h.named(b"Execution"),
                h.key(b"failNative:detail:"),
                &[size(881).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        h.keep(e)
    };
    let fault17 = failure(17);
    let fault99 = failure(99);
    let event_list = h.context(&[std::ptr::null_mut(); 32]);
    let state = h.context(&[
        vt,
        selector,
        new,
        h.call(h.named(b"Error"), b"kind:", &[h.key(b"nativeFailure")]),
        h.named(b"nil"),
        h.named(b"nil"),
        fault17,
        h.gc,
        h.named(b"nil"),
        event_list,
        size(0).unwrap(),
    ]);
    let events = || {
        (0..h.count(state, 10))
            .map(|i| {
                let e = as_size(h.call(event_list, b"at:", &[size(i).unwrap()])).unwrap();
                ["authorize", "notify", "fault", "caught", "cleanup"][e]
            })
            .collect::<Vec<_>>()
    };
    let probe = h.call(h.named(b"Object"), b"clone", &[]);
    let b = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for (name, implementation) in [
        (b"authorize".as_slice(), b"authorize".as_slice()),
        (b"value", b"authorize"),
        (b"notify", b"notify"),
        (b"managedMethodChanged:on:", b"notify"),
        (b"commit", b"commit"),
        (b"fault", b"fault"),
        (b"errorValue", b"errorValue"),
        (b"caught", b"caught"),
        (b"cleaned", b"cleaned"),
        (b"collect", b"collect"),
        (b"authorizer", b"authorizer"),
        (b"observer", b"observer"),
        (b"vtable", b"vtable"),
        (b"selector", b"selector"),
        (b"method", b"method"),
    ] {
        h.bind(b, name, h.method(implementation, state));
    }
    h.call(host.namespace, b"at:put:", &[h.key(b"FailureProbe"), probe]);
    let native = |entry: &[u8], context| {
        let value = h.call(h.named(b"Object"), b"clone", &[]);
        let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[value]);
        let method = h.method(entry, context);
        h.bind(vt, b"value", method);
        h.bind(vt, b"managedMethodChanged:on:", method);
        value
    };
    let ok = native(b"answer", h.named(b"true"));
    let denied = native(b"answer", h.named(b"false"));
    let failed = native(b"fault", state);
    let error = native(b"errorValue", state);
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { host.id.send(r, h.key(n), a) };
    h.eval(include_bytes!(
        "../../../tests/fixtures/publication-failure-boundary.birnam"
    ));
    let catcher = h.prototype(b"BoundaryCaught");
    let uncaught = h.prototype(b"BoundaryUncaught");
    let plain = h.prototype(b"BoundaryPlain");
    // The native-facing artifact path also selects synchronous named adapters.
    // A body signature alone does not change Reflect's source-evaluation adapter.
    let fixture = include_str!("../../../tests/fixtures/publication-failure-boundary.birnam");
    let source = fixture
        .split("// An explicit native-facing")
        .nth(1)
        .unwrap();
    let source = source[source.find("let owner :=").unwrap()..]
        .replace("BoundaryNativeOwner", "BoundaryArtifactOwner")
        .replace("BoundarySourceCaller", "BoundaryArtifactSourceCaller");
    let artifact = std::env::temp_dir().join(format!(
        "birnam-publication-boundary-{}.dylib",
        std::process::id()
    ));
    let path = h.text(artifact.as_os_str().as_encoded_bytes());
    let compiled = h.call(
        h.named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[h.text(source.as_bytes()), path, path, h.named(b"true")],
    );
    assert_ne!(compiled, h.named(b"nil"));
    assert_eq!(h.call(compiled, b"isError", &[]), h.named(b"false"));
    let loaded = h.call(h.named(b"Codegen"), b"load:", &[path]);
    assert_eq!(h.call(loaded, b"isError", &[]), h.named(b"false"));
    let artifact_owner = h.prototype(b"BoundaryArtifactOwner");
    let artifact_caller = h.prototype(b"BoundaryArtifactSourceCaller");
    let sync_plain = h.synchronous(plain);
    h.put(state, 4, ok);
    h.put(state, 5, ok);
    h.call(store, b"setMethodMutationAuthorizer:", &[probe]);
    h.call(store, b"setMethodMutationObserver:", &[probe]);
    // Native Store is the oracle. Probe commits through the real Id dictionary;
    // the experiments deliberately omit the optional description stage.
    for (scenario, auth, observer) in [
        ("ok", ok, ok),
        ("denied", denied, ok),
        ("native-observer-failure", ok, failed),
        ("observer-error-value", ok, error),
        ("observer-catches", ok, catcher),
        ("observer-returns-converted-error", ok, uncaught),
        ("native-authorizer-failure", failed, ok),
        ("authorizer-error-value", error, ok),
        ("authorizer-catches", catcher, ok),
    ] {
        h.put(state, 4, auth);
        h.put(state, 5, observer);
        let mut reference = None;
        for path in ["store", "sync-plain", "artifact-owner"] {
            h.call(vt, b"addMethod:as:", &[old, selector]);
            h.put(state, 10, size(0).unwrap());
            let result = match path {
                "store" => raw(store, b"addMethod:on:with:", &[selector, vt, new]),
                "sync-plain" => raw(sync_plain, b"value", &[]),
                _ => raw(artifact_owner, b"publish", &[]),
            };
            if let Err(error) = result {
                assert_eq!(error, fault17, "exact native failure identity");
            }
            let result = classify(&h, result, probe, store);
            let committed = h.call(target, b"failureBoundaryMethod", &[]) == new_value;
            let events = events();
            eprintln!(
                "boundary {scenario} {path}: {result:?}; committed={committed}; events={events:?}"
            );
            let actual = (result.clone(), committed, events.clone());
            if path == "store" {
                let expected = match scenario {
                    "denied" => (ResultKind::False, false, vec!["authorize"]),
                    "native-authorizer-failure" => (
                        ResultKind::Native(881, 17),
                        false,
                        vec!["authorize", "fault"],
                    ),
                    "authorizer-error-value" => (
                        ResultKind::Error(h.key(b"nativeFailure")),
                        false,
                        vec!["authorize"],
                    ),
                    "native-observer-failure" => (
                        ResultKind::Native(881, 17),
                        true,
                        vec!["authorize", "notify", "fault"],
                    ),
                    "observer-catches" => (
                        ResultKind::Success,
                        true,
                        vec!["authorize", "notify", "fault", "caught"],
                    ),
                    "observer-returns-converted-error" => (
                        ResultKind::Success,
                        true,
                        vec!["authorize", "notify", "fault"],
                    ),
                    "authorizer-catches" => (
                        ResultKind::Success,
                        true,
                        vec!["authorize", "fault", "caught", "notify"],
                    ),
                    _ => (ResultKind::Success, true, vec!["authorize", "notify"]),
                };
                assert_eq!(actual, expected, "Store oracle: {scenario}");
                reference = Some(actual.clone());
            } else {
                assert_eq!(
                    &actual,
                    reference.as_ref().unwrap(),
                    "explicit native publication: {scenario} {path}"
                );
            }
            assert_eq!(events.iter().filter(|&&s| s == "authorize").count(), 1);
            assert!(events.iter().filter(|&&s| s == "notify").count() <= 1);
            assert!(
                events.iter().filter(|&&s| s == "fault").count() <= 1,
                "callback was replayed"
            );
        }
    }
    h.put(state, 4, ok);
    h.put(state, 5, failed);
    // A replacement on the same callback receiver is observed by the next send.
    let failed_vt = h.call(host.id.root, b"behaviorOf:", &[failed]);
    h.bind(
        failed_vt,
        b"managedMethodChanged:on:",
        h.method(b"errorValue", state),
    );
    assert_eq!(
        classify(&h, raw(artifact_owner, b"publish", &[]), probe, store),
        ResultKind::Success
    );
    h.bind(
        failed_vt,
        b"managedMethodChanged:on:",
        h.method(b"fault", state),
    );
    h.put(state, 6, fault99);
    assert_eq!(
        classify(&h, raw(artifact_owner, b"publish", &[]), probe, store),
        ResultKind::Native(881, 99)
    );
    h.put(state, 6, fault17);
    // Exercise a normal resumable source caller of the synchronous artifact.
    h.put(state, 4, ok);
    h.put(state, 5, ok);
    let caller=h.eval(b"[| let answer := [BoundaryArtifactOwner publish]; [[answer isError] ifTrue: [| [FailureProbe caught]; [answer kind]] ifFalse: [| #success]]] ");
    h.put(state, 5, failed);
    h.put(state, 10, size(0).unwrap());
    assert_eq!(h.call(caller, b"value", &[]), h.key(b"nativeFailure"));
    assert_eq!(events(), vec!["authorize", "notify", "fault", "caught"]);
    // This caller was itself compiled synchronously, so it does not convert a
    // native failure into an Error; characterize rather than assume equivalence.
    assert_eq!(
        classify(&h, raw(artifact_caller, b"value", &[]), probe, store),
        ResultKind::Native(881, 17)
    );
    std::fs::remove_file(artifact).unwrap();
    // Earlier failures must not suppress handling in a later ordinary callback.
    assert_eq!(h.call(catcher, b"value", &[]), h.named(b"true"));
    h.put(state, 4, ok);
    h.put(state, 5, ok);
}
