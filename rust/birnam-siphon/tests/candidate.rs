//! Real candidate objects, inert admission and relocation through the Id owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Outcome},
};
use core::{cell::RefCell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" fn forbidden(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 994)
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn candidate_objects_are_inert_snapshot_bound_and_preserve_factory_captures_and_selection() {
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
        let tokens = RefCell::new(Vec::new());
        let keep = |o: Oop| {
            tokens.borrow_mut().push(retain(id, roots, o));
            o
        };
        let symbol = |name: &[u8]| keep(id.intern_name(name).unwrap());
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
        let integer = |i: usize| {
            keep(object(send(
                id,
                named(b"Integer"),
                b"fromWord:",
                &[Operand::word(i)],
            )))
        };
        let at = |v: Oop, i: usize| call(v, b"at:", &[integer(i)]);
        let bytes = |o: Oop| {
            let b = send(id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        };
        let value = |o: Oop| {
            send(id, named(b"Integer"), b"wordOf:", &[Operand::object(o)])
                .as_word()
                .unwrap()
        };
        let codegen = named(b"Codegen");
        let db = named(b"db");
        let globals = named(b"Globals");
        let yes = named(b"true");
        let no = named(b"false");
        let nil = named(b"nil");
        let baseline=text(b"[Module named: #candidateFactory requires: #[]]; let candidateHelper := [| 11]; [Reflect defineMethod: #candidateAnswer on: Integer as: [:self | [candidateHelper value]]]");
        let candidate=text(b"[Module named: #candidateFactory requires: #[]]; let candidateHelper := [| 22]; [Reflect defineMethod: #candidateAnswer on: Integer as: [:self | [candidateHelper value]]]");
        let name = symbol(b"candidateFactory");
        assert_eq!(
            call(codegen, b"recordModuleSource:named:", &[baseline, name]),
            baseline
        );
        let projection = call(codegen, b"initialGenerationProjection", &[]);
        let generation = call(named(b"Object"), b"clone", &[]);
        for (attr, v) in [
            (b"sourceMap".as_slice(), at(projection, 1)),
            (b"baseDigest", at(projection, 2)),
            (b"generationSelection", at(projection, 3)),
        ] {
            assert_eq!(
                call(db, b"assert:attr:value:", &[generation, symbol(attr), v]),
                db
            );
        }
        call(
            db,
            b"assert:attr:value:",
            &[symbol(b"dispatch"), symbol(b"activeGeneration"), generation],
        );
        let dir = std::env::temp_dir().join(format!("birnam-candidate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for synchronous in [false, true] {
            let path = dir.join(if synchronous {
                "sync.dylib"
            } else {
                "resume.dylib"
            });
            let path_object = text(path.as_os_str().as_bytes());
            let compiled = if synchronous {
                call(
                    codegen,
                    b"compileCandidate:named:output:selection:",
                    &[
                        candidate,
                        name,
                        path_object,
                        call(named(b"Vector"), b"new", &[]),
                    ],
                )
            } else {
                call(
                    codegen,
                    b"compileCandidate:named:output:",
                    &[candidate, name, path_object],
                )
            };
            assert_eq!(
                compiled,
                path_object,
                "candidate compilation failed: {:?}",
                bytes(call(codegen, b"lastError", &[]))
            );
            let artifact = std::fs::read(&path).unwrap();
            let unit = call(named(b"Object"), b"clone", &[]);
            assert_eq!(
                call(codegen, b"inspectCandidate:into:", &[path_object, unit]),
                unit
            );
            assert_eq!(call(codegen, b"candidateMapped:", &[unit]), no);
            assert_eq!(call(globals, b"at:", &[symbol(b"candidateHelper")]), nil);
            let revision = call(
                db,
                b"currentValue:attr:",
                &[unit, symbol(b"sourceRevision")],
            );
            let successor = call(
                codegen,
                b"successorGenerationProjection:replacing:with:unit:",
                &[at(projection, 1), name, revision, unit],
            );
            assert_eq!(at(successor, 0), symbol(b"generationProjection"));
            assert_ne!(bytes(at(successor, 2)), bytes(at(projection, 2)));
            // Rewriting the author path cannot change already inspected bytes.
            std::fs::write(&path, b"corrupt replacement").unwrap();
            let tcc_behavior = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(named(b"TCC"))],
            ));
            let mut prior = Vec::new();
            for selector in [b"compile:".as_slice(), b"compileObject:"] {
                let key = symbol(selector);
                prior.push((
                    key,
                    send(
                        id,
                        id.receiver,
                        b"bindingOf:in:",
                        &[Operand::object(key), Operand::object(tcc_behavior)],
                    ),
                ));
                install(id, tcc_behavior, selector, forbidden, core::ptr::null_mut());
            }
            assert_eq!(call(codegen, b"mapCandidate:", &[unit]), unit);
            assert_eq!(call(codegen, b"candidateMapped:", &[unit]), yes);
            assert_eq!(call(globals, b"at:", &[symbol(b"candidateHelper")]), nil);
            assert_eq!(
                send(
                    id,
                    id.receiver,
                    b"bindingOf:in:",
                    &[
                        Operand::object(symbol(b"candidateAnswer")),
                        Operand::object(named(b"IntegerVt"))
                    ]
                )
                .kind(),
                birnam_abi::id_protocol::UNIT
            );
            collect(id, named(b"GC"));
            assert_eq!(call(codegen, b"mapCandidate:", &[unit]), unit);
            for (key, entry) in prior {
                send(
                    id,
                    tcc_behavior,
                    b"addMethod",
                    &[Operand::object(key), entry],
                );
            }
            let bad = call(named(b"Object"), b"clone", &[]);
            assert_eq!(
                call(
                    call(codegen, b"inspectCandidate:into:", &[path_object, bad]),
                    b"isError",
                    &[]
                ),
                yes
            );
            // Independently execute the private factory object as a compiler
            // fixture. This is not activation: no dispatch/Globals are published.
            assert_eq!(&artifact[..8], b"BIRNAMC1");
            let mut cursor = 44;
            let mut object_units = Vec::new();
            for _ in 0..2 {
                let len =
                    u64::from_le_bytes(artifact[cursor..cursor + 8].try_into().unwrap()) as usize;
                cursor += 8;
                let blob = call(text(&artifact[cursor..cursor + len]), b"asBytes", &[]);
                object_units.push(call(named(b"TCC"), b"loadObject:", &[blob]));
                cursor += len;
            }
            assert_eq!(cursor, artifact.len());
            let factory = call(object_units[0], b"initialize", &[]);
            let empty = call(named(b"Vector"), b"new", &[]);
            let block = call(named(b"Block"), b"fromMethod:captures:", &[factory, empty]);
            let machine = call(named(b"Machine"), b"new", &[]);
            let output = call(machine, b"runBlock:", &[block]);
            let helper = at(output, 0);
            let implementation = at(output, 1);
            assert_eq!(value(call(helper, b"value", &[])), 22);
            assert_eq!(call(implementation, b"captureAt:", &[integer(0)]), helper);
            let method = call(object_units[1], b"initializeWith:", &[implementation]);
            assert_eq!(value(call(method, b"invokeOn:", &[integer(1)])), 22);
            assert_eq!(call(globals, b"at:", &[symbol(b"candidateHelper")]), nil);
            assert_eq!(call(globals, b"at:", &[symbol(b"__candidate_values")]), nil);
        }
        std::fs::remove_dir_all(dir).unwrap();
        for token in tokens.into_inner() {
            release(id, roots, token);
        }
    }
}
