//! Read-only generation drift and retained Methods through current Oop owners.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::size;
#[test]
#[ignore = "requires production Oop modules"]
fn ledger_validation_preserves_state_and_reports_current_agreement() {
    if std::env::var_os("BIRNAM_LEDGER_DRIFT_CASE").is_none() {
        for case in [
            "raw", "store", "source", "metadata", "save", "retry", "opaque",
        ] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "ledger_validation_preserves_state_and_reports_current_agreement",
                        "--nocapture"
                    ])
                    .env("BIRNAM_LEDGER_DRIFT_CASE", case)
                    .status()
                    .unwrap()
                    .success(),
                "{case}"
            );
        }
        return;
    }
    let host = support::bootstrap();
    let h = support::H::new(&host);
    h.lifecycle();
    let case = std::env::var("BIRNAM_LEDGER_DRIFT_CASE").unwrap();
    let cg = h.named(b"Codegen");
    let g = h.named(b"Generation");
    let db = h.named(b"db");
    let globals = h.named(b"Globals");
    let yes = h.named(b"true");
    let nil = h.named(b"nil");
    let body=b"let ledgerHelper := [| 17]; let ledgerProjected := [| 1]; [Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | [ledgerHelper value]]]; [Reflect defineMethod: #ledgerOverride on: Integer as: [:self | 1]]";
    h.eval(body);
    let mut source = b"[Module named: #ledgerFixture requires: #[]]; ".to_vec();
    source.extend_from_slice(body);
    source.extend_from_slice(b"; [Reflect removeMethod: #ledgerRemoved on: Integer]");
    let source = h.text(&source);
    assert_eq!(
        h.call(
            cg,
            b"recordModuleSource:named:",
            &[source, h.key(b"ledgerFixture")]
        ),
        source
    );
    h.call(
        globals,
        b"at:put:",
        &[h.key(b"LedgerIntegerAlias"), h.named(b"Integer")],
    );
    let override_body=b"let ledgerProjected := [| 2]; [Reflect defineMethod: #ledgerOverride on: LedgerIntegerAlias as: [:self | 2]]; let System := System; [Reflect defineMethod: #ledgerLexicalEmbed on: LedgerIntegerAlias as: [:self | [System embedFile: \"never-created.bin\"]]]";
    h.eval(override_body);
    let mut override_source =
        b"[Module named: #ledgerOverride requires: #[#ledgerFixture]]; ".to_vec();
    override_source.extend_from_slice(override_body);
    h.call(
        cg,
        b"recordModuleSource:named:",
        &[h.text(&override_source), h.key(b"ledgerOverride")],
    );
    let temp = std::env::temp_dir().join(format!("birnam-ledger-embed-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(temp.join("asset.bin"), b"\0\xffasset").unwrap();
    let embedded_body=b"let ledgerEmbeddedGlobal := [| [System embedFile: \"asset.bin\"]]; [Reflect defineMethod: #ledgerEmbedded on: Integer as: [:self | [System embedFile: \"asset.bin\"]]]";
    let file = temp.join("embedded.birnam");
    std::fs::write(&file, embedded_body).unwrap();
    let loaded = h.call(
        h.named(b"System"),
        b"load:",
        &[h.text(file.as_os_str().as_encoded_bytes())],
    );
    h.q.check_result(loaded).unwrap();
    std::fs::remove_dir_all(temp).unwrap();
    let mut embedded = b"[Module named: #ledgerEmbedded requires: #[]]; ".to_vec();
    embedded.extend_from_slice(embedded_body);
    h.call(
        cg,
        b"recordModuleSource:named:",
        &[h.text(&embedded), h.key(b"ledgerEmbedded")],
    );
    let projection = h.call(cg, b"initialGenerationProjection", &[]);
    let generation = h.record(b"DispatchGeneration");
    for (attr, value) in [
        (b"sourceMap".as_slice(), h.at(projection, 1)),
        (b"baseDigest", h.at(projection, 2)),
        (b"generationSelection", h.at(projection, 3)),
    ] {
        h.call(
            h.named(b"Reflect"),
            b"slot:in:put:",
            &[h.key(attr), generation, value],
        );
    }
    h.call(
        h.prototype(b"DispatchGeneration"),
        b"current:",
        &[generation],
    );
    assert_eq!(
        h.call(
            h.call(cg, b"beginPortableImageApplication", &[]),
            b"kind",
            &[]
        ),
        h.key(b"imageApplicationRequired")
    );
    let vt = h.named(b"IntegerVt");
    let selector = h.key(b"ledgerAnswer");
    // The captured Method is not retained by the test after restoration/adoption.
    let captured = host
        .send(h.named(b"Integer"), b"methodFor:", &[selector])
        .unwrap();
    host.send(db, b"addMethod:on:with:", &[selector, vt, captured])
        .unwrap();
    let prior = host.send(vt, b"lookup", &[selector]).unwrap();
    let pin = host.scope();
    pin.keep(prior).unwrap();
    let restore = || {
        host.send(vt, b"addMethod", &[selector, prior]).unwrap();
    };
    let native_fault = unsafe {
        host.id.send(
            h.named(b"Execution"),
            h.key(b"failNative:detail:"),
            &[size(1).unwrap(), size(91).unwrap()],
        )
    }
    .unwrap_err();
    h.keep(native_fault);
    let raw = h.method(b"reject", h.context(&[native_fault, size(0).unwrap()]));
    h.bind(vt, b"ledgerAnswer", raw);
    let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
    assert_eq!(
        h.call(rejected, b"kind", &[]),
        h.key(b"nonPortableGeneration")
    );
    restore();
    h.eval(b"[Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | 99]]");
    let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
    assert_eq!(
        h.call(rejected, b"kind", &[]),
        h.key(b"sourceBindingMismatch")
    );
    h.call(db, b"removeMethod:on:", &[selector, vt]);
    let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
    assert_eq!(
        h.call(rejected, b"kind", &[]),
        h.key(b"sourceBindingMismatch")
    );
    restore();
    h.eval(b"[Reflect defineMethod: #ledgerRemoved on: Integer as: [:self | 3]]");
    let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
    assert_eq!(
        h.call(rejected, b"kind", &[]),
        h.key(b"sourceBindingMismatch")
    );
    h.call(db, b"removeMethod:on:", &[h.key(b"ledgerRemoved"), vt]);
    h.call(globals, b"at:put:", &[h.key(b"LedgerIntegerAlias"), nil]);
    let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
    assert_eq!(
        h.call(rejected, b"kind", &[]),
        h.key(b"sourceBindingMismatch")
    );
    h.call(
        globals,
        b"at:put:",
        &[h.key(b"LedgerIntegerAlias"), h.named(b"Integer")],
    );
    let helper_name = h.key(b"ledgerHelper");
    let helper = h.call(globals, b"at:", &[helper_name]);
    let wrong = h.call(cg, b"eval:", &[h.text(b"[| 99]")]);
    for replacement in [wrong, h.integer(17), nil] {
        h.call(globals, b"at:put:", &[helper_name, replacement]);
        let rejected = h.call(g, b"adoptInitialGeneration:", &[generation]);
        assert_eq!(
            h.call(rejected, b"kind", &[]),
            h.key(b"sourceBindingMismatch")
        );
    }
    h.call(globals, b"at:put:", &[helper_name, helper]);
    drop(pin);
    for _ in 0..2 {
        assert_eq!(
            h.call(g, b"adoptInitialGeneration:", &[generation]),
            generation
        );
    }
    // Explicit invalidation prevents a Store mirror from masking ledger ownership.
    h.call(db, b"methodChanged:on:", &[selector, vt]);
    h.call(
        db,
        b"assert:attr:value:",
        &[h.key(b"unrelated"), h.key(b"note"), yes],
    );
    assert_eq!(h.call(g, b"reconcileCandidate:", &[generation]), generation);
    h.eval(b"[Reflect defineMethod: #unmanagedAnswer on: Integer as: [:self | 23]]");
    assert_eq!(h.call(g, b"reconcileCandidate:", &[generation]), generation);
    assert_eq!(
        h.eval(b"[Reflect removeMethod: #unmanagedAnswer on: Integer]"),
        h.named(b"Integer")
    );
    assert!(h.binding(vt, b"unmanagedAnswer").is_null());
    assert_eq!(h.call(g, b"reconcileCandidate:", &[generation]), generation);
    let path = std::env::temp_dir().join(format!("birnam-drift-image-{}", std::process::id()));
    std::fs::write(&path, b"previous image").unwrap();
    let filename = h.text(path.as_os_str().as_encoded_bytes());
    let gv = h.vt(h.prototype(b"DispatchGeneration"));
    let state_method = h.binding(gv, b"state:");
    let state_alias = h.key(b"ledgerOriginalState:");
    let error = h.call(
        h.named(b"Error"),
        b"kind:",
        &[h.key(b"injectedDriftFailure")],
    );
    let reject = h.context(&[
        h.gc,
        state_alias,
        h.key(b"drifted"),
        error,
        size(1).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        nil,
    ]);
    let recorded = h.call(
        h.named(b"Reflect"),
        b"slot:in:",
        &[h.key(b"state"), generation],
    );
    let cgvt = h.vt(cg);
    let save_name = b"compilePortableImageCode:bindings:selection:";
    let save_original = h.binding(cgvt, save_name);
    let save_alias = h.key(b"ledgerOriginalImageCode:bindings:selection:");
    let during = h.context(&[save_alias, host.roots, db, selector, vt, size(0).unwrap()]);
    match case.as_str() {
        "retry" => {
            h.bind(gv, b"ledgerOriginalState:", state_method);
            h.bind(gv, b"state:", h.method(b"fault", reject));
            h.call(db, b"removeMethod:on:", &[selector, vt]);
            let failed = h.call(g, b"reconcileCandidate:", &[generation]);
            assert_eq!(h.call(failed, b"kind", &[]), h.key(b"staleGeneration"));
            h.bind(gv, b"state:", state_method);
            h.call(gv, b"removeMethod:", &[state_alias]);
            assert_eq!(h.count(reject, 4), 1, "validation called the state setter");
            assert_eq!(h.count(reject, 5), 0);
            assert_eq!(
                h.call(
                    h.named(b"Reflect"),
                    b"slot:in:",
                    &[h.key(b"state"), generation]
                ),
                recorded
            );
        }
        "metadata" => {
            let registry = h.call(h.prototype(b"SourceRegistry"), b"sourceRegistry", &[]);
            let row = h.at(registry, 1);
            let name = h.at(row, 0);
            let record = h.at(row, 1);
            let replacement = h.vector(&(0..3).map(|i| h.at(record, i)).collect::<Vec<_>>());
            let source = h.call(cg, b"sourceTextOf:", &[record]);
            assert_eq!(
                h.call(cg, b"replaceCompiledSource:named:", &[replacement, name]),
                source
            );
            assert_eq!(
                h.call(cg, b"replaceCompiledSource:named:", &[record, name]),
                source
            );
        }
        "store" => {
            h.call(db, b"removeMethod:on:", &[selector, vt]);
        }
        "source" => {
            h.eval(b"[Reflect defineMethod: #ledgerAnswer on: Integer as: [:self | 19]]");
        }
        "opaque" => {
            h.bind(vt, b"ledgerAnswer", raw);
        }
        "save" => {
            h.bind(
                cgvt,
                b"ledgerOriginalImageCode:bindings:selection:",
                save_original,
            );
            h.bind(cgvt, save_name, h.method(b"duringSave", during));
        }
        _ => {
            host.send(vt, b"removeMethod:", &[selector]).unwrap();
        }
    }
    if case == "store" {
        assert_ne!(
            h.call(
                h.named(b"Reflect"),
                b"slot:in:",
                &[h.key(b"state"), generation]
            ),
            h.key(b"drifted")
        );
    }
    let saved = h.call(h.named(b"System"), b"saveImage:", &[filename]);
    if case == "save" {
        h.bind(cgvt, save_name, save_original);
        h.call(cgvt, b"removeMethod:", &[save_alias]);
        assert!(h.count(during, 5) > 0);
    }
    match case.as_str() {
        "opaque" => {
            assert_eq!(h.call(saved, b"kind", &[]), h.key(b"nonPortableGeneration"));
            assert_eq!(std::fs::read(&path).unwrap(), b"previous image");
        }
        "save" => {
            assert_eq!(saved, nil);
            assert_eq!(std::fs::read(&path).unwrap(), b"previous image");
        }
        _ => {
            assert_eq!(
                saved, yes,
                "edited live worlds can be snapshotted without certification"
            );
            assert_ne!(std::fs::read(&path).unwrap(), b"previous image");
        }
    }
    std::fs::remove_file(path).unwrap();
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(
        h.number(h.call(prior, b"invokeOn:", &[h.integer(2)])),
        17,
        "ledger lost its retained Method"
    );
    let drift = h.call(g, b"reconcileCandidate:", &[generation]);
    assert_eq!(h.call(drift, b"kind", &[]), h.key(b"staleGeneration"));
    assert_eq!(
        h.call(
            h.named(b"Reflect"),
            b"slot:in:",
            &[h.key(b"state"), generation]
        ),
        recorded
    );
    restore();
    assert_eq!(
        h.call(g, b"adoptInitialGeneration:", &[generation]),
        generation
    );
    let facade = h.text(include_bytes!("../../../modules/compiled-unit.birnam"));
    h.q.check_result(h.call(h.named(b"Lang"), b"parse:", &[facade]))
        .unwrap();
}
