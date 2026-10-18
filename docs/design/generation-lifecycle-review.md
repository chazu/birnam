# Generation lifecycle: retain ownership, reduce mandatory coordination

2026-10-16. Review of `80fe2afe`; recommendations, not an implemented lifecycle
redesign. The user subsequently approved this direction. Implementation status is
tracked in [lifecycle record owners](lifecycle-record-owners.md) and
[prepared operation admission](prepared-operation-admission.md) and
[scoped transactions](scoped-generation-transactions.md) and
[explicit image adoption](explicit-image-adoption.md) and
[opt-in baselines](opt-in-generation-baselines.md) and
[module-scoped candidates](module-scoped-candidates.md) and
[ordinary activation](ordinary-candidate-activation.md) and
[logical discard](logical-candidate-discard.md). Follows
[optional Store](optional-store-and-relational.md).

## Conclusion

Do not migrate the entire existing lifecycle from facts to slots unchanged.
It combines executable-code ownership, artifact admission, an exact batch-edit
transaction, whole-program source certification, automatic history, and image
reconstruction. Only the first two are inherently needed for ordinary execution.
Batch edits and undo are useful explicit facilities. A mandatory runtime-wide
generation is not a language invariant.

Ordinary method replacement already bypasses Generation and remains late-bound.
The excess is chiefly around explicit activation, startup, and persistence, not
a Generation check on every message send. Removing this excess is primarily a
design simplification; this review does not claim a measured CPU improvement.

## Implementation status

All implementation recommendations in this review are now completed and qualified.
The original findings/evidence below describe the reviewed baseline; current
contracts and receipts are in the linked implementation reports.
Completed: ordinary lifecycle owners; removal of implicit history and destructive
retirement; removal of dead phase state and status-triggered reconciliation;
value-based prepared admission; selected-state commit, compensation and rollback.

Final delivered slices:

1. Candidate operations now use one module source record and selected bindings;
   whole-program certification is separate and renewable. This slice is complete.
2. Explicit validation reports disagreement without lifecycle mutation or inverse
   revocation. This slice is complete.
3. Activation from entered code is qualified and Host-only intent fulfillment is
   removed. Physical native reclamation checkpoints remain. This slice is complete.
4. Logical discard, explicit artifact cleanup and opt-in weak diagnostics are
   separate. Discard no longer forces GC or creates a receipt. This slice is complete.

A [post-simplification review](generation-post-simplification-review.md) records
two reproduced API edge cases and an optional-certificate ownership follow-up.
Completion of the original sequence does not close those findings.

The following lifecycle and evidence describe the reviewed baseline; follow the
implementation reports above for the current behavior.

## Current lifecycle

1. Compile a candidate against the current whole-program source/selection digest.
2. Open and inspect immutable artifact bytes; map executable code explicitly.
3. Prepare a successor, snapshot binding inverses, and mint a one-use token.
4. Return an `EvolutionIntent`; Host fulfills it outside entered execution.
5. Construct Methods/Blocks once, reserve Store/Globals inverse capacity, apply
   writes, publish lifecycle facts, and install the successor ledger/registry.
6. Retain predecessor and exact inverse Methods/Globals for explicit rollback.
7. On managed drift, mark the entire ledger and its units drifted. On image load,
   normally mint composed replacement units and retire old lifecycle entities.
8. Discard separately deletes matching artifact files, changes lifecycle facts,
   releases candidate roots, and requests GC/native retirement.

The implementation is about 5,000 lines across the authored facade and native
Generation sources. Line count is scope context, not an estimate of removable code.

## Findings and disposition

| Priority | Mechanism | Recommendation and reason |
| --- | --- | --- |
| High | Whole-runtime managed ledger and frozen initial source registry | Make managed revision certification optional. A batch editing module A should not inherently depend on every tracked method of B remaining unchanged. Keep artifact/source provenance as data. |
| High | Store basis, global Globals basis, and global Id epoch on prepared tokens | Replace world-wide invalidation with the selected operation's actual dependency/write set. An unrelated database assertion currently invalidates preparation; private Block construction can also advance the global dictionary epoch. |
| High | Every transactional dictionary write must advance global epoch by exactly one | Remove this global side-effect restriction. An ordinary owner implementation may also log or update another dictionary. Validate the promised affected state if the caller requested a conditional batch; do not prescribe how many other edits a replacement method may perform. |
| High | Retirement retracts every fact on lifecycle carriers | Stop broad fact deletion. Explicit user annotations are not lifecycle-owned data. This is an existing ownership defect, independently of whether we simplify transactions. |
| Medium | `state` reads reconcile the complete ledger and publish drift to all units | Separate lifecycle status from explicit validation of whether a recorded change still matches live bindings. Reading historical status should not poison unrelated edit/undo capabilities. |
| Medium | Mandatory activation-event facts, Store capacity banks, fact inverse journals, history cleanup | Remove mandatory database participation. Use ordinary owned change records; fact history is opt-in. Most fact reservation/compensation disappears with those writes. Globals coordination remains a separate question. |
| Medium | Image composition mints replacement active units, copies metadata, walks ancestry, and retires facts | Default to restoring live code and descriptive provenance without adopting a managed whole-program ledger. Discard process-local transaction authority; explicit adoption can establish a new optional baseline. Retain executable reconstruction and format checks. |
| Medium | Host-only intent fulfillment for every candidate commit | Investigate removing it for ordinary batch installation. Active code already has independent lifetime ownership. This is not yet proven safe to remove from the existing multi-owner transaction. Keep reentry discipline and native reclamation checkpoints. |
| Low, certain | `State.running` / `enterRunning` phase flag | The flag is initialized and written, never read. Remove the dead phase state and review its Host entry plumbing; the entry point still performs idle/busy checks, so removing the public call changes admission behavior. |
| Low | Discard combines logical release, deleting artifact files, facts, collection, and telemetry | Separate releasing a candidate/undo record from optional file cleanup and optional diagnostics. Keep code mapped while any real owner still needs it. |

### Why whole-world drift is excessive

`candidate_ledger.rs::matches` compares all managed bindings and Globals, then
`reconcile` permanently records observed drift and `publish_drift` marks every
unit. Preparation and rollback both depend on this ledger. The existing contract
therefore deliberately rejects a batch even when the divergent method is outside
the batch's writes. That is faithful to the old whole-generation promise, but the
promise is much broader than ordinary malleable programming needs.

The source registry also seals registration; artifact activation checks the seal
before loading. This is a real restriction on later module introduction, not just
an inspection label. REPL/socket startup automatically initializes this regime.
Remove the global assumption as a coherent change: simply deleting the seal while
leaving whole-program digest/image assumptions intact is insufficient.

### Narrowing checks is not removing ownership

For explicit undo, retain the exact prior Method/Global values and their roots.
If an affected binding has since changed, conditional undo can report a conflict.
An unrelated binding or fact does not inherently create that conflict. A caller
wanting unconditional replacement can use ordinary owner messages.

The read set may be larger than the write set: target identity, dependencies,
projection inputs, and any shared Global that determines a target can matter.
Specify that read set before replacing global epochs. Do not add per-method
monkeypatch guards. If strict detection of intervening A-to-B-to-A edits is desired,
make it a documented conditional-operation contract with owner-local revisions;
exact restoration of current values alone does not prove that such history matters.

### Failure compensation is a separate product promise

The current transaction has meaningful selective compensation: it restores its
own still-current writes and preserves differing callback writes. It can already
answer `candidateCompensationFailed`; it does not guarantee recovery from arbitrary
replacement methods. Retaining that bounded guarantee for an explicitly selected
transaction is reasonable and is not itself policing ordinary edits.

Do not silently replace existing explicit activation/rollback with partial
application during a storage migration. A simpler default batch could instead
report exactly what applied on failure, with undo requested separately, but that
would be a deliberate API/contract decision. The user's previous partial-load
decision covered basic live Image loading, not all Generation transactions.

### What should remain

- Entered Methods, captured Blocks, suspended Machines and undo records retain
  the native code they need. Replacing a binding must not unmap entered code.
- GC roots across collecting sends, owner-thread rules and guards against native
  reentry into partially updated private state.
- Bounded decoding, ABI/descriptor checks and immutable inspected artifact bytes.
  Trusted native loading is not a hostile-code sandbox; checksums are not trust.
- Correct lookup invalidation and deoptimization for optimized code. Those guards
  preserve late binding and are distinct from policing user-level monkeypatches.
- Honest imageability checks and atomic image-file replacement. Restoration must
  preserve live methods and aliases without requiring source to match every edit.
- Exact inverse records and truthful failure/conflict results where an explicit
  API promises conditional undo or compensation.

The `retirementReady` condition is an Execution depth/drain condition. It has a
concrete role in reclaiming native code. Generation also uses it to admit an
activation intent, then allows commits while its authored policy runs under a
dynamic flag. That coupling deserves a focused experiment; it is not evidence
that all method installation requires an idle machine.

## Evidence and limitations

Source evidence:

- `modules/compiled-unit.birnam`: initialization/sealing, broad retirement,
  image composition, effectful `state`, event construction, intent facade.
- `candidate_preparation.rs::current_snapshot`: global Store/Globals/Id stamps
  plus exact selected binding observations, checked before and after callbacks.
- `candidate_transaction.rs::put_method`: exactly-one global epoch change;
  `commit`: reservations, full-ledger checks and conditional compensation.
- `candidate_ledger.rs`: full-program certification and sticky drift.
- `candidate_recovery.rs`: old capabilities suspended before image application,
  discarded after successful or failed application as appropriate.
- `candidate_phase.rs` and `lib.rs`: write-only running flag.
- `candidate_retirement.rs`: root release is independent of physical disposal.
- `birnam-execution/src/lib.rs`: entered-call roots and retirement depth/drain;
  `birnam-tcc/src/lib.rs`: native checkpoint discipline.

The isolated qualified runtime `/tmp/birnam-durable/final-prefix` was used for
review probes, not the user's installation:

1. Assert `#userAnnotation` on an object, call `CompiledUnit retireDurableEntity:`,
   then read the annotation. The original value was present and afterward absent.
   This reproduces the broad deletion helper; automatic image composition calls
   it for old units and generations. It is not a new end-to-end image-loss test.
2. Initialize the generation, replace `CompiledUnit dependencyChanges` with an
   ordinary method returning `#[]`, then explicitly validate. The replacement ran,
   validation returned `#staleGeneration`, and the generation became `#drifted`.
   This strict validation error is expected; it is not evidence that ordinary edits
   or ordinary image snapshots are forbidden.

Probe scripts/logs are in `/tmp/birnam-generation-review/`. The existing native
`generation_transaction` test explicitly covers an unrelated fact invalidating a
prepared token, exact rollback, and preservation of callback facts. The review
rerun passed (one native integration test, 56.88 seconds) with precise GC and the
isolated prefix. Log: `/tmp/birnam-generation-review/transaction.log`. Command:

```sh
mise exec -- env BIRNAM_GC_PRECISE=1 \
  BIRNAM_TEST_MODULE_DIRECTORY=/tmp/birnam-durable/final-prefix/lib \
  CARGO_TARGET_DIR=.cargo-target/native-message-tests CARGO_PROFILE_DEV_PANIC=unwind \
  cargo test --offline --locked -p birnam-siphon --test generation_transaction \
  -- --include-ignored --nocapture
```

Existing Lean `SharedInvocation` laws model a *single binding* and its epoch.
They justify protecting a conditional edit against a changed relevant binding;
they do not prove that every unrelated dictionary or Store edit must stale it.
Existing formal/native checks are contracts to reassess where policy is deliberately
changed, not evidence that all historical restrictions are language invariants.
No new formal proof of a scoped replacement transaction is claimed by this review.

## Recommended sequence

1. Agree the smaller contract before migrating the entire lifecycle: ordinary
   CompiledUnit metadata/code ownership; optional change set and undo; optional
   whole-program certification. Keep explicit rollback's existing guarantee until
   its replacement is implemented and qualified.
2. Remove dead phase state, fix broad fact retirement, and correct stale lifecycle
   docs. The guide currently incorrectly says ordinary running-world edits are
   rejected; Design 47 still requires the removed Store-arbitrated MOP.
3. Move unit metadata to ordinary fields and make event/history descriptions
   opt-in. Separate status inspection from explicit live comparison.
4. Replace whole-world preparation/admission with a defined change-set dependency
   scope. Test unrelated edits, relevant conflicts, target rebinding, reentry,
   entered/captured code, GC, and conditional undo. Prove the scoped contracts in
   Lean/TLA+ before replacing existing checks.
5. Remove default whole-world adoption/sealing and image composition policy after
   decoupling source-free mapped/rebuilt code restoration. Retain optional explicit
   baseline certification where tools actually need it.
6. Test ordinary activation within active code before removing Host intent
   fulfillment. Preserve physical native retirement at safe checkpoints.

Do not implement a new generic transaction framework or a permanent native
registry to replace these facts. The next storage slice should follow the reduced
responsibilities rather than fossilize the current lifecycle.
