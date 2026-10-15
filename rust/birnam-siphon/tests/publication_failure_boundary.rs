//! Compare real Store publication with authored failure-boundary experiments.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    os::unix::ffi::OsStrExt,
};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            ))
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn key(&self, n: &[u8]) -> Oop {
        unsafe { self.keep(self.id.intern_name(n).unwrap()) }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
}

impl H {
    unsafe fn text(&self, bytes: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        }
    }
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe {
            let text = self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(source)],
            )));
            let value = self.call(self.named(b"Codegen"), b"eval:", &[text]);
            assert_eq!(self.call(value, b"isError", &[]), self.named(b"false"));
            value
        }
    }
    unsafe fn synchronous(&self, block: Oop) -> Oop {
        unsafe {
            let code = self.call(block, b"codeObject", &[]);
            let size = self.call(block, b"captureCount", &[]);
            let count = send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(size)],
            )
            .as_word()
            .unwrap();
            let mut captures = self.call(self.named(b"Vector"), b"new", &[]);
            for index in 0..count {
                let index = self.keep(object(send(
                    self.id,
                    self.named(b"Integer"),
                    b"fromWord:",
                    &[Operand::word(index)],
                )));
                let value = self.call(block, b"captureAt:", &[index]);
                captures = self.call(captures, b"add:", &[value]);
            }
            self.call(
                self.named(b"Codegen"),
                b"materializeSynchronous:captures:",
                &[code, captures],
            )
        }
    }
    unsafe fn global(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Globals"), b"at:", &[self.key(name)]) }
    }
}
struct Probe<'a> {
    h: &'a H,
    vt: Oop,
    selector: Oop,
    method: Oop,
    error: Oop,
    authorizer: Cell<Oop>,
    observer: Cell<Oop>,
    detail: Cell<usize>,
    events: RefCell<Vec<&'static str>>,
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
unsafe extern "C" fn fault(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("fault");
        collect(p.h.id, p.h.named(b"GC"));
        Outcome::failed(881, p.detail.get())
    }
}
unsafe extern "C" fn error_value(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe { Outcome::loaded(Operand::object((&*c.cast::<Probe<'_>>()).error)) }
}
unsafe extern "C" fn caught(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("caught");
        Outcome::loaded(Operand::object(p.h.named(b"nil")))
    }
}
unsafe extern "C" fn cleaned(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("cleanup");
        collect(p.h.id, p.h.named(b"GC"));
        Outcome::loaded(Operand::object(p.h.named(b"nil")))
    }
}
unsafe extern "C" fn collecting(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        collect(p.h.id, p.h.named(b"GC"));
        Outcome::loaded(Operand::object(p.h.named(b"nil")))
    }
}
unsafe extern "C" fn authorizer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("authorize");
        Outcome::loaded(Operand::object(p.authorizer.get()))
    }
}
unsafe extern "C" fn observer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("notify");
        Outcome::loaded(Operand::object(p.observer.get()))
    }
}
unsafe extern "C" fn vtable(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe { Outcome::loaded(Operand::object((&*c.cast::<Probe<'_>>()).vt)) }
}
unsafe extern "C" fn selector_value(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    unsafe { Outcome::loaded(Operand::object((&*c.cast::<Probe<'_>>()).selector)) }
}
unsafe extern "C" fn method(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe { Outcome::loaded(Operand::object((&*c.cast::<Probe<'_>>()).method)) }
}
unsafe extern "C" fn authorize(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("authorize");
        p.h.id
            .send_named(p.authorizer.get(), b"value", &[])
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn notify(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        p.events.borrow_mut().push("notify");
        p.h.id
            .send_named(
                p.observer.get(),
                b"managedMethodChanged:on:",
                &[Operand::object(p.selector), Operand::object(p.vt)],
            )
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn commit(c: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        match p.h.id.send_named(
            p.vt,
            b"addMethod:as:",
            &[Operand::object(p.method), Operand::object(p.selector)],
        ) {
            Err(e) => e,
            Ok(_) => Outcome::loaded(Operand::object(r)),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResultKind {
    Success,
    True,
    False,
    Error(String),
    Native(u32, usize),
}
unsafe fn classify(
    h: &H,
    result: Result<Operand, Outcome>,
    success: Oop,
    store: Oop,
) -> ResultKind {
    unsafe {
        match result {
            Err(e) => ResultKind::Native(e.status, e.detail),
            Ok(v) => {
                let v = h.keep(object(v));
                if v == success || v == store || v == h.named(b"nil") {
                    ResultKind::Success
                } else if v == h.named(b"true") {
                    ResultKind::True
                } else if v == h.named(b"false") {
                    ResultKind::False
                } else {
                    assert_eq!(h.call(v, b"isError", &[]), h.named(b"true"));
                    let kind = h.call(v, b"kind", &[]);
                    let bytes = send(
                        h.id,
                        h.id.receiver,
                        b"symbolBytesOf:",
                        &[Operand::object(kind)],
                    )
                    .as_bytes()
                    .unwrap();
                    ResultKind::Error(
                        String::from_utf8(
                            birnam_abi::borrowed_bytes(bytes.pointer, bytes.length)
                                .unwrap()
                                .to_vec(),
                        )
                        .unwrap(),
                    )
                }
            }
        }
    }
}
#[test]
#[ignore = "production modules"]
fn authored_boundaries_do_not_silently_change_native_failure_or_callback_handling() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let h = H {
            id,
            ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        let store = h.named(b"Store");
        let object_proto = h.named(b"Object");
        let target = h.call(object_proto, b"clone", &[]);
        let vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[target]);
        let selector = h.key(b"failureBoundaryMethod");
        let old_value = h.key(b"oldBoundary");
        let new_value = h.key(b"newBoundary");
        let make = |v| {
            h.keep(object(send(
                id,
                h.named(b"Method"),
                b"fromNative:owner:",
                &[
                    Operand::entry(NativeEntry::new(answer, v)),
                    Operand::object(v),
                ],
            )))
        };
        let old = make(old_value);
        let new = make(new_value);
        let mut p = Probe {
            h: &h,
            vt,
            selector,
            method: new,
            error: h.call(h.named(b"Error"), b"kind:", &[h.key(b"nativeFailure")]),
            authorizer: Cell::new(h.named(b"nil")),
            observer: Cell::new(h.named(b"nil")),
            detail: Cell::new(17),
            events: RefCell::new(vec![]),
        };
        let ctx = (&raw mut p).cast();
        let b = h.keep(behavior(id));
        for (name, f) in [
            (
                b"authorize".as_slice(),
                authorize as birnam_abi::id_protocol::Method,
            ),
            (b"value", authorize),
            (b"notify", notify),
            (b"managedMethodChanged:on:", notify),
            (b"commit", commit),
            (b"fault", fault),
            (b"errorValue", error_value),
            (b"caught", caught),
            (b"cleaned", cleaned),
            (b"collect", collecting),
            (b"authorizer", authorizer),
            (b"observer", observer),
            (b"vtable", vtable),
            (b"selector", selector_value),
            (b"method", method),
        ] {
            install(id, b, name, f, ctx);
        }
        let probe = h.keep(allocate(id, b, 0));
        h.call(ns, b"at:put:", &[h.key(b"FailureProbe"), probe]);
        let native = |f: birnam_abi::id_protocol::Method, c| {
            let b = h.keep(behavior(id));
            install(id, b, b"value", f, c);
            install(id, b, b"managedMethodChanged:on:", f, c);
            h.keep(allocate(id, b, 0))
        };
        let ok = native(answer, h.named(b"true"));
        let denied = native(answer, h.named(b"false"));
        let failed = native(fault, ctx);
        let error = native(error_value, ctx);
        h.eval(include_bytes!(
            "../../../tests/fixtures/publication-failure-boundary.birnam"
        ));
        let catcher = h.global(b"BoundaryCaught");
        let uncaught = h.global(b"BoundaryUncaught");
        let plain = h.global(b"BoundaryPlain");
        let local = h.global(b"BoundaryLocal");
        let request_body = h.global(b"BoundaryRequestBody");
        let tail = h.global(b"BoundaryTail");
        let native_owner = h.global(b"BoundaryNativeOwner");
        let source_caller = h.global(b"BoundarySourceCaller");
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
        let path = h.text(artifact.as_os_str().as_bytes());
        let compiled = h.call(
            h.named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[h.text(source.as_bytes()), path, path, h.named(b"true")],
        );
        assert_ne!(compiled, h.named(b"nil"));
        assert_eq!(h.call(compiled, b"isError", &[]), h.named(b"false"));
        let loaded = h.call(h.named(b"Codegen"), b"load:", &[path]);
        assert_eq!(h.call(loaded, b"isError", &[]), h.named(b"false"));
        let artifact_owner = h.global(b"BoundaryArtifactOwner");
        let artifact_caller = h.global(b"BoundaryArtifactSourceCaller");
        let sync_plain = h.synchronous(plain);
        let sync_local = h.synchronous(local);
        p.authorizer.set(ok);
        p.observer.set(ok);
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
            p.authorizer.set(auth);
            p.observer.set(observer);
            let mut reference = None;
            for path in [
                "store",
                "plain",
                "whole-preserve",
                "local-preserve",
                "sync-plain",
                "sync-whole-preserve",
                "sync-local-preserve",
                "native-owner",
                "artifact-owner",
            ] {
                h.call(vt, b"addMethod:as:", &[old, selector]);
                p.events.borrow_mut().clear();
                let result = match path {
                    "store" => id.send_named(
                        store,
                        b"addMethod:on:with:",
                        &[
                            Operand::object(selector),
                            Operand::object(vt),
                            Operand::object(new),
                        ],
                    ),
                    "plain" => id.send_named(plain, b"value", &[]),
                    "whole-preserve" => id.send_named(
                        h.named(b"Execution"),
                        b"evaluatePreservingFailures:",
                        &[Operand::object(plain)],
                    ),
                    "local-preserve" => id.send_named(local, b"value", &[]),
                    "sync-plain" => id.send_named(sync_plain, b"value", &[]),
                    "sync-whole-preserve" => id.send_named(
                        h.named(b"Execution"),
                        b"evaluatePreservingFailures:",
                        &[Operand::object(sync_plain)],
                    ),
                    "sync-local-preserve" => id.send_named(sync_local, b"value", &[]),
                    "native-owner" => id.send_named(native_owner, b"publish", &[]),
                    _ => id.send_named(artifact_owner, b"publish", &[]),
                };
                let result = classify(&h, result, probe, store);
                let committed = h.call(target, b"failureBoundaryMethod", &[]) == new_value;
                let events = p.events.borrow().clone();
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
                            ResultKind::Error("nativeFailure".into()),
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
                } else if path == "artifact-owner" || path == "sync-plain" {
                    assert_eq!(
                        &actual,
                        reference.as_ref().unwrap(),
                        "faithful boundary: {scenario} {path}"
                    );
                } else if matches!(path, "plain" | "native-owner") {
                    if scenario.starts_with("native-") {
                        assert_ne!(
                            &actual,
                            reference.as_ref().unwrap(),
                            "negative control should lose native channel"
                        );
                        assert_eq!(committed, reference.as_ref().unwrap().1);
                        assert!(!matches!(result, ResultKind::Native(..)));
                    } else {
                        assert_eq!(&actual, reference.as_ref().unwrap());
                    }
                } else if matches!(
                    scenario,
                    "observer-catches" | "authorizer-catches" | "observer-returns-converted-error"
                ) {
                    assert!(
                        matches!(result, ResultKind::Native(..)),
                        "preserving scope must expose the inner native failure"
                    );
                    assert!(
                        !events.contains(&"caught"),
                        "ambient preservation bypasses callback-local handling"
                    );
                    assert_eq!(committed, scenario != "authorizer-catches");
                } else if scenario.starts_with("native-") {
                    assert!(matches!(result, ResultKind::Native(..)));
                    assert_eq!(committed, reference.as_ref().unwrap().1);
                } else {
                    assert_eq!(&actual, reference.as_ref().unwrap());
                }
                assert_eq!(events.iter().filter(|&&s| s == "authorize").count(), 1);
                assert!(events.iter().filter(|&&s| s == "notify").count() <= 1);
                assert!(
                    events.iter().filter(|&&s| s == "fault").count() <= 1,
                    "callback was replayed"
                );
            }
        }
        p.authorizer.set(ok);
        for (scenario, observer) in [
            ("native-failure", failed),
            ("error-value", error),
            ("catches", catcher),
            ("converted-error", uncaught),
        ] {
            p.observer.set(observer);
            for path in ["request-in-body", "tail-request"] {
                h.call(vt, b"addMethod:as:", &[old, selector]);
                p.events.borrow_mut().clear();
                let result = if path == "tail-request" {
                    id.send_named(tail, b"publish", &[])
                } else {
                    id.send_named(request_body, b"value", &[])
                };
                let result = classify(&h, result, probe, store);
                let events = p.events.borrow().clone();
                assert_eq!(h.call(target, b"failureBoundaryMethod", &[]), new_value);
                eprintln!("boundary {scenario} {path}: {result:?}; events={events:?}");
                assert_eq!(events.iter().filter(|&&s| s == "notify").count(), 1);
                assert!(!events.contains(&"caught"));
                if scenario == "error-value" {
                    assert_eq!(result, ResultKind::Success);
                } else if path == "tail-request" {
                    assert_eq!(result, ResultKind::Native(881, 17));
                } else if scenario == "native-failure" {
                    assert_eq!(result, ResultKind::Success);
                } else {
                    assert!(matches!(result, ResultKind::Native(..)));
                }
                if path == "tail-request" {
                    assert_eq!(events.iter().filter(|&&s| s == "cleanup").count(), 1);
                }
            }
        }
        for (scenario, auth) in [
            ("native-failure", failed),
            ("error-value", error),
            ("catches", catcher),
        ] {
            p.authorizer.set(auth);
            p.events.borrow_mut().clear();
            let result = classify(&h, id.send_named(tail, b"authorize", &[]), probe, store);
            eprintln!(
                "boundary {scenario} tail-value: {result:?}; events={:?}",
                p.events.borrow()
            );
            assert_eq!(
                result,
                match scenario {
                    "native-failure" => ResultKind::Native(881, 17),
                    "error-value" => ResultKind::Error("nativeFailure".into()),
                    _ => ResultKind::True,
                }
            );
            assert_eq!(p.events.borrow().contains(&"caught"), scenario == "catches");
        }
        // The ordinary source caller still receives an Error value from the native
        // boundary; no failure-preservation flag is forced into its callback body.
        p.authorizer.set(ok);
        p.observer.set(failed);
        p.events.borrow_mut().clear();
        assert_eq!(
            h.call(source_caller, b"value", &[]),
            h.key(b"nativeFailure")
        );
        assert_eq!(
            *p.events.borrow(),
            vec!["authorize", "notify", "fault", "caught"]
        );
        // A replacement on the same callback receiver is observed by the next send.
        let failed_vt = h.call(id.receiver, b"behaviorOf:", &[failed]);
        install(id, failed_vt, b"managedMethodChanged:on:", error_value, ctx);
        assert_eq!(
            classify(
                &h,
                id.send_named(native_owner, b"publish", &[]),
                probe,
                store
            ),
            ResultKind::Success
        );
        assert_eq!(
            classify(
                &h,
                id.send_named(artifact_owner, b"publish", &[]),
                probe,
                store
            ),
            ResultKind::Success
        );
        install(id, failed_vt, b"managedMethodChanged:on:", fault, ctx);
        p.detail.set(99);
        assert_eq!(
            classify(
                &h,
                id.send_named(native_owner, b"publish", &[]),
                probe,
                store
            ),
            ResultKind::Error("nativeFailure".into())
        );
        assert_eq!(
            classify(
                &h,
                id.send_named(artifact_owner, b"publish", &[]),
                probe,
                store
            ),
            ResultKind::Native(881, 99)
        );
        p.detail.set(17);
        // Exercise a normal resumable source caller of the synchronous artifact.
        p.authorizer.set(ok);
        p.observer.set(ok);
        let caller=h.eval(b"[| let answer := [BoundaryArtifactOwner publish]; [[answer isError] ifTrue: [| [FailureProbe caught]; [answer kind]] ifFalse: [| #success]]] ");
        p.observer.set(failed);
        p.events.borrow_mut().clear();
        assert_eq!(h.call(caller, b"value", &[]), h.key(b"nativeFailure"));
        assert_eq!(
            *p.events.borrow(),
            vec!["authorize", "notify", "fault", "caught"]
        );
        // This caller was itself compiled synchronously, so it does not convert a
        // native failure into an Error; characterize rather than assume equivalence.
        assert_eq!(
            classify(
                &h,
                id.send_named(artifact_caller, b"value", &[]),
                probe,
                store
            ),
            ResultKind::Native(881, 17)
        );
        std::fs::remove_file(artifact).unwrap();
        // A failing preserving scope must restore the ambient conversion policy.
        assert_eq!(h.call(catcher, b"value", &[]), h.named(b"true"));
        p.authorizer.set(ok);
        p.observer.set(ok);
    }
}
