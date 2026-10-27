//! Discard, conditional file removal, weak mapping accounting and callable lifetime.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::size;
#[test]
#[ignore = "requires production Oop modules with TCC metrics"]
fn discard_preserves_replaced_files_and_live_methods_then_reclaims_mappings() {
    if std::env::var_os("BIRNAM_RETIREMENT_PHASE").is_none() {
        for phase in ["bootstrap", "running", "metadata"] {
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
    let phase = std::env::var("BIRNAM_RETIREMENT_PHASE").unwrap();
    let host = support::bootstrap();
    let h = support::H::new(&host);
    h.lifecycle();
    let cg = h.named(b"Codegen");
    let g = h.named(b"Generation");
    let nil = h.named(b"nil");
    let yes = h.named(b"true");
    let ivt = h.named(b"IntegerVt");
    let body = b"[Reflect defineMethod: #retireValue on: Integer as: [:self | 10]]";
    h.eval(body);
    let mut source = b"[Module named: #txFixture requires: #[]]; ".to_vec();
    source.extend_from_slice(body);
    h.call(
        cg,
        b"recordModuleSource:named:",
        &[h.text(&source), h.key(b"txFixture")],
    );
    let projection = h.call(cg, b"initialGenerationProjection", &[]);
    let generation = h.record(b"DispatchGeneration");
    for (attr, value) in [
        (b"sourceMap".as_slice(), h.at(projection, 1)),
        (b"baseDigest", h.at(projection, 2)),
        (b"generationSelection", h.at(projection, 3)),
        (b"predecessor", nil),
        (b"deltaUnit", nil),
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
    assert_eq!(
        h.call(g, b"adoptInitialGeneration:", &[generation]),
        generation
    );
    if phase == "running" {
        let fresh = h.record(b"CompiledUnit");
        let before = h.vt(fresh);
        let block = h.eval(b"[:self | 99]");
        let method = h.binding(ivt, b"retireValue");
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[h.key(b"newMethod"), fresh, block]
            ),
            fresh
        );
        assert_ne!(h.vt(fresh), before);
        assert_eq!(h.number(h.call(fresh, b"newMethod", &[])), 99);
        for source in [
            b"[Reflect defineMethod: #ordinaryRetire on: Integer as: [:self | 99]]".as_slice(),
            b"[Reflect removeMethod: #ordinaryRetire on: Integer]",
        ] {
            assert_eq!(h.eval(source), h.named(b"Integer"));
        }
        assert_eq!(h.call(cg, b"lastError", &[]), nil);
        let store = h.named(b"Store");
        assert_eq!(
            h.call(
                store,
                b"addMethod:on:with:",
                &[h.key(b"ordinaryRetire"), ivt, method]
            ),
            store
        );
        assert_eq!(
            h.call(store, b"removeMethod:on:", &[h.key(b"ordinaryRetire"), ivt]),
            store
        );
        assert_eq!(h.number(h.call(h.integer(1), b"retireValue", &[])), 10);
        assert_eq!(h.call(g, b"reconcileCandidate:", &[generation]), generation);
    }
    let dir = std::env::temp_dir().join(format!("birnam-retirement-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let projection_pin = host.scope();
    projection_pin.keep(projection).unwrap();
    for (index, mode) in ["unmapped", "inert", "resumable", "synchronous", "callback"]
        .into_iter()
        .enumerate()
    {
        let source=h.text(b"[Module named: #txFixture requires: #[]]; [Reflect defineMethod: #retireValue on: Integer as: [:self | 20]]");
        let path = dir.join(format!("{index}.o"));
        let sidecar = dir.join(format!("{index}.o.candidate"));
        let output = h.text(path.as_os_str().as_encoded_bytes());
        let compiled = if mode == "synchronous" {
            h.call(
                g,
                b"compileCandidate:named:output:selection:",
                &[source, h.key(b"txFixture"), output, h.vector(&[])],
            )
        } else {
            h.call(
                g,
                b"compileCandidate:named:output:",
                &[source, h.key(b"txFixture"), output],
            )
        };
        assert_eq!(compiled, output);
        let unit = h.record(b"CompiledUnit");
        assert_eq!(h.call(g, b"inspectCandidate:into:", &[output, unit]), unit);
        if mode != "unmapped" {
            assert_eq!(h.call(g, b"mapCandidate:", &[unit]), unit);
        }
        let artifact = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"new author contents").unwrap();
        let rejected = h.call(g, b"removeCandidateFiles:", &[unit]);
        assert_eq!(h.call(rejected, b"isError", &[]), yes);
        assert_eq!(std::fs::read(&path).unwrap(), b"new author contents");
        assert!(sidecar.exists());
        assert_ne!(h.call(unit, b"state", &[]), h.key(b"discarded"));
        std::fs::write(&path, &artifact).unwrap();
        let mut held = None;
        if index >= 2 {
            let token = h.call(
                g,
                b"prepareActivation:plan:",
                &[unit, h.vector(&[h.key(b"activate"), unit])],
            );
            h.q.check_result(token).unwrap();
            if phase == "metadata" {
                let name = h.key(b"txFixture");
                let text = h.call(cg, b"moduleSourceFor:", &[name]);
                let old = h.call(cg, b"compiledSourceFor:", &[text]);
                let metadata = h.call(cg, b"retainedSourceMetadataOf:", &[old]);
                let changed = h.call(cg, b"compiledSource:metadata:", &[text, metadata]);
                assert_ne!(old, changed);
                h.call(cg, b"replaceCompiledSource:named:", &[changed, name]);
                let rejected = h.call(g, b"commitPrepared:", &[token]);
                assert_eq!(h.call(rejected, b"kind", &[]), h.key(b"staleGeneration"));
                assert_eq!(h.number(h.call(h.integer(2), b"retireValue", &[])), 10);
                assert!(path.exists());
                std::fs::remove_dir_all(dir).unwrap();
                return;
            }
            assert!(path.exists() && sidecar.exists());
            if mode == "callback" {
                let meta = h.vt(ivt);
                let original = h.binding(meta, b"addMethod:as:");
                let alias = h.key(b"retirementOriginalAdd:as:");
                h.bind(meta, b"retirementOriginalAdd:as:", original);
                let probe = h.context(&[alias, size(1).unwrap(), size(0).unwrap()]);
                h.bind(meta, b"addMethod:as:", h.method(b"edit", probe));
                let committed = h.call(g, b"commitPrepared:", &[token]);
                h.bind(meta, b"addMethod:as:", original);
                h.call(meta, b"removeMethod:", &[alias]);
                assert_eq!(h.count(probe, 1), 0);
                assert_eq!(h.count(probe, 2), 0);
                assert_eq!(committed, unit);
                assert_eq!(h.call(unit, b"state", &[]), h.key(b"active"));
                assert_eq!(h.number(h.call(h.integer(1), b"retireValue", &[])), 20);
            } else {
                assert_eq!(h.call(g, b"commitPrepared:", &[token]), unit);
            }
            let method = h.binding(ivt, b"retireValue");
            let pin = host.scope();
            pin.keep(method).unwrap();
            held = Some((method, pin));
            let rollback = h.call(
                g,
                b"prepareRollback:plan:",
                &[unit, h.vector(&[h.key(b"rollback"), unit])],
            );
            assert_eq!(h.call(g, b"commitPreparedRollback:", &[rollback]), unit);
            assert_eq!(h.number(h.call(h.integer(1), b"retireValue", &[])), 10);
        }
        assert_eq!(h.call(g, b"watchCandidateRetirement:", &[unit]), unit);
        if mode != "inert" {
            assert_eq!(h.call(g, b"removeCandidateFiles:", &[unit]), unit);
            assert!(!path.exists() && !sidecar.exists());
        }
        h.clear();
        assert_eq!(h.call(g, b"discardCandidate:", &[unit]), unit);
        assert_eq!(h.call(unit, b"state", &[]), h.key(b"discarded"));
        assert_eq!(h.call(unit, b"rollbackAvailable", &[]), h.named(b"false"));
        if mode == "inert" {
            assert!(
                path.exists() && sidecar.exists(),
                "logical discard deleted files"
            );
        }
        h.call(h.gc, b"collectPrecisely", &[]);
        h.call(h.named(b"TCC"), b"checkpoint", &[]);
        if let Some((method, pin)) = held {
            assert_eq!(h.number(h.call(g, b"retainedGenerationCount", &[])), 1);
            assert!(h.number(h.call(g, b"retainedGenerationBytes", &[])) > 0);
            assert_eq!(h.number(h.call(method, b"invokeOn:", &[h.integer(1)])), 20);
            drop(pin);
        }
        let rejected = h.call(g, b"discardCandidate:", &[unit]);
        assert_eq!(h.call(rejected, b"kind", &[]), h.key(b"invalidCandidate"));
        h.clear();
        h.call(h.gc, b"collectPrecisely", &[]);
        h.call(h.named(b"TCC"), b"checkpoint", &[]);
        assert_eq!(
            h.number(h.call(g, b"retainedGenerationCount", &[])),
            0,
            "{mode}"
        );
        assert_eq!(h.number(h.call(g, b"retainedGenerationBytes", &[])), 0);
    }
    h.clear();
    drop(projection_pin);
    std::fs::remove_dir_all(dir).unwrap();
}
