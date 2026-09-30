//! Current-generation drift and Method retention through real Id owners.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Outcome},
};
use core::ffi::c_void;
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" fn raw(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 91)
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn ledger_adoption_roots_portable_methods_and_never_readopts_drift() {
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
        let body = b"let ledgerHelper := [| 17]; [Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | [ledgerHelper value]]]";
        let evaluated = call(cg, b"evalPersisting:", &[text(body)]);
        assert_eq!(call(evaluated, b"isError", &[]), no);
        let mut source = b"[Module named: #ledgerFixture requires: #[]]; ".to_vec();
        source.extend_from_slice(body);
        let source = text(&source);
        assert_eq!(
            call(
                cg,
                b"recordModuleSource:named:",
                &[source, sym(b"ledgerFixture")]
            ),
            source
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
        let rejected = call(cg, b"adoptComposedGeneration:", &[generation]);
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
        let rejected = call(cg, b"adoptInitialGeneration:", &[generation]);
        assert_eq!(call(rejected, b"kind", &[]), sym(b"nonPortableGeneration"));
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(prior)],
        );
        release(id, named(b"Roots"), method_root);
        assert_eq!(
            call(cg, b"adoptInitialGeneration:", &[generation]),
            generation
        );
        assert_eq!(
            call(cg, b"adoptInitialGeneration:", &[generation]),
            generation
        );
        call(
            db,
            b"assert:attr:value:",
            &[sym(b"unrelated"), sym(b"note"), yes],
        );
        assert_eq!(call(cg, b"reconcileCandidate:", &[generation]), generation);
        // Removing a managed entry is visible even without a lifecycle/MOP
        // operation. The inverse Method must survive this collection.
        send(id, vt, b"removeMethod:", &[Operand::object(selector)]);
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
        let drift = call(cg, b"reconcileCandidate:", &[generation]);
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
        let readoption = call(cg, b"adoptInitialGeneration:", &[generation]);
        assert_eq!(call(readoption, b"kind", &[]), sym(b"staleGeneration"));
        // Keep the authored lifecycle facade parseable as its initial adoption
        // is connected to this native mechanism.
        let facade = text(&std::fs::read("../../src/id/compiled-unit.birnam").unwrap());
        assert_eq!(
            call(call(named(b"Lang"), b"parse:", &[facade]), b"isError", &[]),
            no
        );
    }
}
