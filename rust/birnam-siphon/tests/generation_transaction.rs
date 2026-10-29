//! Authored candidate preflight, partial application and explicit conditional undo.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{Oop, size};
use support::H;
fn fault(
    h: &H<'_>,
    vt: Oop,
    name: &[u8],
    alias: &[u8],
    key: Oop,
    error: Oop,
    after: bool,
    extra: bool,
) -> (Oop, Oop) {
    let original = h.binding(vt, name);
    h.bind(vt, alias, original);
    let state = h.context(&[
        h.gc,
        h.key(alias),
        key,
        error,
        size(1).unwrap(),
        size(0).unwrap(),
        size(usize::from(after)).unwrap(),
        size(usize::from(extra)).unwrap(),
        h.key(b"txCallback"),
    ]);
    h.bind(vt, name, h.method(b"fault", state));
    (original, state)
}
fn restore(h: &H<'_>, vt: Oop, name: &[u8], alias: &[u8], original: Oop) {
    h.bind(vt, name, original);
    h.call(vt, b"removeMethod:", &[h.key(alias)]);
}
#[test]
#[ignore = "requires production Oop modules"]
fn candidate_partial_edits_and_explicit_undo_preserve_current_owners() {
    let host = support::bootstrap();
    let h = H::new(&host);
    let retired_undo = h.key(b"restoreBinding:of:in:expected:previous:atEpoch:");
    assert!(
        h.call(host.id.root, b"bind:to:", &[retired_undo, host.id.root])
            .is_null()
    );
    h.lifecycle();
    assert!(
        h.call(host.id.root, b"bind:to:", &[retired_undo, host.id.root])
            .is_null(),
        "loading explicit coordination must not restore Id atomic undo"
    );
    let cg = h.named(b"Codegen");
    let generation = h.named(b"Generation");
    let globals = h.named(b"Globals");
    let nil = h.named(b"nil");
    let yes = h.named(b"true");
    let ivt = h.named(b"IntegerVt");
    let meta = h.vt(ivt);
    let direct = |name: &[u8]| h.binding(ivt, name);
    let before=h.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 10]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txRemove on: Integer as: [:self | 30]]");
    h.eval(b"let txHelper := [| 10]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txRemove on: Integer as: [:self | 30]]");
    h.call(
        cg,
        b"recordModuleSource:named:",
        &[before, h.key(b"txFixture")],
    );
    let helper = h.call(globals, b"at:", &[h.key(b"txHelper")]);
    let old_replace = direct(b"txReplace");
    let old_remove = direct(b"txRemove");
    let after=h.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 20]; let txNewHelper := [| 7]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txAdded on: Integer as: [:self | 40]]; [Reflect removeMethod: #txRemove on: Integer]");
    let dir = std::env::temp_dir().join(format!("birnam-candidate-partial-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for synchronous in [false, true] {
        let path = h.text(
            dir.join(if synchronous { "sync.o" } else { "resumable.o" })
                .as_os_str()
                .as_encoded_bytes(),
        );
        let compiled = if synchronous {
            h.call(
                generation,
                b"compileCandidate:named:output:selection:",
                &[after, h.key(b"txFixture"), path, h.vector(&[])],
            )
        } else {
            h.call(
                generation,
                b"compileCandidate:named:output:",
                &[after, h.key(b"txFixture"), path],
            )
        };
        assert_eq!(compiled, path);
        let unit = h.record(b"CompiledUnit");
        assert_eq!(
            h.call(generation, b"inspectCandidate:into:", &[path, unit]),
            unit
        );
        h.call(generation, b"mapCandidate:", &[unit]);
        let prepare = || {
            h.call(
                generation,
                b"prepareActivation:plan:",
                &[unit, h.vector(&[h.key(b"activate"), unit])],
            )
        };
        let token = prepare();
        h.call(globals, b"at:put:", &[h.key(b"txNewHelper"), nil]);
        assert_eq!(
            h.call(
                h.call(generation, b"commitPrepared:", &[token]),
                b"kind",
                &[]
            ),
            h.key(b"staleGeneration")
        );
        assert_eq!(direct(b"txReplace"), old_replace);
        h.call(globals, b"remove:", &[h.key(b"txNewHelper")]);
        let token = prepare();
        assert_eq!(h.call(generation, b"discardPrepared:", &[token]), unit);
        assert_eq!(
            h.call(
                h.call(generation, b"commitPrepared:", &[token]),
                b"kind",
                &[]
            ),
            h.key(b"invalidPreparedToken")
        );
        let error = h.call(
            h.named(b"Error"),
            b"kind:",
            &[h.key(b"setterReportedFailure")],
        );
        let (setter, state) = fault(
            &h,
            meta,
            b"addMethod:as:",
            b"txOriginalAdd:as:",
            h.key(b"txReplace"),
            error,
            true,
            true,
        );
        let token = prepare();
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"slot:in:put:",
                &[h.key(b"state"), unit, h.key(b"externalDescription")]
            ),
            unit
        );
        h.call(
            h.named(b"db"),
            b"assert:attr:value:",
            &[unit, h.key(b"state"), h.key(b"explicitUserFact")],
        );
        let failed = h.call(generation, b"commitPrepared:", &[token]);
        restore(&h, meta, b"addMethod:as:", b"txOriginalAdd:as:", setter);
        assert_eq!(failed, error);
        assert_eq!(h.count(state, 5), 1);
        assert_eq!(
            h.number(h.call(h.integer(2), b"txReplace", &[])),
            20,
            "failure must not run an automatic inverse"
        );
        assert_eq!(direct(b"txRemove"), old_remove);
        assert!(direct(b"txAdded").is_null());
        assert_eq!(h.call(unit, b"rollbackAvailable", &[]), yes);
        let installed = h.call(
            h.named(b"Execution"),
            b"directMethodFor:in:",
            &[h.key(b"txReplace"), ivt],
        );
        let outside = direct(b"txCallback");
        let epoch_before_intervening_edit = h.call(host.id.root, b"behaviorEpoch", &[]);
        h.eval(b"[Reflect defineMethod: #txReplace on: Integer as: [:self | 999]]");
        assert_eq!(
            h.call(h.call(unit, b"rollback", &[]), b"kind", &[]),
            h.key(b"undoConflict")
        );
        assert_eq!(h.number(h.call(h.integer(2), b"txReplace", &[])), 999);
        assert_eq!(
            h.number(h.call(
                h.call(globals, b"at:", &[h.key(b"txHelper")]),
                b"value",
                &[]
            )),
            20
        );
        h.call(ivt, b"addMethod:as:", &[installed, h.key(b"txReplace")]);
        assert_eq!(direct(b"txReplace"), installed);
        assert_ne!(
            h.call(host.id.root, b"behaviorEpoch", &[]),
            epoch_before_intervening_edit
        );
        // The selected contract checks the current binding, not its intervening
        // history: expected -> different -> expected permits explicit rollback.
        assert_eq!(
            h.call(unit, b"rollback", &[]),
            unit,
            "value-checked rollback permits restored expected bindings"
        );
        assert_eq!(
            h.call(unit, b"rollback", &[]),
            unit,
            "completed rollback is idempotent"
        );
        assert_eq!(direct(b"txReplace"), old_replace);
        assert_eq!(direct(b"txRemove"), old_remove);
        assert_eq!(
            direct(b"txCallback"),
            outside,
            "undo erased the setter's unrelated effect"
        );
        assert_eq!(h.call(globals, b"at:", &[h.key(b"txHelper")]), helper);
        assert_eq!(
            h.number(h.call(
                h.call(globals, b"bindingAt:", &[h.key(b"txNewHelper")]),
                b"size",
                &[]
            )),
            0
        );
        assert_eq!(
            h.call(
                h.named(b"db"),
                b"currentValue:attr:",
                &[unit, h.key(b"state")]
            ),
            h.key(b"explicitUserFact")
        );
        assert!(
            h.call(
                host.id.root,
                b"bind:to:",
                &[h.key(b"reserveBindingCompensationFor:count:"), globals]
            )
            .is_null()
        );
        assert_eq!(h.call(unit, b"activate", &[]), unit);
        assert_eq!(h.number(h.call(h.integer(2), b"txAdded", &[])), 40);
        assert!(direct(b"txRemove").is_null());
        let (removal, state) = fault(
            &h,
            meta,
            b"removeMethod:",
            b"txOriginalRemove:",
            h.key(b"txAdded"),
            error,
            true,
            false,
        );
        assert_eq!(h.call(unit, b"rollback", &[]), error);
        restore(&h, meta, b"removeMethod:", b"txOriginalRemove:", removal);
        assert_eq!(h.count(state, 5), 1);
        assert!(
            direct(b"txAdded").is_null(),
            "failed undo must not be automatically redone"
        );
        assert_eq!(direct(b"txRemove"), old_remove);
        assert_eq!(h.number(h.call(h.integer(2), b"txReplace", &[])), 20);
        assert_eq!(h.call(unit, b"rollback", &[]), unit);
        assert_eq!(direct(b"txReplace"), old_replace);
        assert_eq!(h.call(globals, b"at:", &[h.key(b"txHelper")]), helper);
        assert_eq!(h.call(unit, b"discard", &[]), unit);
        assert_eq!(h.call(generation, b"candidatePolicyCellFor:", &[unit]), nil);
        // The current Reflect constructor is live. Its failure must
        // not replay the initializer on a second activation attempt.
        let second = h.record(b"CompiledUnit");
        h.call(generation, b"inspectCandidate:into:", &[path, second]);
        h.call(generation, b"mapCandidate:", &[second]);
        let avt = h.vt(h.named(b"Reflect"));
        let (init, state) = fault(
            &h,
            avt,
            b"methodForArity:implementation:",
            b"txOriginalImplementation:",
            std::ptr::null_mut(),
            error,
            false,
            false,
        );
        assert_eq!(h.call(second, b"activate", &[]), error);
        assert_eq!(
            h.call(h.call(second, b"activate", &[]), b"kind", &[]),
            h.key(b"candidateConstructionFailed")
        );
        assert_eq!(h.count(state, 5), 1);
        restore(
            &h,
            avt,
            b"methodForArity:implementation:",
            b"txOriginalImplementation:",
            init,
        );
        h.call(second, b"discard", &[]);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
