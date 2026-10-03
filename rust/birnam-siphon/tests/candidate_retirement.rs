//! Discard admission, weak mapping accounting and retained callable lifetime.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Vm {
    id: Protocol,
    ns: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl Vm {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.sym(n))],
            ))
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        unsafe {
            self.tokens
                .borrow_mut()
                .push(retain(self.id, self.named(b"Roots"), o));
            o
        }
    }
    unsafe fn sym(&self, n: &[u8]) -> Oop {
        unsafe { self.id.intern_name(n).unwrap() }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )))
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
    unsafe fn number(&self, o: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn at(&self, o: Oop, i: usize) -> Oop {
        unsafe { self.call(o, b"at:", &[self.integer(i)]) }
    }
    unsafe fn vector(&self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for &x in values {
                v = self.call(v, b"add:", &[x]);
            }
            v
        }
    }
    unsafe fn fact(&self, e: Oop, a: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"db"), b"currentValue:attr:", &[e, self.sym(a)]) }
    }
    unsafe fn set(&self, e: Oop, a: &[u8], v: Oop) {
        unsafe {
            assert_eq!(
                self.call(
                    self.named(b"db"),
                    b"assert:attr:value:",
                    &[e, self.sym(a), v]
                ),
                self.named(b"db")
            );
        }
    }
    unsafe fn carrier(&self, p: Oop, predecessor: Oop, unit: Oop) -> Oop {
        unsafe {
            let g = self.call(self.named(b"Object"), b"clone", &[]);
            for (attr, v) in [
                (b"sourceMap".as_slice(), self.at(p, 1)),
                (b"baseDigest", self.at(p, 2)),
                (b"generationSelection", self.at(p, 3)),
                (b"predecessor", predecessor),
                (b"deltaUnit", unit),
            ] {
                self.set(g, attr, v);
            }
            g
        }
    }
    unsafe fn plan(&self, action: &[u8], unit: Oop, expected: Oop, next: Oop) -> Oop {
        unsafe {
            let event = self.call(self.named(b"Object"), b"clone", &[]);
            let basis = self.call(self.named(b"db"), b"basis", &[]);
            self.vector(&[
                self.sym(action),
                unit,
                self.sym(b"txFixture"),
                self.named(b"nil"),
                event,
                basis,
                expected,
                next,
            ])
        }
    }
    unsafe fn direct(&self, selector: &[u8]) -> Operand {
        unsafe {
            send(
                self.id,
                self.id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(self.sym(selector)),
                    Operand::object(self.named(b"IntegerVt")),
                ],
            )
        }
    }
}

impl Vm {
    unsafe fn clear(&self) {
        unsafe {
            for token in self.tokens.borrow_mut().drain(..) {
                release(self.id, self.named(b"Roots"), token);
            }
        }
    }
}
struct ForeignEdit {
    id: Protocol,
    original: NativeEntry,
    armed: Cell<bool>,
    denied: Cell<usize>,
}
unsafe extern "C" fn edit_after_publication(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<ForeignEdit>();
        let answer = p.original.method.unwrap()(p.original.context, r, a, n);
        if p.armed.replace(false) {
            let a = borrowed_slice(a, n).unwrap();
            for (name, args) in [
                (b"removeMethod:on:".as_slice(), &a[..2]),
                (b"addMethod:on:with:", a),
            ] {
                if send(p.id, r, name, args).as_object() != Some(r) {
                    p.denied.set(p.denied.get() + 1);
                }
            }
        }
        answer
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn discard_preserves_replaced_files_and_live_methods_then_reclaims_mappings() {
    if std::env::var_os("BIRNAM_RETIREMENT_PHASE").is_none() {
        for phase in ["bootstrap", "running"] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "discard_preserves_replaced_files_and_live_methods_then_reclaims_mappings",
                        "--nocapture"
                    ])
                    .env("BIRNAM_RETIREMENT_PHASE", phase)
                    .status()
                    .unwrap()
                    .success(),
                "{phase}"
            );
        }
        return;
    }
    let running = std::env::var("BIRNAM_RETIREMENT_PHASE").unwrap() == "running";
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let v = Vm {
            id,
            ns,
            tokens: RefCell::new(Vec::new()),
        };
        let cg = v.named(b"Codegen");
        let nil = v.named(b"nil");
        let body = b"[Reflect defineMethod: #retireValue on: Integer as: [:self | 10]]";
        v.call(cg, b"evalPersisting:", &[v.text(body)]);
        let mut source = b"[Module named: #txFixture requires: #[]]; ".to_vec();
        source.extend_from_slice(body);
        v.call(
            cg,
            b"recordModuleSource:named:",
            &[v.text(&source), v.sym(b"txFixture")],
        );
        let projection = v.call(cg, b"initialGenerationProjection", &[]);
        let generation = v.carrier(projection, nil, nil);
        v.set(v.sym(b"dispatch"), b"activeGeneration", generation);
        assert_eq!(
            v.call(
                v.named(b"Generation"),
                b"adoptInitialGeneration:",
                &[generation]
            ),
            generation
        );
        v.call(cg, b"sealInitialGeneration", &[]);
        if running {
            let entered = v.call(
                cg,
                b"evalPersisting:",
                &[v.text(b"[Generation enterRunning]")],
            );
            assert_eq!(
                v.call(entered, b"kind", &[]),
                v.sym(b"generationNotQuiescent")
            );
            let fresh = v.call(v.named(b"Object"), b"clone", &[]);
            let before = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(fresh)],
            ));
            let block = v.call(cg, b"evalPersisting:", &[v.text(b"[:self | 99]")]);
            let method = v.keep(object(send(
                id,
                v.named(b"Execution"),
                b"methodOfEntry:",
                &[v.direct(b"retireValue")],
            )));
            assert_eq!(
                v.call(v.named(b"Generation"), b"enterRunning", &[]),
                v.named(b"true")
            );
            assert_eq!(
                v.call(v.named(b"Generation"), b"enterRunning", &[]),
                v.named(b"true")
            );
            let rejected = v.call(
                v.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[v.sym(b"newMethod"), fresh, block],
            );
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"generationNotQuiescent")
            );
            assert_eq!(
                send(id, id.receiver, b"behaviorOf:", &[Operand::object(fresh)]).as_object(),
                Some(before)
            );
            for source in [
                b"[Reflect defineMethod: #retireValue on: Integer as: [:self | 99]]".as_slice(),
                b"[Reflect removeMethod: #retireValue on: Integer]",
            ] {
                let rejected = v.call(cg, b"evalPersisting:", &[v.text(source)]);
                assert_eq!(
                    v.call(rejected, b"kind", &[]),
                    v.sym(b"generationNotQuiescent")
                );
            }
            assert_eq!(v.call(cg, b"lastError", &[]), nil);
            let rejected = v.call(
                v.named(b"Store"),
                b"addMethod:on:with:",
                &[v.sym(b"retireValue"), v.named(b"IntegerVt"), method],
            );
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"generationNotQuiescent")
            );
            let rejected = v.call(
                v.named(b"Store"),
                b"removeMethod:on:",
                &[v.sym(b"retireValue"), v.named(b"IntegerVt")],
            );
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"generationNotQuiescent")
            );
            assert_eq!(v.number(v.call(v.integer(1), b"retireValue", &[])), 10);
            assert_eq!(
                v.call(
                    v.named(b"Generation"),
                    b"reconcileCandidate:",
                    &[generation]
                ),
                generation
            );
        }
        let dir = std::env::temp_dir().join(format!("birnam-retirement-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Keep the projection across explicit root releases between iterations.
        let projection_root = retain(id, v.named(b"Roots"), projection);
        for (index, mode) in ["unmapped", "inert", "resumable", "synchronous", "callback"]
            .iter()
            .enumerate()
        {
            let source = v.text(b"[Module named: #txFixture requires: #[]]; [Reflect defineMethod: #retireValue on: Integer as: [:self | 20]]");
            let path = dir.join(format!("{index}.o"));
            let sidecar = dir.join(format!("{index}.o.candidate"));
            let path_object = v.text(path.as_os_str().as_bytes());
            let compiled = if *mode == "synchronous" {
                v.call(
                    v.named(b"Generation"),
                    b"compileCandidate:named:output:selection:",
                    &[source, v.sym(b"txFixture"), path_object, v.vector(&[])],
                )
            } else {
                v.call(
                    v.named(b"Generation"),
                    b"compileCandidate:named:output:",
                    &[source, v.sym(b"txFixture"), path_object],
                )
            };
            assert_eq!(compiled, path_object);
            let unit = v.call(v.named(b"Object"), b"clone", &[]);
            assert_eq!(
                v.call(
                    v.named(b"Generation"),
                    b"inspectCandidate:into:",
                    &[path_object, unit]
                ),
                unit
            );
            if *mode != "unmapped" {
                assert_eq!(
                    v.call(v.named(b"Generation"), b"mapCandidate:", &[unit]),
                    unit
                );
            }
            let artifact = std::fs::read(&path).unwrap();
            std::fs::write(&path, b"new author contents").unwrap();
            let rejected = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
            assert_eq!(v.call(rejected, b"isError", &[]), v.named(b"true"));
            assert_eq!(std::fs::read(&path).unwrap(), b"new author contents");
            assert!(sidecar.exists());
            assert_ne!(v.fact(unit, b"state"), v.sym(b"discarded"));
            std::fs::write(&path, &artifact).unwrap();
            let mut held = None;
            if index >= 2 {
                let projected = v.call(
                    v.named(b"Generation"),
                    b"successorGenerationProjection:replacing:with:unit:",
                    &[
                        v.at(projection, 1),
                        v.sym(b"txFixture"),
                        v.fact(unit, b"sourceRevision"),
                        unit,
                    ],
                );
                let next = v.carrier(projected, generation, unit);
                let token = v.call(
                    v.named(b"Generation"),
                    b"prepareActivation:plan:",
                    &[unit, v.plan(b"activate", unit, generation, next)],
                );
                let rejected = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
                assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"candidateInUse"));
                assert!(path.exists() && sidecar.exists());
                if *mode == "callback" {
                    let store_vt = v.named(b"StoreVt");
                    let key = v.sym(b"addMethod:on:with:");
                    let original = send(
                        id,
                        id.receiver,
                        b"bindingOf:in:",
                        &[Operand::object(key), Operand::object(store_vt)],
                    )
                    .as_entry()
                    .unwrap();
                    let mut probe = ForeignEdit {
                        id,
                        original,
                        armed: Cell::new(true),
                        denied: Cell::new(0),
                    };
                    install(
                        id,
                        store_vt,
                        b"addMethod:on:with:",
                        edit_after_publication,
                        core::ptr::from_mut(&mut probe).cast(),
                    );
                    // Installing the fixture changes Id's epoch. Prepare the
                    // actual token only after that dictionary instrumentation.
                    assert_eq!(
                        v.call(v.named(b"Generation"), b"discardPrepared:", &[token]),
                        unit
                    );
                    let token = v.call(
                        v.named(b"Generation"),
                        b"prepareActivation:plan:",
                        &[unit, v.plan(b"activate", unit, generation, next)],
                    );
                    let rejected = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
                    send(
                        id,
                        store_vt,
                        b"addMethod",
                        &[Operand::object(key), Operand::entry(original)],
                    );
                    assert!(!probe.armed.get());
                    if running {
                        assert_eq!(probe.denied.get(), 2);
                        assert_eq!(rejected, unit);
                        assert_eq!(v.number(v.call(v.integer(1), b"retireValue", &[])), 20);
                        let rollback = v.call(
                            v.named(b"Generation"),
                            b"prepareRollback:plan:",
                            &[unit, v.plan(b"rollback", unit, next, generation)],
                        );
                        assert_eq!(
                            v.call(
                                v.named(b"Generation"),
                                b"commitPreparedRollback:",
                                &[rollback]
                            ),
                            unit
                        );
                        assert_eq!(v.number(v.call(v.integer(1), b"retireValue", &[])), 10);
                        v.clear();
                        let receipt = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
                        assert_eq!(v.call(receipt, b"physical", &[]), v.sym(b"disposed"));
                        continue;
                    }
                    assert_eq!(probe.denied.get(), 0);
                    assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"staleGeneration"));
                    assert_eq!(v.fact(generation, b"state"), v.sym(b"drifted"));
                    assert_eq!(v.fact(unit, b"state"), v.sym(b"candidate"));
                    assert_eq!(v.number(v.call(v.integer(1), b"retireValue", &[])), 10);
                    v.clear();
                    let receipt = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
                    assert_eq!(v.call(receipt, b"logical", &[]), v.sym(b"discarded"));
                    assert_eq!(v.call(receipt, b"physical", &[]), v.sym(b"disposed"));
                    continue;
                }
                assert_eq!(
                    v.call(v.named(b"Generation"), b"commitPrepared:", &[token]),
                    unit
                );
                let rejected = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
                assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"candidateInUse"));
                let method = v.keep(object(send(
                    id,
                    v.named(b"Execution"),
                    b"methodOfEntry:",
                    &[v.direct(b"retireValue")],
                )));
                held = Some((method, retain(id, v.named(b"Roots"), method)));
                let rollback = v.call(
                    v.named(b"Generation"),
                    b"prepareRollback:plan:",
                    &[unit, v.plan(b"rollback", unit, next, generation)],
                );
                assert_eq!(
                    v.call(
                        v.named(b"Generation"),
                        b"commitPreparedRollback:",
                        &[rollback]
                    ),
                    unit
                );
                assert_eq!(v.number(v.call(v.integer(1), b"retireValue", &[])), 10);
            }
            v.clear();
            let receipt = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
            assert_eq!(v.call(receipt, b"generation", &[]), unit);
            assert_eq!(v.call(receipt, b"logical", &[]), v.sym(b"discarded"));
            assert_eq!(v.fact(unit, b"state"), v.sym(b"discarded"));
            assert!(!path.exists() && !sidecar.exists());
            collect(id, v.named(b"GC"));
            assert_eq!(v.call(receipt, b"logical", &[]), v.sym(b"discarded"));
            if let Some((method, token)) = held {
                assert_eq!(v.call(receipt, b"physical", &[]), v.sym(b"retained"));
                assert_eq!(
                    v.call(receipt, b"reason", &[]),
                    v.sym(b"codeGenerationPinned")
                );
                assert_eq!(
                    v.number(v.call(v.named(b"Generation"), b"retainedGenerationCount", &[])),
                    1
                );
                assert!(
                    v.number(v.call(v.named(b"Generation"), b"retainedGenerationBytes", &[])) > 0
                );
                assert_eq!(v.number(v.call(method, b"invokeOn:", &[v.integer(1)])), 20);
                release(id, v.named(b"Roots"), token);
            } else {
                assert_eq!(v.call(receipt, b"physical", &[]), v.sym(b"disposed"));
                assert_eq!(v.call(receipt, b"reason", &[]), nil);
            }
            let rejected = v.call(v.named(b"Generation"), b"discardCandidate:", &[unit]);
            assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"invalidCandidate"));
            v.clear();
            collect(id, v.named(b"GC"));
            send(id, v.named(b"TCC"), b"checkpoint", &[]);
            assert_eq!(
                v.number(v.call(v.named(b"Generation"), b"retainedGenerationCount", &[])),
                0,
                "{mode}"
            );
            assert_eq!(
                v.number(v.call(v.named(b"Generation"), b"retainedGenerationBytes", &[])),
                0
            );
        }
        v.clear();
        release(id, v.named(b"Roots"), projection_root);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
