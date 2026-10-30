# Generation

This owner holds candidate artifact/mapping capabilities, selected change records,
and optional source certificates. Source registration belongs to the authored
`SourceRegistry` owner in Globals. It does not define ordinary
message dispatch. Foreign owners are accessed through current Id messages; no
foreign payload layouts or new service table are used.

`CompiledUnit` is the authored facade. Candidate envelopes (ABI 5) bind exact
source/artifact/selection bytes to the previous revision of **that module**.
Compilation requires registered source, not an adopted whole-world baseline.
Identity, dependency and residual-initializer compatibility remain checked.
Inspection and mapping do not run the candidate factory; construction runs once
and is retained across retries.

Candidate preparation, construction sequencing, activation, undo, active queries
and discard policy are authored in `candidate-policy.birnam`. The native carrier
roots one opaque ordinary Cell per admitted artifact; native code never reads its
contents. The Cell is exposed through `candidatePolicyCellFor:`. Carrier data and
unit enumeration use `candidateDataFor:` / `candidateUnits`; `releaseCandidate:`
releases only carrier ownership. There is no native prepared-token or undo table.

The authored `prepareActivation:plan:` / `prepareRollback:plan:` messages accept
`[action, unit]` and return ordinary one-use preparation descriptors. Activation
records planned writes before outgoing sends. Failure retains partial effects and
explicit undo; it never attempts automatic compensation. Undo is conditional on
recorded owners/values, preserves intervening writes, and can be retried after a
partial failure. No-op setters and descriptive fields are not policed by postchecks.
Ordinary message dispatch is independent of all of these operations.

Whole-program certification is separate: `DispatchGeneration adoptCurrent`
projects registered source and calls `adoptInitialGeneration:`. Re-certification
replaces only that optional ledger. `reconcileCandidate:` / `validateImageGeneration`
report current agreement without lifecycle setters or inverse revocation. A
candidate commit does not manufacture a certificate or update a global generation.

Source records are ordinary objects retained by SourceRegistry. Generation reads
its public protocol through Id, with no backing table, slot-layout access or
owner cache. Prepared work and inverses retain their selected registry receiver.
Source replacement participates in the same explicit conditional undo as other
candidate writes; a rebound owner is never mistaken for that receiver.
Snapshot admission checks imageability at registered authored selectors, not
equality to old source bodies. Images restore the owner and its aliases directly.

Entered Methods, captured Blocks and suspended Machines retain code through
Execution. GC/TCC checkpoints govern physical reclamation. Logical candidate
ownership never authorizes forcibly unmapping their code.

Activation and rollback are ordinary sends. Host does not detect or fulfill a
special result type. Physical retirement checkpoints remain independent.

`discardCandidate:` releases the handle, preparation descriptors and explicit undo state,
including for an active unit, and returns the unit. It does not delete files,
collect, checkpoint or return a physical-disposal receipt. `removeCandidateFiles:`
is separate conditional cleanup while a handle is open. Optional
`watchCandidateRetirement:` records weak mapping identities for the existing
retained-count/bytes queries. Unwatched discards add no diagnostic history and
mapping does not depend on diagnostic callbacks. Detailed contracts/evidence:

- [module-scoped candidates](../../docs/archive/2026-10-runtime-refactors/module-scoped-candidates.md)
- [observational certification](../../docs/archive/2026-10-runtime-refactors/opt-in-generation-baselines.md)
- [image adoption](../../docs/archive/2026-10-runtime-refactors/explicit-image-adoption.md)
- [compilation record ownership](../../docs/archive/2026-10-runtime-refactors/retained-compilation-records.md)

See [ordinary activation](../../docs/archive/2026-10-runtime-refactors/ordinary-candidate-activation.md).

See [logical discard](../../docs/archive/2026-10-runtime-refactors/logical-candidate-discard.md).

## Image application

`beginImageApplication` invalidates candidate handles, prepared operations and
optional certification before image effects begin. It retains only an application
flag, which keeps candidate operations unavailable until cleanup.
`finishImageApplication` clears that flag; it never restores capabilities. Before
application, the same cleanup leaves existing candidate authority intact. Source
image staging belongs to Codegen's temporary image operation, not Generation.
Executable lifetime remains with Execution/TCC. All inter-module calls use Id.
