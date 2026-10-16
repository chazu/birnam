//! End-to-end candidate publication, compensation and inverse dispatch.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Vm {
    id: Protocol,
    ns: Oop,
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
            retain(self.id, self.named(b"Roots"), o);
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
struct Failure {
    vm: Vm,
    original: NativeEntry,
    before: Cell<bool>,
    armed: Cell<bool>,
    helper: Cell<Oop>,
}
unsafe extern "C" fn fail_after_removal(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<Failure>();
        if c.before.replace(false) {
            return Outcome::loaded(Operand::object(c.vm.call(
                c.vm.named(b"Error"), b"kind:", &[c.vm.sym(b"injectedBeforeWriteFailure")])));
        }
        let result = c.original.method.unwrap()(c.original.context, r, a, n);
        let args = birnam_abi::borrowed_slice(a, n).unwrap();
        if args[0].as_object() == Some(c.vm.sym(b"txRemove")) && c.armed.replace(false) {
            let entry = c.vm.direct(b"txReplace");
            let block = object(send(
                c.vm.id,
                c.vm.named(b"Execution"),
                b"implementationOfEntry:",
                &[entry],
            ));
            c.helper
                .set(c.vm.call(block, b"captureAt:", &[c.vm.integer(0)]));
            c.vm.set(
                c.vm.sym(b"foreignCallback"),
                b"survives",
                c.vm.named(b"true"),
            );
            return Outcome::loaded(Operand::object(c.vm.call(
                c.vm.named(b"Error"),
                b"kind:",
                &[c.vm.sym(b"injectedPublicationFailure")],
            )));
        }
        result
    }
}
struct PublicationFailure {
    vm: Vm,
    original: NativeEntry,
    destination: Oop,
    armed: Cell<bool>,
}
unsafe extern "C" fn fail_after_publication(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<PublicationFailure>();
        let result = c.original.method.unwrap()(c.original.context, r, a, n);
        let args = birnam_abi::borrowed_slice(a, n).unwrap();
        if args[1].as_object() == Some(c.vm.sym(b"activeGeneration"))
            && args[3].as_object() == Some(c.destination)
            && c.armed.replace(false)
        {
            return Outcome::loaded(Operand::object(c.vm.call(
                c.vm.named(b"Error"),
                b"kind:",
                &[c.vm.sym(b"injectedPublicationFactFailure")],
            )));
        }
        result
    }
}
struct ReleaseFailure {
    vm: Vm,
    original: NativeEntry,
    armed: Cell<bool>,
}
unsafe extern "C" fn fail_after_release(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<ReleaseFailure>();
        let result = c.original.method.unwrap()(c.original.context, r, a, n);
        if c.armed.replace(false) {
            return Outcome::loaded(Operand::object(c.vm.call(
                c.vm.named(b"Error"),
                b"kind:",
                &[c.vm.sym(b"injectedCleanupFailure")],
            )));
        }
        result
    }
}
// Independent owner observations must be consulted even while the transitional
// owner happens to store its values in db. Collect inside each replaced send.
struct GlobalProbe {
    vm: Vm,
    original: NativeEntry,
    calls: Cell<usize>,
    basis_offset: std::rc::Rc<Cell<usize>>,
    bump_after_read: Cell<bool>,
    is_basis: bool,
}
unsafe extern "C" fn probe_global(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<GlobalProbe>();
        p.calls.set(p.calls.get() + 1);
        collect(p.vm.id, p.vm.named(b"GC"));
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if p.bump_after_read.replace(false) {
            p.basis_offset.set(p.basis_offset.get() + 1);
        }
        if p.is_basis && p.basis_offset.get() != 0 && result.value.as_object().is_some() {
            return Outcome::loaded(Operand::object(
                p.vm.integer(p.vm.number(object(result.value)) + p.basis_offset.get()),
            ));
        }
        result
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn candidate_transactions_restore_exact_dispatch_globals_and_preserve_callback_facts() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let v = Vm { id, ns };
        let cg = v.named(b"Codegen");
        let db = v.named(b"db");
        let nil = v.named(b"nil");
        let yes = v.named(b"true");
        let mut global_probes = Vec::new();
        let owner_basis_offset = std::rc::Rc::new(Cell::new(0));
        for selector in [
            b"bindingAt:".as_slice(),
            b"bindingBasis",
            b"restoreBinding:expected:previous:reservation:",
        ] {
            let original = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(v.sym(selector)),
                    Operand::object(object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(v.named(b"Globals"))]))),
                ],
            )
            .as_entry()
            .unwrap();
            if let Some(method) = send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original)]).as_object() { v.keep(method); }
            let mut probe = Box::new(GlobalProbe {
                vm: Vm { id, ns },
                original,
                calls: Cell::new(0),
                basis_offset: owner_basis_offset.clone(),
                bump_after_read: Cell::new(false),
                is_basis: selector == b"bindingBasis",
            });
            install(
                id,
                object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(v.named(b"Globals"))])),
                selector,
                probe_global,
                core::ptr::from_mut(&mut *probe).cast(),
            );
            global_probes.push((selector, probe));
        }

        for (commit, kind) in [
            (
                b"commitPrepared:".as_slice(),
                b"candidatePreparationFailed".as_slice(),
            ),
            (
                b"commitPreparedRollback:",
                b"candidateRollbackPreparationFailed",
            ),
        ] {
            let rejected = v.call(v.named(b"Generation"), commit, &[nil]);
            assert_eq!(v.call(rejected, b"kind", &[]), v.sym(kind));
            assert_eq!(
                v.call(v.named(b"Generation"), commit, &[rejected]),
                rejected
            );
        }

        let baseline=b"let txHelper := [| 10]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txRemove on: Integer as: [:self | 30]]";
        let initial = v.call(cg, b"evalPersisting:", &[v.text(baseline)]);
        assert_eq!(v.call(initial, b"isError", &[]), v.named(b"false"));
        let mut source = b"[Module named: #txFixture requires: #[]]; ".to_vec();
        source.extend_from_slice(baseline);
        let source = v.text(&source);
        v.call(
            cg,
            b"recordModuleSource:named:",
            &[source, v.sym(b"txFixture")],
        );
        let projection = v.call(cg, b"initialGenerationProjection", &[]);
        let generation = v.carrier(projection, nil, nil);
        v.set(v.sym(b"dispatch"), b"activeGeneration", generation);
        // A binding callback changes only its owner's revision. Neither adoption
        // nor later reconciliation may call that a coherent observation.
        global_probes[0].1.bump_after_read.set(true);
        let unstable = v.call(
            v.named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(v.call(unstable, b"kind", &[]), v.sym(b"staleGeneration"));
        owner_basis_offset.set(0);

        assert_eq!(
            v.call(
                v.named(b"Generation"),
                b"adoptInitialGeneration:",
                &[generation]
            ),
            generation
        );
        v.call(cg, b"sealInitialGeneration", &[]);
        global_probes[0].1.bump_after_read.set(true);
        let unstable = v.call(
            v.named(b"Generation"),
            b"reconcileCandidate:",
            &[generation],
        );
        assert_eq!(v.call(unstable, b"kind", &[]), v.sym(b"staleGeneration"));
        owner_basis_offset.set(0);
        assert_eq!(
            v.call(
                v.named(b"Generation"),
                b"reconcileCandidate:",
                &[generation]
            ),
            generation
        );

        let original_helper = v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]);
        let original_replace = v.direct(b"txReplace").as_entry().unwrap();
        let original_remove = v.direct(b"txRemove").as_entry().unwrap();
        let source=v.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 20]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txAdded on: Integer as: [:self | 40]]; [Reflect removeMethod: #txRemove on: Integer]");
        let dir = std::env::temp_dir().join(format!(
            "birnam-generation-transaction-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for synchronous in [false, true] {
            let path = v.text(
                dir.join(if synchronous { "sync.o" } else { "resume.o" })
                    .as_os_str()
                    .as_bytes(),
            );
            let compiled = if synchronous {
                v.call(
                    v.named(b"Generation"),
                    b"compileCandidate:named:output:selection:",
                    &[source, v.sym(b"txFixture"), path, v.vector(&[])],
                )
            } else {
                v.call(
                    v.named(b"Generation"),
                    b"compileCandidate:named:output:",
                    &[source, v.sym(b"txFixture"), path],
                )
            };
            assert_eq!(compiled, path);
            let unit = v.call(v.named(b"Object"), b"clone", &[]);
            assert_eq!(
                v.call(
                    v.named(b"Generation"),
                    b"inspectCandidate:into:",
                    &[path, unit]
                ),
                unit
            );
            assert_eq!(
                v.call(v.named(b"Generation"), b"mapCandidate:", &[unit]),
                unit
            );
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
            let owner_token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            let store_basis = v.number(v.call(db, b"basis", &[]));
            global_probes[1].1.basis_offset.set(1);
            let stale_owner = v.call(v.named(b"Generation"), b"commitPrepared:", &[owner_token]);
            assert_eq!(v.call(stale_owner, b"kind", &[]), v.sym(b"staleGeneration"));
            assert_eq!(v.number(v.call(db, b"basis", &[])), store_basis);
            global_probes[1].1.basis_offset.set(0);
            let token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            assert_eq!(v.call(token, b"isError", &[]), v.named(b"false"));
            v.set(
                v.sym(b"unrelated"),
                b"version",
                v.integer(if synchronous { 2 } else { 1 }),
            );
            let stale = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(v.call(stale, b"kind", &[]), v.sym(b"staleGeneration"));
            let consumed = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(
                v.call(consumed, b"kind", &[]),
                v.sym(b"invalidPreparedToken")
            );
            let token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            v.call(ns, b"at:put:", &[v.sym(b"TxPreparedToken"), token]);
            let unsafe_commit = v.call(
                cg,
                b"evalPersisting:",
                &[v.text(b"[Generation commitPrepared: TxPreparedToken]")],
            );
            assert_eq!(
                v.call(unsafe_commit, b"kind", &[]),
                v.sym(b"generationNotQuiescent")
            );
            let consumed = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(
                v.call(consumed, b"kind", &[]),
                v.sym(b"invalidPreparedToken")
            );
            let db_vt = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(db)],
            ));
            let remove_key = v.sym(b"removeMethod:");
            let original = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(remove_key), Operand::object(id.receiver)],
            )
            .as_entry()
            .unwrap();
            let mut failure = Failure {
                vm: Vm { id, ns },
                original,
                before: Cell::new(true),
                armed: Cell::new(true),
                helper: Cell::new(core::ptr::null_mut()),
            };
            install(
                id,
                id.receiver,
                b"removeMethod:",
                fail_after_removal,
                core::ptr::from_mut(&mut failure).cast(),
            );
            let token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            let rejected = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"injectedBeforeWriteFailure"));
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            assert_eq!(v.direct(b"txRemove").as_entry(), Some(original_remove));
            let token = v.call(v.named(b"Generation"), b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)]);
            let rejected = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            let reason = v.call(cg, b"lastError", &[]);
            let description = if reason == nil {
                None
            } else {
                send(id, reason, b"bytes", &[]).as_bytes()
            };
            let description = description.map(|b| {
                String::from_utf8_lossy(birnam_abi::borrowed_bytes(b.pointer, b.length).unwrap())
                    .into_owned()
            });
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"injectedPublicationFailure"),
                "commit failure: {:?}",
                description
            );
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            assert_eq!(v.direct(b"txRemove").as_entry(), Some(original_remove));
            assert_eq!(v.direct(b"txAdded").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                original_helper
            );
            assert_eq!(v.fact(v.sym(b"foreignCallback"), b"survives"), yes);
            assert_eq!(v.fact(unit, b"state"), v.sym(b"candidate"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), generation);
            assert_eq!(
                v.number(v.call(db, b"reservedCompensationFor:", &[unit])),
                0
            );
            let restore_key = v.sym(b"restoreValue:attr:expected:previous:reservation:");
            let original_restore = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(restore_key), Operand::object(db_vt)],
            )
            .as_entry()
            .unwrap();
            let mut publication_failure = PublicationFailure {
                vm: Vm { id, ns },
                original: original_restore,
                destination: next,
                armed: Cell::new(true),
            };
            install(
                id,
                db_vt,
                b"restoreValue:attr:expected:previous:reservation:",
                fail_after_publication,
                core::ptr::from_mut(&mut publication_failure).cast(),
            );
            let token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            let rejected = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"injectedPublicationFactFailure")
            );
            assert_eq!(v.fact(unit, b"state"), v.sym(b"candidate"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), generation);
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            assert_eq!(v.direct(b"txRemove").as_entry(), Some(original_remove));
            assert_eq!(v.direct(b"txAdded").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                original_helper
            );
            collect(id, v.named(b"GC"));
            let token = v.call(
                v.named(b"Generation"),
                b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)],
            );
            assert_eq!(
                v.call(v.named(b"Generation"), b"commitPrepared:", &[token]),
                unit
            );
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                failure.helper.get()
            );
            assert_eq!(v.number(v.call(v.integer(2), b"txReplace", &[])), 20);
            assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
            assert_eq!(v.direct(b"txRemove").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), next);
            assert_eq!(
                v.call(v.named(b"Generation"), b"reconcileCandidate:", &[unit]),
                unit
            );
            assert!(v.number(v.call(db, b"reservedCompensationFor:", &[unit])) > 0);
            publication_failure.destination = generation;
            publication_failure.armed.set(true);
            let token = v.call(
                v.named(b"Generation"),
                b"prepareRollback:plan:",
                &[unit, v.plan(b"rollback", unit, next, generation)],
            );
            let rejected = v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[token]);
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"injectedPublicationFactFailure")
            );
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), next);
            assert_eq!(v.number(v.call(v.integer(2), b"txReplace", &[])), 20);
            assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
            assert_eq!(v.direct(b"txRemove").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(
                v.call(v.named(b"Generation"), b"reconcileCandidate:", &[unit]),
                unit
            );
            let release_key = v.sym(b"releaseCompensationFor:");
            let original_release = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(release_key), Operand::object(db_vt)],
            )
            .as_entry()
            .unwrap();
            let mut release_failure = ReleaseFailure {
                vm: Vm { id, ns },
                original: original_release,
                armed: Cell::new(true),
            };
            install(
                id,
                db_vt,
                b"releaseCompensationFor:",
                fail_after_release,
                core::ptr::from_mut(&mut release_failure).cast(),
            );
            let token = v.call(
                v.named(b"Generation"),
                b"prepareRollback:plan:",
                &[unit, v.plan(b"rollback", unit, next, generation)],
            );
            let rejected = v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[token]);
            assert_eq!(
                v.call(rejected, b"kind", &[]),
                v.sym(b"injectedCleanupFailure")
            );
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), next);
            assert_eq!(v.number(v.call(v.integer(2), b"txReplace", &[])), 20);
            assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
            assert_eq!(v.direct(b"txRemove").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                failure.helper.get()
            );
            assert_eq!(
                v.call(v.named(b"Generation"), b"reconcileCandidate:", &[unit]),
                unit
            );
            assert!(v.number(v.call(db, b"reservedCompensationFor:", &[unit])) > 0);
            let token = v.call(
                v.named(b"Generation"),
                b"prepareRollback:plan:",
                &[unit, v.plan(b"rollback", unit, next, generation)],
            );
            assert_eq!(v.call(token, b"isError", &[]), v.named(b"false"));
            assert_eq!(
                v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[token]),
                unit
            );
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            assert_eq!(v.direct(b"txRemove").as_entry(), Some(original_remove));
            assert_eq!(v.direct(b"txAdded").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                original_helper
            );
            assert_eq!(v.number(v.call(v.integer(2), b"txReplace", &[])), 10);
            assert_eq!(v.number(v.call(v.integer(2), b"txRemove", &[])), 30);
            assert_eq!(v.fact(unit, b"state"), v.sym(b"rolledBack"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), generation);
            assert_eq!(
                v.number(v.call(db, b"reservedCompensationFor:", &[unit])),
                0
            );
            send(
                id,
                id.receiver,
                b"addMethod",
                &[Operand::object(remove_key), Operand::entry(original)],
            );
            send(
                id,
                db_vt,
                b"addMethod",
                &[
                    Operand::object(restore_key),
                    Operand::entry(original_restore),
                ],
            );
            send(
                id,
                db_vt,
                b"addMethod",
                &[
                    Operand::object(release_key),
                    Operand::entry(original_release),
                ],
            );
            assert_eq!(
                v.call(
                    v.named(b"Generation"),
                    b"reconcileCandidate:",
                    &[generation]
                ),
                generation
            );
            if synchronous {
                let fresh = v.call(v.named(b"Object"), b"clone", &[]);
                assert_eq!(
                    v.call(
                        v.named(b"Generation"),
                        b"inspectCandidate:into:",
                        &[path, fresh]
                    ),
                    fresh
                );
                assert_eq!(
                    v.call(v.named(b"Generation"), b"mapCandidate:", &[fresh]),
                    fresh
                );
                let projected = v.call(
                    v.named(b"Generation"),
                    b"successorGenerationProjection:replacing:with:unit:",
                    &[
                        v.at(projection, 1),
                        v.sym(b"txFixture"),
                        v.fact(fresh, b"sourceRevision"),
                        fresh,
                    ],
                );
                let changed = v.carrier(projected, generation, fresh);
                let token = v.call(
                    v.named(b"Generation"),
                    b"prepareActivation:plan:",
                    &[fresh, v.plan(b"activate", fresh, generation, changed)],
                );
                assert_eq!(
                    v.call(v.named(b"Generation"), b"commitPrepared:", &[token]),
                    fresh
                );
                let token = v.call(
                    v.named(b"Generation"),
                    b"prepareRollback:plan:",
                    &[fresh, v.plan(b"rollback", fresh, changed, generation)],
                );
                send(
                    id,
                    v.named(b"IntegerVt"),
                    b"removeMethod:",
                    &[Operand::object(v.sym(b"txReplace"))],
                );
                let rejected = v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[token]);
                assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"staleGeneration"));
                assert_eq!(v.fact(fresh, b"state"), v.sym(b"drifted"));
                assert_eq!(v.fact(fresh, b"rollbackAvailable"), v.named(b"false"));
                assert_eq!(v.direct(b"txReplace").kind(), birnam_abi::id_protocol::UNIT);
                assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), changed);
                assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
            }
        }
        for (selector, probe) in global_probes {
            assert!(
                probe.calls.get() > 0,
                "binding owner send was bypassed: {:?}",
                selector
            );
            send(
                id,
                object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(v.named(b"Globals"))])),
                b"addMethod",
                &[
                    Operand::object(v.sym(selector)),
                    Operand::entry(probe.original),
                ],
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
