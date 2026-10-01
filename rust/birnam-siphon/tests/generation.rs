//! Initial generation metadata through current parsing, values and GC owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::stackless_selection::{Disposition, inspect_generation_selection};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;

struct Callback {
    id: Protocol,
    gc: Oop,
    prior: NativeEntry,
    codegen: Oop,
    added: Oop,
    name: Oop,
    mutate: Cell<bool>,
}
unsafe extern "C" fn constructing(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Callback>();
        if c.mutate.replace(false) {
            send(
                c.id,
                c.codegen,
                b"recordModuleSource:named:",
                &[Operand::object(c.added), Operand::object(c.name)],
            );
        }
        collect(c.id, c.gc);
        c.prior.method.unwrap()(c.prior.context, r, a, n)
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn initial_projection_is_canonical_inert_rooted_and_rejects_mixed_sources() {
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
        let roots = named(b"Roots");
        let keep = |o: Oop| {
            retain(id, roots, o);
            o
        };
        let text = |bytes: &[u8]| {
            keep(object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        };
        let call = |r: Oop, name: &[u8], a: &[Operand]| keep(object(send(id, r, name, a)));
        let at = |r: Oop, i: usize| {
            call(
                r,
                b"at:",
                &[send(
                    id,
                    named(b"Integer"),
                    b"fromWord:",
                    &[Operand::word(i)],
                )],
            )
        };
        let bytes = |o: Oop| {
            let b = send(id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        };
        let codegen = named(b"Codegen");
        let core = call(codegen, b"initialGenerationProjection", &[]);
        let core_selection = bytes(at(core, 3));
        let core_selection = inspect_generation_selection(&core_selection).unwrap();
        let core_revisions = core_selection.revision_count();
        let core_rows = core_selection.row_count();
        assert_eq!(core_revisions, 2, "Module and Lang are compiled core modules");
        let source_a=text(b"[Module named: #a requires: #[]]; [Reflect defineMethod: #generationAnswer on: Integer as: [:self | 42]]");
        let source_b = text(b"[Module named: #b requires: #[#a]]; let projected := [| 99]");
        for (name, source) in [(b"b".as_slice(), source_b), (b"a".as_slice(), source_a)] {
            assert_eq!(
                call(
                    codegen,
                    b"recordModuleSource:named:",
                    &[
                        Operand::object(source),
                        Operand::object(id.intern_name(name).unwrap())
                    ]
                ),
                source
            );
        }
        let projection = call(codegen, b"initialGenerationProjection", &[]);
        assert_eq!(
            at(projection, 0),
            id.intern_name(b"generationProjection").unwrap()
        );
        let map = at(projection, 1);
        assert_eq!(at(at(map, 0), 0), id.intern_name(b"a").unwrap());
        assert_eq!(at(at(map, 1), 0), id.intern_name(b"b").unwrap());
        assert_eq!(at(at(at(map, 0), 1), 3), source_a);
        assert_eq!(at(at(at(map, 1), 1), 3), source_b);
        assert_eq!(at(at(map, 0), 2), named(b"nil"));
        let digest = bytes(at(projection, 2));
        assert_eq!(digest.len(), 64);
        let selection = bytes(at(projection, 3));
        let parsed = inspect_generation_selection(&selection).unwrap();
        assert_eq!(parsed.revision_count(), core_revisions + 2);
        assert_eq!(parsed.row_count(), core_rows + 3); // named adapter, its body, projected Block
        assert!(
            parsed
                .rows()
                .all(|row| row.unwrap().disposition == Disposition::ResumableRequired)
        );
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(id.intern_name(b"generationAnswer").unwrap()),
                    Operand::object(named(b"IntegerVt"))
                ]
            )
            .kind(),
            birnam_abi::id_protocol::UNIT
        );
        let repeated = call(codegen, b"initialGenerationProjection", &[]);
        assert_eq!(bytes(at(repeated, 2)), digest);
        assert_eq!(bytes(at(repeated, 3)), selection);
        let extra = text(b"[Module named: #c requires: #[]]");
        let behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(named(b"String"))],
        ));
        let key = id.intern_name(b"fromBytes:").unwrap();
        keep(key);
        let prior = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(key), Operand::object(behavior)],
        );
        let c = Box::leak(Box::new(Callback {
            id,
            gc: named(b"GC"),
            prior: prior.as_entry().unwrap(),
            codegen,
            added: extra,
            name: keep(id.intern_name(b"c").unwrap()),
            mutate: Cell::new(false),
        }));
        install(
            id,
            behavior,
            b"fromBytes:",
            constructing,
            (c as *mut Callback).cast(),
        );
        let collected = call(codegen, b"initialGenerationProjection", &[]);
        assert_eq!(bytes(at(collected, 2)), digest);
        c.mutate.set(true);
        let changed = call(codegen, b"initialGenerationProjection", &[]);
        assert_eq!(call(changed, b"isError", &[]), named(b"true"));
        send(id, behavior, b"addMethod", &[Operand::object(key), prior]);
        let fresh = call(codegen, b"initialGenerationProjection", &[]);
        assert_ne!(bytes(at(fresh, 2)), digest);
        assert_eq!(
            inspect_generation_selection(&bytes(at(fresh, 3)))
                .unwrap()
                .revision_count(),
            core_revisions + 3
        );
        assert_eq!(
            call(codegen, b"moduleSourceFor:", &[Operand::object(c.name)]),
            extra
        );
    }
}
