//! Proposal probe: explicit ordinary Error results and optional description ordering.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::{cell::RefCell, ffi::c_void, os::unix::ffi::OsStrExt};
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
    unsafe fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, v) });
        v
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
    unsafe fn integer(&self, n: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        }
    }
    unsafe fn number(&self, n: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(n)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn text(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe {
            let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
            assert_eq!(self.call(result, b"isError", &[]), self.named(b"false"));
            result
        }
    }
    unsafe fn clear(&self, mark: usize) {
        for t in self.tokens.borrow_mut().drain(mark..).rev() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
}

struct Events {
    id: Protocol,
    gc: Oop,
    value: Oop,
    failure: bool,
    events: *const RefCell<Vec<&'static str>>,
    label: &'static str,
}
unsafe extern "C" fn event(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Events>();
        (&*p.events).borrow_mut().push(p.label);
        collect(p.id, p.gc);
        if p.failure {
            Outcome::failed(881, 17)
        } else {
            Outcome::loaded(Operand::object(p.value))
        }
    }
}
#[test]
#[ignore = "production modules; experimental result protocol, not a runtime migration"]
fn ordinary_results_keep_failure_visible_and_description_optional() {
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
        h.call(h.named(b"MethodInvocation"), b"install", &[]);
        h.eval(include_bytes!(
            "../../../tests/fixtures/publication-result-proposal.birnam"
        ));

        let implementation = h.eval(b"[:self | 42]");
        let method = h.call(
            h.named(b"Reflect"),
            b"methodForArity:implementation:",
            &[h.integer(0), implementation],
        );
        let selector = h.keep(id.intern_name(b"proposalAnswer").unwrap());
        let explicit_error = h.call(
            h.named(b"Error"),
            b"kind:",
            &[h.keep(id.intern_name(b"proposalFailure").unwrap())],
        );
        let dir =
            std::env::temp_dir().join(format!("birnam-result-proposal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image = h.text(dir.join("proposal.image").as_os_str().as_bytes());
        assert_eq!(
            h.call(h.named(b"System"), b"saveImage:", &[image]),
            h.named(b"true")
        );
        let events = RefCell::new(vec![]);
        let mut hooks = Vec::new();
        let mut owners = Vec::new();
        for (label, selector) in [
            ("authorize", b"value".as_slice()),
            ("describe", b"value:value:"),
            ("notify", b"managedMethodChanged:on:"),
        ] {
            let owner = h.call(h.named(b"Object"), b"clone", &[]);
            let mut hook = Box::new(Events {
                id,
                gc: h.named(b"GC"),
                value: h.named(b"true"),
                failure: false,
                events: &events,
                label,
            });
            let vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[owner]);
            install(id, vt, selector, event, (&mut *hook as *mut Events).cast());
            hooks.push(hook);
            owners.push(owner);
        }
        let slots = h.call(
            h.call(h.named(b"Vector"), b"new", &[]),
            b"add:",
            &[owners[2]],
        );
        let caught = h.call(
            h.named(b"ObjectModel"),
            b"newSlots:parent:",
            &[slots, h.named(b"ObjectVt")],
        );
        let body=h.eval(b"[:self :selector :target | let result := [[ObjectModel slot: 0 in: self] managedMethodChanged: selector on: target]; [[result isError] ifTrue: [| nil] ifFalse: [| result]]]");
        h.call(
            h.named(b"Reflect"),
            b"defineMethod:on:as:",
            &[
                h.keep(id.intern_name(b"managedMethodChanged:on:").unwrap()),
                caught,
                body,
            ],
        );
        for mode in ["fresh", "mapped", "rebuilt"] {
            if mode != "fresh" {
                assert_eq!(
                    h.call(
                        h.named(b"System"),
                        b"loadImage:rebuild:",
                        &[
                            image,
                            h.named(if mode == "rebuilt" { b"true" } else { b"false" })
                        ]
                    ),
                    h.named(if mode == "rebuilt" { b"false" } else { b"true" })
                );
            }
            let policy = h.eval(b"PublicationResultProposal");
            for scenario in [
                "plain",
                "described",
                "denied",
                "auth-error",
                "auth-native",
                "describe-error",
                "notify-error",
                "notify-native",
                "notify-caught",
            ] {
                for hook in &mut hooks {
                    hook.value = h.named(b"true");
                    hook.failure = false;
                }
                match scenario {
                    "denied" => hooks[0].value = h.named(b"false"),
                    "auth-error" => hooks[0].value = explicit_error,
                    "auth-native" => hooks[0].failure = true,
                    "describe-error" => hooks[1].value = explicit_error,
                    "notify-error" => hooks[2].value = explicit_error,
                    "notify-native" | "notify-caught" => hooks[2].failure = true,
                    _ => {}
                }
                let target = h.call(h.named(b"Object"), b"clone", &[]);
                let vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[target]);
                let basis = h.call(h.named(b"db"), b"basis", &[]);
                events.borrow_mut().clear();
                let description = if scenario == "plain" {
                    h.named(b"nil")
                } else {
                    owners[1]
                };
                let result = h.call(
                    policy,
                    b"publish:on:with:authorize:describe:notify:",
                    &[
                        selector,
                        vt,
                        method,
                        owners[0],
                        description,
                        if scenario == "notify-caught" {
                            caught
                        } else {
                            owners[2]
                        },
                    ],
                );
                let committed = h.call(target, b"methodFor:", &[selector]) != h.named(b"nil");
                let expected_commit = !matches!(scenario, "denied" | "auth-error" | "auth-native");
                assert_eq!(committed, expected_commit);
                assert_eq!(
                    h.number(h.call(h.named(b"db"), b"basis", &[])),
                    h.number(basis)
                );
                let expected_events = if !expected_commit {
                    vec!["authorize"]
                } else if scenario == "plain" {
                    vec!["authorize", "notify"]
                } else if scenario == "describe-error" {
                    vec!["authorize", "describe"]
                } else {
                    vec!["authorize", "describe", "notify"]
                };
                assert_eq!(*events.borrow(), expected_events);
                if scenario.ends_with("error") {
                    assert_eq!(result, explicit_error);
                } else if scenario.ends_with("native") {
                    assert_eq!(h.call(result, b"isError", &[]), h.named(b"true"));
                } else if scenario == "denied" {
                    assert_eq!(result, h.named(b"false"));
                } else {
                    assert_eq!(result, policy);
                }
                if committed {
                    assert_eq!(h.number(h.call(target, b"proposalAnswer", &[])), 42);
                }
                eprintln!(
                    "result-proposal {mode} {scenario}: committed={committed} events={:?}",
                    events.borrow()
                );
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
        h.clear(0);
    }
}
