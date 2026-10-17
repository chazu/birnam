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
    unsafe fn prototype(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Globals"), b"at:", &[self.sym(name)]) }
    }
    unsafe fn record(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.prototype(name), b"clone", &[]) }
    }
    unsafe fn fact(&self, e: Oop, a: &[u8]) -> Oop {
        unsafe {
            if a == b"activeGeneration" { return self.call(self.prototype(b"DispatchGeneration"), b"current", &[]); }
            if a == b"version" || a == b"survives" { return self.call(self.named(b"db"), b"currentValue:attr:", &[e, self.sym(a)]); }
            self.call(self.named(b"Reflect"), b"slot:in:", &[self.sym(a), e])
        }
    }
    unsafe fn set(&self, e: Oop, a: &[u8], v: Oop) {
        unsafe {
            if a == b"activeGeneration" { self.call(self.prototype(b"DispatchGeneration"), b"current:", &[v]); }
            else if a == b"version" || a == b"survives" { self.call(self.named(b"db"), b"assert:attr:value:", &[e, self.sym(a), v]); }
            else { assert_eq!(self.call(self.named(b"Reflect"), b"slot:in:put:", &[self.sym(a), e, v]), e); }
        }
    }
    unsafe fn carrier(&self, p: Oop, predecessor: Oop, unit: Oop) -> Oop {
        unsafe {
            let g = self.record(b"DispatchGeneration");
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
            self.vector(&[
                self.sym(action),
                unit,
                self.sym(b"txFixture"),
                self.named(b"nil"),
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
// A legitimate replacement setter may edit another dictionary entry and return
// an ordinary success value. A no-op or returned Error must still fail the batch.
struct SetterEffects {
    vm: Vm,
    original: NativeEntry,
    noop: Cell<bool>,
    fail: Cell<bool>,
    effects: Cell<usize>,
}
unsafe extern "C" fn setter_effects(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<SetterEffects>();
        let args = birnam_abi::borrowed_slice(a, n).unwrap();
        if args[1].as_object() != Some(p.vm.sym(b"txReplace")) {
            return p.original.method.unwrap()(p.original.context, r, a, n);
        }
        if p.noop.replace(false) { return Outcome::loaded(args[0]); }
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if result.status != 0 { return result; }
        if p.effects.get() < 2 {
            let extra = [args[0], Operand::object(p.vm.sym(b"txSetterSideEffect"))];
            let effect = p.original.method.unwrap()(p.original.context, r, extra.as_ptr(), extra.len());
            if effect.status != 0 { return effect; }
            p.effects.set(p.effects.get() + 1);
        }
        if p.fail.replace(false) {
            return Outcome::loaded(Operand::object(p.vm.call(p.vm.named(b"Error"), b"kind:", &[p.vm.sym(b"setterReportedFailure")])));
        }
        Outcome::loaded(Operand::object(r))
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
        if args[0].as_object() == Some(c.destination)
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
        lifecycle_records(id, ns);
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
        // A separate managed module is outside this candidate's read/write set.
        let independent = b"let txIndependentGlobal := [| 5]; [Reflect defineMethod: #txIndependent on: Integer as: [:self | 5]]; [Reflect defineMethod: #txSetterSideEffect on: Integer as: [:self | 5]]";
        v.call(cg, b"evalPersisting:", &[v.text(independent)]);
        let mut independent_source = b"[Module named: #txIndependentFixture requires: #[]]; ".to_vec();
        independent_source.extend_from_slice(independent);
        v.call(cg, b"recordModuleSource:named:", &[v.text(&independent_source), v.sym(b"txIndependentFixture")]);
        let independent_global = v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txIndependentGlobal")]);
        let independent_entry = v.direct(b"txIndependent").as_entry().unwrap();
        let side_effect_entry = v.direct(b"txSetterSideEffect").as_entry().unwrap();
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
        let source=v.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 20]; let txNewHelper := [| 7]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txAdded on: Integer as: [:self | 40]]; [Reflect removeMethod: #txRemove on: Integer]");
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
            let unit = v.record(b"CompiledUnit");
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
            // Explicit facts at lifecycle field names are independent data.
            v.call(db, b"assert:attr:value:", &[unit, v.sym(b"state"), v.sym(b"userAnnotation")]);
            let changed_token = v.call(v.named(b"Generation"), b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)]);
            v.set(unit, b"state", v.sym(b"externalEdit"));
            let stale_fields = v.call(v.named(b"Generation"), b"commitPrepared:", &[changed_token]);
            assert_eq!(v.call(stale_fields, b"kind", &[]), v.sym(b"staleGeneration"));
            assert_eq!(v.fact(unit, b"state"), v.sym(b"externalEdit"));
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            v.set(unit, b"state", v.sym(b"candidate"));
            // A changed projected Global is relevant even when it is absent from
            // the old managed ledger. Present nil must not be confused with absence.
            let binding_token = v.call(v.named(b"Generation"), b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)]);
            let existing_method = v.keep(object(send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original_replace)])));
            v.call(v.named(b"IntegerVt"), b"addMethod:as:", &[existing_method, v.sym(b"txAdded")]);
            let stale_binding = v.call(v.named(b"Generation"), b"commitPrepared:", &[binding_token]);
            assert_eq!(v.call(stale_binding, b"kind", &[]), v.sym(b"staleGeneration"));
            assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 10);
            send(id, v.named(b"IntegerVt"), b"removeMethod:", &[Operand::object(v.sym(b"txAdded"))]);
            let owner_token = v.call(v.named(b"Generation"), b"prepareActivation:plan:",
                &[unit, v.plan(b"activate", unit, generation, next)]);
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txNewHelper"), nil]);
            let stale_owner = v.call(v.named(b"Generation"), b"commitPrepared:", &[owner_token]);
            assert_eq!(v.call(stale_owner, b"kind", &[]), v.sym(b"staleGeneration"));
            assert_eq!(v.number(v.call(v.call(v.named(b"Globals"), b"bindingAt:", &[v.sym(b"txNewHelper")]), b"size", &[])), 1);
            v.call(v.named(b"Globals"), b"remove:", &[v.sym(b"txNewHelper")]);
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
            assert_eq!(v.call(v.named(b"Generation"), b"discardPrepared:", &[token]), unit);
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
            let dispatch_vt = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(v.prototype(b"DispatchGeneration"))],
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
            let restore_key = v.sym(b"current:");
            let original_restore = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(restore_key), Operand::object(dispatch_vt)],
            )
            .as_entry()
            .unwrap();
            if let Some(method) = send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original_restore)]).as_object() { v.keep(method); }
            let mut publication_failure = PublicationFailure {
                vm: Vm { id, ns },
                original: original_restore,
                destination: next,
                armed: Cell::new(true),
            };
            install(
                id,
                dispatch_vt,
                b"current:",
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
            let setter_key = v.sym(b"addMethod:as:");
            let original_setter = send(id, id.receiver, b"bindingOf:in:", &[Operand::object(setter_key), Operand::object(id.receiver)]).as_entry().unwrap();
            if let Some(method) = send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original_setter)]).as_object() { v.keep(method); }
            let effects = SetterEffects { vm: Vm { id, ns }, original: original_setter, noop: Cell::new(true), fail: Cell::new(true), effects: Cell::new(0) };
            install(id, id.receiver, b"addMethod:as:", setter_effects, core::ptr::from_ref(&effects).cast_mut().cast());
            let token = v.call(v.named(b"Generation"), b"prepareActivation:plan:", &[unit, v.plan(b"activate", unit, generation, next)]);
            let noop = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(v.call(noop, b"kind", &[]), v.sym(b"candidateCommitFailed"));
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            assert_eq!(v.fact(unit, b"state"), v.sym(b"candidate"));
            let token = v.call(v.named(b"Generation"), b"prepareActivation:plan:", &[unit, v.plan(b"activate", unit, generation, next)]);
            let reported = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            assert_eq!(v.call(reported, b"kind", &[]), v.sym(b"setterReportedFailure"));
            assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
            // Compensation must not erase the callback's unrelated binding. Its
            // second write installs the restored Method under that separate name.
            assert_eq!(v.number(v.call(v.integer(2), b"txSetterSideEffect", &[])), 10);
            effects.effects.set(0);
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txIndependentGlobal"), v.integer(321)]);
            send(id, v.named(b"IntegerVt"), b"removeMethod:", &[Operand::object(v.sym(b"txIndependent"))]);
            collect(id, v.named(b"GC"));
            let token = v.call(v.named(b"Generation"), b"prepareActivation:plan:", &[unit, v.plan(b"activate", unit, generation, next)]);
            let epoch = send(id, id.receiver, b"bindingEpoch", &[]).as_word().unwrap();
            // Parsing/constructing a branch Block and an unrelated method edit
            // are normal work between prepare and commit, not token expiry.
            v.call(cg, b"evalPersisting:", &[v.text(b"[:x | [x isNil]]")]);
            let prior_method = v.keep(object(send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original_replace)])));
            v.call(v.named(b"IntegerVt"), b"addMethod:as:", &[prior_method, v.sym(b"txUnrelated")]);
            send(id, v.named(b"IntegerVt"), b"removeMethod:", &[Operand::object(v.sym(b"txReplace"))]);
            v.call(v.named(b"IntegerVt"), b"addMethod:as:", &[prior_method, v.sym(b"txReplace")]);
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txNewHelper"), nil]);
            v.call(v.named(b"Globals"), b"remove:", &[v.sym(b"txNewHelper")]);

            assert!(send(id, id.receiver, b"bindingEpoch", &[]).as_word().unwrap() > epoch);
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txUnrelatedGlobal"), v.integer(123)]);
            global_probes[1].1.basis_offset.set(1);
            v.set(v.sym(b"unrelated"), b"version", v.integer(99));
            let store = v.keep(v.named(b"Store"));
            v.keep(db);
            v.call(ns, b"remove:", &[v.sym(b"Store")]);
            v.call(ns, b"remove:", &[v.sym(b"db")]);
            let committed = v.call(v.named(b"Generation"), b"commitPrepared:", &[token]);
            v.call(ns, b"at:put:", &[v.sym(b"Store"), store]);
            v.call(ns, b"at:put:", &[v.sym(b"db"), db]);
            assert_eq!(committed, unit);
            // Explicit validation may observe another managed module's drift,
            // but must leave this activation's inverse usable.
            let mismatch = v.call(v.named(b"Generation"), b"reconcileCandidate:", &[unit]);
            assert_eq!(v.call(mismatch, b"kind", &[]), v.sym(b"staleGeneration"));
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.fact(unit, b"rollbackAvailable"), yes);
            global_probes[1].1.basis_offset.set(0);
            assert!(effects.effects.get() > 0, "replacement setter did not run");
            let side_entry = v.direct(b"txSetterSideEffect");
            assert_eq!(v.number(v.call(v.integer(2), b"txSetterSideEffect", &[])), 20);
            assert_eq!(
                v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txHelper")]),
                failure.helper.get()
            );
            assert_eq!(v.number(v.call(v.integer(2), b"txReplace", &[])), 20);
            assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
            assert_eq!(v.direct(b"txRemove").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), next);
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.number(v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txIndependentGlobal")])), 321);
            assert_eq!(v.direct(b"txIndependent").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(v.number(v.call(db, b"reservedCompensationFor:", &[unit])), 0);
            effects.effects.set(2);
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
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.number(v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txIndependentGlobal")])), 321);
            assert_eq!(v.direct(b"txIndependent").kind(), birnam_abi::id_protocol::UNIT);
            let globals_vt = object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(v.named(b"Globals"))]));
            let release_key = v.sym(b"releaseBindingCompensationFor:");
            let original_release = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(release_key), Operand::object(globals_vt)],
            )
            .as_entry()
            .unwrap();
            if let Some(method) = send(id, v.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(original_release)]).as_object() { v.keep(method); }
            let mut release_failure = ReleaseFailure {
                vm: Vm { id, ns },
                original: original_release,
                armed: Cell::new(true),
            };
            install(
                id,
                globals_vt,
                b"releaseBindingCompensationFor:",
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
            assert_eq!(v.fact(unit, b"state"), v.sym(b"active"));
            assert_eq!(v.number(v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txIndependentGlobal")])), 321);
            assert_eq!(v.direct(b"txIndependent").kind(), birnam_abi::id_protocol::UNIT);
            assert_eq!(v.number(v.call(db, b"reservedCompensationFor:", &[unit])), 0);
            let token = v.call(
                v.named(b"Generation"),
                b"prepareRollback:plan:",
                &[unit, v.plan(b"rollback", unit, next, generation)],
            );
            assert_eq!(v.call(token, b"isError", &[]), v.named(b"false"));
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txIndependentGlobal"), v.integer(654)]);
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txUnrelatedGlobal"), v.integer(456)]);
            v.call(v.named(b"IntegerVt"), b"addMethod:as:", &[prior_method, v.sym(b"txBetweenRollback")]);
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
            assert_eq!(v.direct(b"txSetterSideEffect").as_entry(), side_entry.as_entry());
            collect(id, v.named(b"GC"));
            assert_eq!(v.number(v.call(v.integer(2), b"txSetterSideEffect", &[])), 20);
            assert_eq!(v.number(v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txUnrelatedGlobal")])), 456);
            assert_eq!(v.number(v.call(v.call(v.named(b"Globals"), b"bindingAt:", &[v.sym(b"txNewHelper")]), b"size", &[])), 0);
            send(id, id.receiver, b"addMethod", &[Operand::object(setter_key), Operand::entry(original_setter)]);
            assert_eq!(v.number(v.call(v.named(b"Globals"), b"at:", &[v.sym(b"txIndependentGlobal")])), 654);
            assert_eq!(v.direct(b"txIndependent").kind(), birnam_abi::id_protocol::UNIT);
            // Restore the independent module before explicitly certifying the whole ledger.
            v.call(v.named(b"Globals"), b"at:put:", &[v.sym(b"txIndependentGlobal"), independent_global]);
            for (key, entry) in [(b"txIndependent".as_slice(), independent_entry), (b"txSetterSideEffect", side_effect_entry)] {
                send(id, v.named(b"IntegerVt"), b"addMethod", &[Operand::object(v.sym(key)), Operand::entry(entry)]);
            }
            for key in [b"txUnrelated".as_slice(), b"txBetweenRollback"] {
                send(id, v.named(b"IntegerVt"), b"removeMethod:", &[Operand::object(v.sym(key))]);
            }

            assert_eq!(v.call(db, b"currentValue:attr:", &[unit, v.sym(b"state")]), v.sym(b"userAnnotation"));
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
                dispatch_vt,
                b"addMethod",
                &[
                    Operand::object(restore_key),
                    Operand::entry(original_restore),
                ],
            );
            send(
                id,
                globals_vt,
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
                let fresh = v.record(b"CompiledUnit");
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
                let installed_replace = v.direct(b"txReplace").as_entry().unwrap();
                send(
                    id,
                    v.named(b"IntegerVt"),
                    b"removeMethod:",
                    &[Operand::object(v.sym(b"txReplace"))],
                );
                let rejected = v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[token]);
                assert_eq!(v.call(rejected, b"kind", &[]), v.sym(b"staleGeneration"));
                // A scoped conflict does not poison the generation or revoke the inverse.
                assert_eq!(v.fact(fresh, b"state"), v.sym(b"active"));
                assert_eq!(v.fact(fresh, b"rollbackAvailable"), yes);
                assert_eq!(v.direct(b"txReplace").kind(), birnam_abi::id_protocol::UNIT);
                assert_eq!(v.fact(v.sym(b"dispatch"), b"activeGeneration"), changed);
                assert_eq!(v.number(v.call(v.integer(2), b"txAdded", &[])), 40);
                send(id, v.named(b"IntegerVt"), b"addMethod", &[Operand::object(v.sym(b"txReplace")), Operand::entry(installed_replace)]);
                let retry = v.call(v.named(b"Generation"), b"prepareRollback:plan:", &[fresh, v.plan(b"rollback", fresh, changed, generation)]);
                assert_eq!(v.call(v.named(b"Generation"), b"commitPreparedRollback:", &[retry]), fresh);
                assert_eq!(v.direct(b"txReplace").as_entry(), Some(original_replace));
                // Scoped success has not silently adopted the other module's edits.
                send(id, v.named(b"IntegerVt"), b"removeMethod:", &[Operand::object(v.sym(b"txIndependent"))]);
                let certificate = v.call(v.named(b"Generation"), b"reconcileCandidate:", &[generation]);
                assert_eq!(v.call(certificate, b"kind", &[]), v.sym(b"staleGeneration"));
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
