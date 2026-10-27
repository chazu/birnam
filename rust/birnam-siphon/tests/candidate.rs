//! Inert candidate admission, owned bytes, factory captures and explicit targets.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::size;
#[test]
#[ignore = "requires production Oop modules"]
fn candidate_objects_are_inert_snapshot_bound_and_preserve_factory_captures_and_selection() {
    let host = support::bootstrap();
    let h = support::H::new(&host);
    h.lifecycle();
    let cg = h.named(b"Codegen");
    let g = h.named(b"Generation");
    let globals = h.named(b"Globals");
    let db = h.named(b"db");
    let yes = h.named(b"true");
    let no = h.named(b"false");
    let nil = h.named(b"nil");
    let name = h.key(b"candidateFactory");
    let ivt = h.named(b"IntegerVt");
    let baseline=h.text(b"[Module named: #candidateFactory requires: #[]]; [Reflect removeMethod: #candidateAnswer on: Integer]");
    let candidate=h.text(b"[Module named: #candidateFactory requires: #[]]; let candidateHelper := [| 22]; [Reflect defineMethod: #candidateAnswer on: Integer as: [:self | [candidateHelper value]]]");
    assert_eq!(
        h.call(cg, b"recordModuleSource:named:", &[baseline, name]),
        baseline
    );
    let projection = h.call(cg, b"initialGenerationProjection", &[]);
    let generation = h.record(b"DispatchGeneration");
    for (attr, value) in [
        (b"sourceMap".as_slice(), h.at(projection, 1)),
        (b"baseDigest", h.at(projection, 2)),
        (b"generationSelection", h.at(projection, 3)),
    ] {
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"slot:in:put:",
                &[h.key(attr), generation, value]
            ),
            generation
        );
    }
    h.call(
        h.prototype(b"DispatchGeneration"),
        b"current:",
        &[generation],
    );
    for _ in 0..2 {
        assert_eq!(
            h.call(g, b"adoptInitialGeneration:", &[generation]),
            generation
        );
    }
    let fault = unsafe {
        host.id.send(
            h.named(b"Execution"),
            h.key(b"failNative:detail:"),
            &[size(1).unwrap(), size(994).unwrap()],
        )
    }
    .unwrap_err();
    h.keep(fault);
    let dir = std::env::temp_dir().join(format!("birnam-candidate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for synchronous in [false, true] {
        let path = dir.join(if synchronous {
            "sync.dylib"
        } else {
            "resume.dylib"
        });
        let output = h.text(path.as_os_str().as_encoded_bytes());
        let compiled = if synchronous {
            h.call(
                g,
                b"compileCandidate:named:output:selection:",
                &[candidate, name, output, h.vector(&[])],
            )
        } else {
            h.call(
                g,
                b"compileCandidate:named:output:",
                &[candidate, name, output],
            )
        };
        assert_eq!(compiled, output);
        let artifact = std::fs::read(&path).unwrap();
        assert_eq!(&artifact[..8], b"BIRNAMC2");
        let unit = h.record(b"CompiledUnit");
        assert_eq!(h.call(g, b"inspectCandidate:into:", &[output, unit]), unit);
        assert_eq!(h.call(g, b"candidateMapped:", &[unit]), no);
        let publication = || h.vector(&[h.key(b"activate"), unit]);
        assert_eq!(h.call(globals, b"at:", &[h.key(b"candidateHelper")]), nil);
        let unmapped = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        assert_eq!(
            h.call(unmapped, b"kind", &[]),
            h.key(b"candidatePreparationFailed")
        );
        std::fs::write(&path, b"corrupt replacement").unwrap();
        let tvt = h.vt(h.named(b"TCC"));
        let calls = h.context(&[fault, size(0).unwrap()]);
        let deny = h.method(b"reject", calls);
        let prior: Vec<_> = [b"compile:".as_slice(), b"compileObject:"]
            .into_iter()
            .map(|n| {
                let m = h.binding(tvt, n);
                h.bind(tvt, n, deny);
                (n, m)
            })
            .collect();
        assert_eq!(h.call(g, b"mapCandidate:", &[unit]), unit);
        assert_eq!(h.call(g, b"candidateMapped:", &[unit]), yes);
        assert_eq!(h.call(globals, b"at:", &[h.key(b"candidateHelper")]), nil);
        assert!(h.binding(ivt, b"candidateAnswer").is_null());
        h.call(h.gc, b"collectPrecisely", &[]);
        assert_eq!(h.call(g, b"mapCandidate:", &[unit]), unit);
        assert_eq!(h.count(calls, 1), 0, "mapping must not compile source");
        for (n, m) in prior {
            h.bind(tvt, n, m);
        }
        h.call(
            h.named(b"Reflect"),
            b"slot:in:put:",
            &[h.key(b"unitName"), unit, h.key(b"forgedModule")],
        );
        let described = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        h.q.check_result(described).unwrap();
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"slot:in:",
                &[h.key(b"unitName"), unit]
            ),
            h.key(b"forgedModule")
        );
        h.call(g, b"discardPrepared:", &[described]);
        h.call(
            h.named(b"Reflect"),
            b"slot:in:put:",
            &[h.key(b"unitName"), unit, name],
        );
        let token = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        h.q.check_result(token).unwrap();
        assert_eq!(h.call(globals, b"at:", &[h.key(b"candidateHelper")]), nil);
        assert!(h.binding(ivt, b"candidateAnswer").is_null());
        let duplicate = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        assert_eq!(
            h.call(duplicate, b"kind", &[]),
            h.key(b"candidatePreparationFailed")
        );
        h.call(h.gc, b"collectPrecisely", &[]);
        assert_eq!(h.call(g, b"discardPrepared:", &[token]), unit);
        let consumed = h.call(g, b"discardPrepared:", &[token]);
        assert_eq!(
            h.call(consumed, b"kind", &[]),
            h.key(b"invalidPreparedToken")
        );
        let second = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        assert_ne!(second, token);
        assert_eq!(h.call(g, b"discardPrepared:", &[second]), unit);
        h.call(db, b"assert:attr:value:", &[unit, h.key(b"testDrift"), yes]);
        let unrelated = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        h.q.check_result(unrelated).unwrap();
        assert_eq!(h.call(g, b"discardPrepared:", &[unrelated]), unit);
        let dvt = h.vt(db);
        let basis = h.binding(dvt, b"basis");
        h.bind(dvt, b"basis", deny);
        let unrelated = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
        h.bind(dvt, b"basis", basis);
        h.q.check_result(unrelated).unwrap();
        assert_eq!(
            h.count(calls, 1),
            0,
            "Generation must not consult Store basis"
        );
        h.call(g, b"discardPrepared:", &[unrelated]);
        // Use valid bytes so this checks the target protocol, not file corruption.
        std::fs::write(&path, &artifact).unwrap();
        let bad = h.call(h.named(b"Object"), b"clone", &[]);
        let invalid = h.call(g, b"inspectCandidate:into:", &[output, bad]);
        assert_eq!(h.call(invalid, b"isError", &[]), yes);
        let removal_source=h.text(b"[Module named: #candidateFactory requires: #[]]; let candidateHelper := [| 22]; [Reflect removeMethod: #candidateAnswer on: Integer]");
        let remove_path = h.text(dir.join("remove.dylib").as_os_str().as_encoded_bytes());
        assert_eq!(
            h.call(
                g,
                b"compileCandidate:named:output:",
                &[removal_source, name, remove_path]
            ),
            remove_path
        );
        let removal = h.record(b"CompiledUnit");
        assert_eq!(
            h.call(g, b"inspectCandidate:into:", &[remove_path, removal]),
            removal
        );
        assert_eq!(h.call(g, b"mapCandidate:", &[removal]), removal);
        let rejected = h.call(
            g,
            b"prepareActivation:plan:",
            &[removal, h.vector(&[h.key(b"activate"), removal])],
        );
        assert_eq!(
            h.call(rejected, b"kind", &[]),
            h.key(b"missingDirectMethod")
        );
        // Execute the already mapped factory through the current public description.
        // No initializer writes a dispatch or Globals binding until activation.
        let info = h.call(g, b"candidateDataFor:", &[unit]);
        let factory = h.at(info, 6);
        let adapters = h.at(info, 7);
        let block = h.call(
            h.named(b"Block"),
            b"fromMethod:captures:",
            &[factory, h.vector(&[])],
        );
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        let values = h.call(machine, b"runBlock:", &[block]);
        let helper = h.at(values, 0);
        let implementation = h.at(values, 1);
        assert_eq!(h.number(h.call(helper, b"value", &[])), 22);
        assert_eq!(
            h.call(implementation, b"captureAt:", &[h.integer(0)]),
            helper
        );
        let method = h.call(h.at(adapters, 0), b"withImplementation:", &[implementation]);
        assert_eq!(h.number(h.call(method, b"invokeOn:", &[h.integer(1)])), 22);
        assert_eq!(h.call(globals, b"at:", &[h.key(b"candidateHelper")]), nil);
        assert_eq!(
            h.call(globals, b"at:", &[h.key(b"__candidate_values")]),
            nil
        );
        if synchronous {
            let temporary = h.call(h.named(b"Object"), b"clone", &[]);
            let before = h.vt(temporary);
            h.call(globals, b"at:put:", &[h.key(b"Integer"), temporary]);
            let rejected = h.call(g, b"prepareActivation:plan:", &[unit, publication()]);
            assert_eq!(
                h.call(rejected, b"kind", &[]),
                h.key(b"unstableMethodTarget")
            );
            assert_eq!(h.vt(temporary), before);
            h.call(globals, b"remove:", &[h.key(b"Integer")]);
            assert_eq!(
                h.call(g, b"adoptInitialGeneration:", &[generation]),
                generation
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
