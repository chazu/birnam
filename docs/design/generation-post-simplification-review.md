# Generation after lifecycle simplification

2026-10-17. Review of `c4bf4451`. The original simplification sequence is
implemented and qualified. The findings below describe that baseline; resolution
status is tracked separately here.

## Resolution status

1. Dependency reporting now compares against the module's current registered
   source through ordinary Codegen sends. Missing registration reports an Error.
   The helper remains descriptive and malleable. The production-prefix Host
   regression passes without a certificate, with a stale certificate, before/after
   activation and rollback, with edited dependency fields and missing registration.
   The same regression fails against the reviewed baseline. Logs:
   `/tmp/birnam-generation-followups/dependencies-{before,after}.log`.
   It is included in the Host generation acceptance group. The full language
   suite on that build also passes all 826 assertions.
2. Undo no longer writes lifecycle fields on released predecessor handles.
   Preparation omits an already-released predecessor; commit drops only those
   predecessor-field observations if it was released after preparation. Saved
   Methods/source and selected binding checks remain. Native private-state
   exclusion keeps handle ownership stable during callbacks. Qualified by four
   Host scenarios (including A/B/C and release between prepare/commit), native
   transaction and retirement tests, and 62 CLI checks. The same Host regression
   fails on the pre-fix runtime. Logs are under
   `/tmp/birnam-generation-followups/predecessor-{before,after,native,cli}.log`.
   Lean checks 371 theorems and rejects 112 mutations; TLC reports 244 expected
   outcomes. Receipt: `.cargo-target/formal/results/rooting-gmxr65x8/summary.json`.
3. `DispatchGeneration releaseCurrent` explicitly drops the native certificate
   observation and clears the authored current reference. It is idempotent and
   leaves retained descriptions, candidate undo and executable ownership intact.
   Neither release nor the ordinary current setter triggers collection. A replaced
   setter's effects are ordinary message passing, not an atomic transaction.
   The production Host test confirms that the stale certificate alone retains a
   watched candidate mapping even after `current: nil`; release plus GC/checkpoint
   reduces the watched count and bytes to zero. Live dispatch, another candidate's
   rollback, the retained source map, idempotent release and fresh adoption pass.
   Final qualification passes: 826 language assertions, the full Host generation
   group (including mapped/rebuilt/resaved/divergent images and console/TCP), 62
   compiled-unit CLI checks, native ledger/transaction/retirement suites, and
   startup with parsing/compilation disabled. Lean checks 375 theorems, rejects
   115 mutations, and TLC obtains 249 expected outcomes. The abstract proofs
   supplement the production native lifetime tests.

All three findings are resolved. Dependency reporting is committed in `f735e4a9`
and predecessor handling in `0652da35`; certificate release and this final receipt
form the third slice. The qualified prefix is
`/tmp/birnam-generation-followups/release`. See the
[qualification receipt](../measurements/generation-review-resolutions-20261017.json).
No user installation, release or push accompanies these fixes.

## Assessment

The resulting boundaries fit the language: ordinary sends edit live code;
candidate transactions provide explicitly requested conditional undo; source
certification is optional; Execution/TCC own physical code lifetime. Nothing in
these findings calls for global monkeypatch policing or restoring Host intents.

The two reproduced API inconsistencies now have fixes, and optional certification
has an explicit release path. See the resolution status and qualification above.
These are bounded follow-ups, not another lifecycle redesign. Independent small
module ports need not wait for broader image/resource work.

## Original findings at `c4bf4451`

### 1. Dependency reporting still assumes a certificate

`CompiledUnit dependencyChanges` reads `DispatchGeneration current` to obtain
the old dependencies. With the normal nil certificate, it treats the old list as
empty. A module whose baseline and candidate both require `#stdlib` reports:

```text
#[#certificate nil]
#[#firstDependencies {#added -> #[#stdlib], #removed -> #[]}]
```

This was reproduced against the final qualified runtime with an ordinarily
registered baseline and a compiled/opened candidate. Candidate admission itself
still compares module dependencies correctly; the defect is in descriptive output.
A stale certificate can likewise make the report describe an obsolete baseline.

Original recommendation: make the comparison basis explicit and module-local, using retained
source metadata rather than optional certification. Decide whether this helper
still earns its place when accepted candidates must preserve dependencies. Test
no certificate, a stale certificate, and before/after activation. Do not add a
certificate requirement to ordinary candidate work.

### 2. Undo rewrites a discarded predecessor's status

Reproduced sequence, with all operations using the default implementation:

1. Register module version 1; compile, map and activate candidate A (version 2).
2. Compile, map and activate candidate B (version 3) for the same module.
3. Discard the superseded A handle. Its state is `#discarded`, undo false.
4. Roll back B.

The selected Method correctly returns version 2. However, A's state becomes
`#active` again, while its undo flag remains false, `activeFor:` returns nil and
`A rollback` reports `#rollbackUnavailable`. The native handle was not recreated.

The cause is `candidate_transaction.rs::publication`: rollback writes `#active`
to the saved predecessor object unconditionally, even after that handle was
released. `plan_fields` accepts its current discarded status as the observation.

Original recommendation: keep restoration of saved Methods/source independent of whether the
predecessor's handle still exists. A released handle should remain released in
the default lifecycle description. Qualify A/B and A/B/C chains, discard between
prepare and commit, and release of all handles while restored code remains live.
Do not forbid discarding a predecessor or require keeping a native handle just
to restore code that B's inverse already owns.

### 3. Optional certification has acquisition and refresh, but no release

Source inspection finds a native `Ledger` that strongly roots the certificate,
its Methods, targets and Globals. A successful adoption replaces it; Image
recovery releases it. `DispatchGeneration current: nil` only changes the authored
Cell and does not clear that ledger. There is no dedicated release operation.

Consequently an explicitly adopted, subsequently stale certificate can retain
old code/data after the caller stops using it. This is an ownership/API gap;
this review has not measured its retained byte cost or run a reclamation probe.
It does not affect default worlds that never adopt a certificate.

Original recommendation: provide explicit release of certification's native observation roots
while preserving caller-retained descriptive records, installed code and candidate
undo. Test adopt, edit, release, GC/checkpoint, continued dispatch and fresh
adoption. Avoid a hidden mandatory observer or automatic drift-triggered revocation.

### 4. Documentation retained superseded behavior

The live guide still described returning an intent to the REPL for quiescent
fulfillment, and used a runtime-wide-generation heading. The plan status still
called narrower coordination pending, and the review labeled completed slices
"Still open". These are corrected in this review commit. Historical reports
remain historical; the authoritative plan links to this follow-up list.

## Retained boundaries and next migration

- Conditional compensation can fail if callbacks prevent restoration. Replaced
  metadata setters can leave effects when discard fails. These are documented
  limits of explicit operations, not reasons to guard ordinary sends.
- Live image loading can partially apply on failure. Save before loading when
  recovery is wanted; process-local candidate/undo capabilities do not survive
  loading. Do not quietly reintroduce automatic rollback during owner migration.
- Native lifetime evidence must remain distinct from abstract proofs. Before
  moving these owners into Birnam, exercise repeated activation/undo/discard with
  GC and suspended/resumed execution as well as entered and captured code. The
  existing qualification demonstrates entered/captured ownership; this review
  does not claim a new combined suspended-Machine lifecycle stress test.
- ABI-4 candidates and images/facades carrying removed lifecycle contracts need
  rebuilding. No backwards-compatibility adapter is required. This work has not
  been released, pushed or installed into the user's normal prefix.
- Source-registry ownership, Alien recipes, Image staging and task/worker/I/O
  ownership remain in the Store plan. Completing generation simplification does
  not establish Store/Relational-absent cold bootstrap or complete Rust migration.

Next, migrate the source registry as an ordinary discoverable owner. Continue
owner-by-owner; preserve source-free mapped startup and explicit rebuild, and
keep the final Store-absent acceptance path separate from individual module tests.

## Original review evidence and limits

The worktree was clean at review start; all requested implementation changes
were already committed through `c4bf4451`. No runtime implementation changes were
made by this review. The prior qualification receipt is
[logical candidate discard](../measurements/logical-candidate-discard-20261017.json).
Its 826 language assertions, 62 CLI checks, 368 Lean theorems, 110 mutation controls
and 241 expected TLC outcomes are prior qualification, not tests rerun here.

This review read the current transaction, preparation, retirement, recovery,
certificate and registry owners and ran a fresh default-language probe against
`/tmp/birnam-logical-discard/final-prefix`. Probe source, two candidate bundles and
output are in `/tmp/birnam-generation-followup-review/` (temporary local evidence).
The process exited successfully, reproducing findings 1 and 2. Its final output:

```text
#[#afterDiscard #discarded false]
#[#undoSecond anObject]
#[#afterUndo #active false nil 2]
#[#firstUndo error(#rollbackUnavailable: candidate has no retained inverse)]
```

No native-lifetime failure or dispatch error was observed in that probe. This is
a focused review, not a proof that every remaining lifecycle interleaving is safe.
