//! Current-generation drift and Method retention through real Id owners.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" fn raw(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 91)
}
struct DuringSave {
    id: Protocol,
    original: NativeEntry,
    roots: Oop,
    db: Oop,
    selector: Oop,
    behavior: Oop,
    fired: Cell<bool>,
}
unsafe extern "C" fn edit_during_save(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<DuringSave>();
        let answer = p.original.method.unwrap()(p.original.context, r, a, n);
        let root = answer.value.as_object().map(|o| retain(p.id, p.roots, o));
        p.fired.set(true);
        send(
            p.id,
            p.db,
            b"removeMethod:on:",
            &[Operand::object(p.selector), Operand::object(p.behavior)],
        );
        if let Some(root) = root {
            release(p.id, p.roots, root);
        }
        answer
    }
}
struct RejectDrift {
    id: Protocol,
    original: NativeEntry,
    generation: Oop,
    state: Oop,
    drifted: Oop,
    error: Oop,
    gc: Oop,
    armed: Cell<bool>,
}
unsafe extern "C" fn reject_first_drift(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<RejectDrift>();
        let args = borrowed_slice(a, n).unwrap();
        if args[0].as_object() == Some(p.generation)
            && args[1].as_object() == Some(p.state)
            && args[2].as_object() == Some(p.drifted)
            && p.armed.replace(false)
        {
            collect(p.id, p.gc);
            return Outcome::loaded(Operand::object(p.error));
        }
        p.original.method.unwrap()(p.original.context, r, a, n)
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn ledger_adoption_roots_portable_methods_and_never_readopts_drift() {
    if std::env::var_os("BIRNAM_LEDGER_DRIFT_CASE").is_none() {
        for case in ["raw", "store", "source", "save", "retry", "opaque"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "ledger_adoption_roots_portable_methods_and_never_readopts_drift",
                    "--nocapture",
                ])
                .env("BIRNAM_LEDGER_DRIFT_CASE", case)
                .status()
                .unwrap();
            assert!(status.success(), "{case}");
        }
        return;
    }
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let keep = |o: Oop| {
            retain(id, named(b"Roots"), o);
            o
        };
        let sym = |name: &[u8]| keep(id.intern_name(name).unwrap());
        let call = |r: Oop, name: &[u8], args: &[Oop]| {
            keep(object(send(
                id,
                r,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        };
        let text = |bytes: &[u8]| {
            keep(object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        };
        let integer = |n| {
            keep(object(send(
                id,
                named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        };
        let at = |v, i| call(v, b"at:", &[integer(i)]);
        let cg = named(b"Codegen");
        let db = named(b"db");
        let yes = named(b"true");
        let no = named(b"false");
        let body = b"let ledgerHelper := [| 17]; let ledgerProjected := [| 1]; [Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | [ledgerHelper value]]]; [Reflect defineMethod: #ledgerOverride on: Integer as: [:self | 1]]";
        let evaluated = call(cg, b"evalPersisting:", &[text(body)]);
        assert_eq!(call(evaluated, b"isError", &[]), no);
        let mut source = b"[Module named: #ledgerFixture requires: #[]]; ".to_vec();
        source.extend_from_slice(body);
        source.extend_from_slice(b"; [Reflect removeMethod: #ledgerRemoved on: Integer]");
        let source = text(&source);
        assert_eq!(
            call(
                cg,
                b"recordModuleSource:named:",
                &[source, sym(b"ledgerFixture")]
            ),
            source
        );
        // An alias and a later dependent module deliberately replace the first
        // definition. Adoption compares the final dictionary writer, not both.
        call(
            named(b"Globals"),
            b"at:put:",
            &[sym(b"LedgerIntegerAlias"), named(b"Integer")],
        );
        let override_body =
            b"let ledgerProjected := [| 2]; [Reflect defineMethod: #ledgerOverride on: LedgerIntegerAlias as: [:self | 2]]; let System := System; [Reflect defineMethod: #ledgerLexicalEmbed on: LedgerIntegerAlias as: [:self | [System embedFile: \"never-created.bin\"]]]";
        call(cg, b"evalPersisting:", &[text(override_body)]);
        let mut override_source =
            b"[Module named: #ledgerOverride requires: #[#ledgerFixture]]; ".to_vec();
        override_source.extend_from_slice(override_body);
        call(
            cg,
            b"recordModuleSource:named:",
            &[text(&override_source), sym(b"ledgerOverride")],
        );
        // Generation adoption must remain inert after assets and source files
        // are removed. The compiled method retains the embedded binary payload.
        let temp = std::env::temp_dir().join(format!("birnam-ledger-embed-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("asset.bin"), b"\0\xffasset").unwrap();
        let embedded_body = b"let ledgerEmbeddedGlobal := [| [System embedFile: \"asset.bin\"]]; [Reflect defineMethod: #ledgerEmbedded on: Integer as: [:self | [System embedFile: \"asset.bin\"]]]";
        let file = temp.join("embedded.birnam");
        std::fs::write(&file, embedded_body).unwrap();
        let loaded = call(
            named(b"System"),
            b"load:",
            &[text(file.as_os_str().as_bytes())],
        );
        assert_eq!(call(loaded, b"isError", &[]), no);
        std::fs::remove_dir_all(&temp).unwrap();
        let mut embedded_source = b"[Module named: #ledgerEmbedded requires: #[]]; ".to_vec();
        embedded_source.extend_from_slice(embedded_body);
        call(
            cg,
            b"recordModuleSource:named:",
            &[text(&embedded_source), sym(b"ledgerEmbedded")],
        );
        let projection = call(cg, b"initialGenerationProjection", &[]);
        let generation = call(named(b"Object"), b"clone", &[]);
        for (attr, value) in [
            (b"sourceMap".as_slice(), at(projection, 1)),
            (b"baseDigest", at(projection, 2)),
            (b"generationSelection", at(projection, 3)),
        ] {
            call(db, b"assert:attr:value:", &[generation, sym(attr), value]);
        }
        call(
            db,
            b"assert:attr:value:",
            &[sym(b"dispatch"), sym(b"activeGeneration"), generation],
        );
        let rejected = call(
            named(b"Generation"),
            b"adoptComposedGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"imageRecoveryRequired"));
        let rejected = call(cg, b"beginPortableImageRecovery:", &[text(&[b'a'; 64])]);
        assert_eq!(call(rejected, b"kind", &[]), sym(b"imageRecoveryRequired"));
        let rejected = call(cg, b"recoverPortableImageGeneration:", &[text(&[b'a'; 64])]);
        assert_eq!(call(rejected, b"kind", &[]), sym(b"imageRecoveryRequired"));
        let vt = named(b"IntegerVt");
        let selector = sym(b"ledgerAnswer");
        // A first-class Method capture remains portable when installed again.
        // Do not root the captured Method in this test: after installation, only
        // dictionary ownership and then the adopted ledger keep it alive.
        let captured = object(send(
            id,
            named(b"Integer"),
            b"methodFor:",
            &[Operand::object(selector)],
        ));
        send(
            id,
            db,
            b"addMethod:on:with:",
            &[
                Operand::object(selector),
                Operand::object(vt),
                Operand::object(captured),
            ],
        );
        let prior = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        // Merely claiming portable source cannot adopt a native entry with no
        // image-rebuildable implementation. Keep its displaced Method only until
        // restoring the binding, then let the ledger supply the ownership.
        let method = object(send(
            id,
            named(b"Execution"),
            b"methodOfEntry:",
            &[Operand::entry(prior)],
        ));
        let method_root = retain(id, named(b"Roots"), method);
        install(id, vt, b"ledgerAnswer", raw, core::ptr::null_mut());
        let rejected = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"nonPortableGeneration"));
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(prior)],
        );
        // A portable Method alone is insufficient: its body must agree with the
        // recorded source, and an install declaration must have a live binding.
        let wrong = call(
            cg,
            b"evalPersisting:",
            &[text(
                b"[Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | 99]]",
            )],
        );
        assert_eq!(call(wrong, b"isError", &[]), no);
        let rejected = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"sourceBindingMismatch"));
        call(db, b"removeMethod:on:", &[selector, vt]);
        let rejected = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"sourceBindingMismatch"));
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(prior)],
        );
        call(
            cg,
            b"evalPersisting:",
            &[text(
                b"[Reflect defineMethod: #ledgerRemoved on: Integer as: [:self | 3]]",
            )],
        );
        let rejected = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"sourceBindingMismatch"));
        call(db, b"removeMethod:on:", &[sym(b"ledgerRemoved"), vt]);
        call(
            named(b"Globals"),
            b"at:put:",
            &[sym(b"LedgerIntegerAlias"), named(b"nil")],
        );
        let rejected = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(rejected, b"kind", &[]), sym(b"sourceBindingMismatch"));
        call(
            named(b"Globals"),
            b"at:put:",
            &[sym(b"LedgerIntegerAlias"), named(b"Integer")],
        );
        // Source authority includes the final capture-free global Block body.
        // The dependent module's ledgerProjected overrides the earlier module.
        let globals = named(b"Globals");
        let helper_name = sym(b"ledgerHelper");
        let helper = call(globals, b"at:", &[helper_name]);
        let wrong_helper = call(cg, b"eval:", &[text(b"[| 99]")]);
        for replacement in [wrong_helper, integer(17), named(b"nil")] {
            call(globals, b"at:put:", &[helper_name, replacement]);
            let rejected = call(
                named(b"Generation"),
                b"adoptInitialGeneration:",
                &[generation],
            );
            assert_eq!(call(rejected, b"kind", &[]), sym(b"sourceBindingMismatch"));
        }
        call(globals, b"at:put:", &[helper_name, helper]);
        release(id, named(b"Roots"), method_root);
        assert_eq!(
            call(
                named(b"Generation"),
                b"adoptInitialGeneration:",
                &[generation]
            ),
            generation
        );
        assert_eq!(
            call(
                named(b"Generation"),
                b"adoptInitialGeneration:",
                &[generation]
            ),
            generation
        );
        call(
            db,
            b"assert:attr:value:",
            &[sym(b"unrelated"), sym(b"note"), yes],
        );
        assert_eq!(
            call(named(b"Generation"), b"reconcileCandidate:", &[generation]),
            generation
        );
        // An unrelated tracked publication must not invalidate this ledger.
        call(
            cg,
            b"evalPersisting:",
            &[text(
                b"[Reflect defineMethod: #unmanagedAnswer on: Integer as: [:self | 23]]",
            )],
        );
        assert_eq!(
            call(named(b"Generation"), b"reconcileCandidate:", &[generation]),
            generation
        );
        let removed = call(
            cg,
            b"evalPersisting:",
            &[text(
                b"[Reflect removeMethod: #unmanagedAnswer on: Integer]",
            )],
        );
        assert_eq!(removed, named(b"Integer"));
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(sym(b"unmanagedAnswer")),
                    Operand::object(vt)
                ]
            )
            .kind(),
            birnam_abi::id_protocol::UNIT
        );
        assert_eq!(
            call(named(b"Generation"), b"reconcileCandidate:", &[generation]),
            generation
        );
        let path = std::env::temp_dir().join(format!("birnam-drift-image-{}", std::process::id()));
        std::fs::write(&path, b"previous image").unwrap();
        let filename = text(path.as_os_str().as_bytes());
        let mut during_save = DuringSave {
            id,
            original: prior,
            roots: named(b"Roots"),
            db,
            selector,
            behavior: vt,
            fired: Cell::new(false),
        };
        let cg_vt = named(b"CodegenVt");
        let save_selector = sym(b"compilePortableImageCode:bindings:selection:");
        let case = std::env::var("BIRNAM_LEDGER_DRIFT_CASE").unwrap();
        let assert_selector = sym(b"assert:attr:value:");
        let store_vt = named(b"StoreVt");
        let original_assert = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(assert_selector), Operand::object(store_vt)],
        )
        .as_entry()
        .unwrap();
        let mut reject = RejectDrift {
            id,
            original: original_assert,
            generation,
            state: sym(b"state"),
            drifted: sym(b"drifted"),
            error: call(named(b"Error"), b"kind:", &[sym(b"injectedDriftFailure")]),
            gc: named(b"GC"),
            armed: Cell::new(true),
        };
        match case.as_str() {
            "retry" => {
                let before = call(db, b"currentValue:attr:", &[generation, sym(b"state")]);
                install(
                    id,
                    store_vt,
                    b"assert:attr:value:",
                    reject_first_drift,
                    core::ptr::from_mut(&mut reject).cast(),
                );
                call(db, b"removeMethod:on:", &[selector, vt]);
                let failed = call(named(b"Generation"), b"reconcileCandidate:", &[generation]);
                assert_eq!(call(failed, b"kind", &[]), sym(b"injectedDriftFailure"));
                send(
                    id,
                    store_vt,
                    b"addMethod",
                    &[
                        Operand::object(assert_selector),
                        Operand::entry(original_assert),
                    ],
                );
                assert!(!reject.armed.get());
                assert_eq!(
                    call(db, b"currentValue:attr:", &[generation, sym(b"state")]),
                    before
                );
            }

            "store" => {
                call(db, b"removeMethod:on:", &[selector, vt]);
            }
            "source" => {
                call(
                    cg,
                    b"evalPersisting:",
                    &[text(
                        b"[Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | 19]]",
                    )],
                );
            }
            "opaque" => {
                install(id, vt, b"ledgerAnswer", raw, core::ptr::null_mut());
            }
            "save" => {
                during_save.original = send(
                    id,
                    id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(save_selector), Operand::object(cg_vt)],
                )
                .as_entry()
                .unwrap();
                install(
                    id,
                    cg_vt,
                    b"compilePortableImageCode:bindings:selection:",
                    edit_during_save,
                    core::ptr::from_mut(&mut during_save).cast(),
                );
            }
            _ => {
                send(id, vt, b"removeMethod:", &[Operand::object(selector)]);
            }
        }
        if case == "store" {
            // No implicit observer updates descriptions. Reconciliation is explicit.
            assert_ne!(
                call(db, b"currentValue:attr:", &[generation, sym(b"state")]),
                sym(b"drifted")
            );
        }
        let saved = call(named(b"System"), b"saveImage:", &[filename]);
        if case == "save" {
            send(
                id,
                cg_vt,
                b"addMethod",
                &[
                    Operand::object(save_selector),
                    Operand::entry(during_save.original),
                ],
            );
            assert!(during_save.fired.get());
        }
        if case == "opaque" {
            assert_eq!(call(saved, b"kind", &[]), sym(b"nonPortableGeneration"));
            assert_eq!(std::fs::read(&path).unwrap(), b"previous image");
        } else if case == "save" {
            assert_eq!(saved, named(b"nil"));
            assert_eq!(std::fs::read(&path).unwrap(), b"previous image");
        } else {
            assert_eq!(saved, yes, "an edited live world can be snapshotted as drifted data");
            assert_ne!(std::fs::read(&path).unwrap(), b"previous image");
        }
        std::fs::remove_file(path).unwrap();
        // The inverse Method remains alive after every form of divergence.
        collect(id, named(b"GC"));
        let result = call(method, b"invokeOn:", &[integer(2)]);
        assert_eq!(
            send(
                id,
                named(b"Integer"),
                b"wordOf:",
                &[Operand::object(result)]
            )
            .as_word(),
            Some(17)
        );
        let drift = call(named(b"Generation"), b"reconcileCandidate:", &[generation]);
        assert_eq!(call(drift, b"kind", &[]), sym(b"staleGeneration"));
        assert_eq!(
            call(db, b"currentValue:attr:", &[generation, sym(b"state")]),
            sym(b"drifted")
        );
        // Even restoring the exact entry cannot erase an observed divergence.
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(prior)],
        );
        let readoption = call(
            named(b"Generation"),
            b"adoptInitialGeneration:",
            &[generation],
        );
        assert_eq!(call(readoption, b"kind", &[]), sym(b"staleGeneration"));
        // Keep the authored lifecycle facade parseable as its initial adoption
        // is connected to this native mechanism.
        let facade = text(&std::fs::read("../../modules/compiled-unit.birnam").unwrap());
        assert_eq!(
            call(call(named(b"Lang"), b"parse:", &[facade]), b"isError", &[]),
            no
        );
    }
}
