//! Real candidate objects, inert admission and relocation through the Id owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" fn forbidden(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 994)
}
struct Drift {
    id: Protocol,
    original: NativeEntry,
    db: Oop,
    entity: Oop,
    attribute: Oop,
    value: Oop,
    fired: Cell<bool>,
}
unsafe extern "C" fn drift_basis(
    c: *mut c_void,
    receiver: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<Drift>();
        let answer = c.original.method.unwrap()(c.original.context, receiver, a, n);
        if !c.fired.replace(true) {
            c.id.send_named(
                c.db,
                b"assert:attr:value:",
                &[
                    Operand::object(c.entity),
                    Operand::object(c.attribute),
                    Operand::object(c.value),
                ],
            )
            .unwrap();
        }
        answer
    }
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
        assert_eq!(
            call(codegen, b"adoptInitialGeneration:", &[generation]),
            generation
        );
        assert_eq!(
            call(codegen, b"adoptInitialGeneration:", &[generation]),
            generation
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
            let next = call(named(b"Object"), b"clone", &[]);
            for (attr, v) in [
                (b"sourceMap".as_slice(), at(successor, 1)),
                (b"baseDigest", at(successor, 2)),
                (b"generationSelection", at(successor, 3)),
                (b"predecessor", generation),
                (b"deltaUnit", unit),
            ] {
                call(db, b"assert:attr:value:", &[next, symbol(attr), v]);
            }
            let publication = || {
                let event = call(named(b"Object"), b"clone", &[]);
                let basis = call(db, b"basis", &[]);
                let mut plan = call(named(b"Vector"), b"new", &[]);
                for value in [
                    symbol(b"activate"),
                    unit,
                    name,
                    nil,
                    event,
                    basis,
                    generation,
                    next,
                ] {
                    plan = call(plan, b"add:", &[value]);
                }
                plan
            };
            let unmapped = call(codegen, b"prepareActivation:plan:", &[unit, publication()]);
            assert_eq!(
                call(unmapped, b"kind", &[]),
                symbol(b"candidatePreparationFailed")
            );
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
            call(
                db,
                b"assert:attr:value:",
                &[unit, symbol(b"unitName"), symbol(b"forgedModule")],
            );
            let mismatch = call(codegen, b"prepareActivation:plan:", &[unit, publication()]);
            assert_eq!(
                call(mismatch, b"kind", &[]),
                symbol(b"candidatePreparationFailed")
            );
            call(
                db,
                b"assert:attr:value:",
                &[unit, symbol(b"unitName"), name],
            );
            let plan = publication();
            let token = call(codegen, b"prepareActivation:plan:", &[unit, plan]);
            assert_eq!(
                call(token, b"isError", &[]),
                no,
                "preparation: {:?}",
                bytes(call(codegen, b"lastError", &[]))
            );
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
            let duplicate = call(codegen, b"prepareActivation:plan:", &[unit, publication()]);
            assert_eq!(
                call(duplicate, b"kind", &[]),
                symbol(b"candidatePreparationFailed")
            );
            collect(id, named(b"GC"));
            assert_eq!(call(codegen, b"discardPrepared:", &[token]), unit);
            let consumed = call(codegen, b"discardPrepared:", &[token]);
            assert_eq!(
                call(consumed, b"kind", &[]),
                symbol(b"invalidPreparedToken")
            );
            let second = call(codegen, b"prepareActivation:plan:", &[unit, publication()]);
            assert!(value(second) > value(token));
            assert_eq!(call(codegen, b"discardPrepared:", &[second]), unit);
            let stale = publication();
            call(
                db,
                b"assert:attr:value:",
                &[unit, symbol(b"testDrift"), yes],
            );
            let rejected = call(codegen, b"prepareActivation:plan:", &[unit, stale]);
            assert_eq!(call(rejected, b"kind", &[]), symbol(b"staleGeneration"));
            // A callback may return the old basis and then mutate Store. The
            // final snapshot check must catch that mixed observation.
            let drifting_plan = publication();
            let db_behavior = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(db)],
            ));
            let basis_selector = symbol(b"basis");
            let prior_basis = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(basis_selector),
                    Operand::object(db_behavior),
                ],
            )
            .as_entry()
            .unwrap();
            let mut drift = Drift {
                id,
                original: prior_basis,
                db,
                entity: unit,
                attribute: symbol(b"callbackDrift"),
                value: integer(if synchronous { 2 } else { 1 }),
                fired: Cell::new(false),
            };
            install(
                id,
                db_behavior,
                b"basis",
                drift_basis,
                core::ptr::from_mut(&mut drift).cast(),
            );
            let rejected = call(codegen, b"prepareActivation:plan:", &[unit, drifting_plan]);
            send(
                id,
                db_behavior,
                b"addMethod",
                &[Operand::object(basis_selector), Operand::entry(prior_basis)],
            );
            assert!(drift.fired.get());
            assert_eq!(call(rejected, b"kind", &[]), symbol(b"staleGeneration"));
            let bad = call(named(b"Object"), b"clone", &[]);
            assert_eq!(
                call(
                    call(codegen, b"inspectCandidate:into:", &[path_object, bad]),
                    b"isError",
                    &[]
                ),
                yes
            );
            // A removal must find a direct entry, never merely an inherited
            // implementation or a Store mirror assertion.
            let removal_source = text(b"[Module named: #candidateFactory requires: #[]]; let candidateHelper := [| 22]; [Reflect removeMethod: #candidateAnswer on: Integer]");
            let removal_path = text(dir.join("remove.dylib").as_os_str().as_bytes());
            assert_eq!(
                call(
                    codegen,
                    b"compileCandidate:named:output:",
                    &[removal_source, name, removal_path]
                ),
                removal_path
            );
            let removal = call(named(b"Object"), b"clone", &[]);
            assert_eq!(
                call(codegen, b"inspectCandidate:into:", &[removal_path, removal]),
                removal
            );
            assert_eq!(call(codegen, b"mapCandidate:", &[removal]), removal);
            let removal_revision = call(
                db,
                b"currentValue:attr:",
                &[removal, symbol(b"sourceRevision")],
            );
            let removal_projection = call(
                codegen,
                b"successorGenerationProjection:replacing:with:unit:",
                &[at(projection, 1), name, removal_revision, removal],
            );
            let removal_next = call(named(b"Object"), b"clone", &[]);
            for (attr, v) in [
                (b"sourceMap".as_slice(), at(removal_projection, 1)),
                (b"baseDigest", at(removal_projection, 2)),
                (b"generationSelection", at(removal_projection, 3)),
                (b"predecessor", generation),
                (b"deltaUnit", removal),
            ] {
                call(db, b"assert:attr:value:", &[removal_next, symbol(attr), v]);
            }
            let event = call(named(b"Object"), b"clone", &[]);
            let removal_basis = call(db, b"basis", &[]);
            let mut removal_plan = call(named(b"Vector"), b"new", &[]);
            for value in [
                symbol(b"activate"),
                removal,
                name,
                nil,
                event,
                removal_basis,
                generation,
                removal_next,
            ] {
                removal_plan = call(removal_plan, b"add:", &[value]);
            }
            let rejected = call(
                codegen,
                b"prepareActivation:plan:",
                &[removal, removal_plan],
            );
            assert_eq!(call(rejected, b"kind", &[]), symbol(b"missingDirectMethod"));
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
            if synchronous {
                // Preparation cannot specialise a previously unowned singleton.
                let temporary = call(named(b"Object"), b"clone", &[]);
                let old_behavior = send(
                    id,
                    id.receiver,
                    b"behaviorOf:",
                    &[Operand::object(temporary)],
                )
                .as_object()
                .unwrap();
                call(globals, b"at:put:", &[symbol(b"Integer"), temporary]);
                let rejected = call(codegen, b"prepareActivation:plan:", &[unit, publication()]);
                assert_eq!(call(rejected, b"kind", &[]), symbol(b"staleGeneration"));
                assert_eq!(
                    send(
                        id,
                        id.receiver,
                        b"behaviorOf:",
                        &[Operand::object(temporary)]
                    )
                    .as_object(),
                    Some(old_behavior)
                );
                call(
                    db,
                    b"retract:attr:value:",
                    &[symbol(b"__globals__"), symbol(b"Integer"), temporary],
                );
                let readoption = call(codegen, b"adoptInitialGeneration:", &[generation]);
                assert_eq!(call(readoption, b"kind", &[]), symbol(b"staleGeneration"));
                assert_eq!(
                    call(db, b"currentValue:attr:", &[generation, symbol(b"state")]),
                    symbol(b"drifted")
                );
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
        for token in tokens.into_inner() {
            release(id, roots, token);
        }
    }
}
