# Lifecycle records own their state

2026-10-16. First implementation slice of the approved
[generation lifecycle simplification](generation-lifecycle-review.md), following
`80fe2afe`. Implemented and qualified; this report accompanies the implementation commit. No release or user installation.

## Implemented boundary

CompiledUnit, DispatchGeneration, EvolutionIntent and the operation descriptions
returned by `operations` are ordinary objects with named reference slots. Existing
accessors remain available. `Reflect slotsOf:`, `slot:in:` and `slot:in:put:`
discover, read and change their actual storage. Native Generation accesses those
fields through their ordinary named getter/setter messages; it does not decode another owner's
payload, capture a service method, or add a native metadata registry.

| Owner | Fields |
| --- | --- |
| CompiledUnit | unitName, generation, kind, state, requires, provides, sourceRevision, authoredSource, revisionDigest, artifactChecksum, expectedBaseDigest, importStatus, rollbackAvailable, composedInto, compositionIdentity |
| DispatchGeneration | sourceMap, baseDigest, generationSelection, predecessor, deltaUnit, kind, state, compositionIdentity, composedInto |
| EvolutionIntent | intentUnit, intentAction |
| CompiledOperation | targetName, selector, operation, priorPresent |

The current-generation reference and per-name numbering map live in authored local
Cells captured by their owner methods. `DispatchGeneration current:` provides
explicit replacement of that reference. `CompiledUnit generationNumbers` /
`generationNumbers:` exposes the numbering Map without incrementing it. It does not activate methods or certify
that a supplied description matches live code.

Raw clones copy field references. Changing a clone's field leaves the original
unchanged; aliases of one object see the same edits. Nil is an ordinary field value. `rollbackAvailable` defaults to false; explicitly
setting its field to nil remains visible as nil. `sourceRevision` reads the stored
revision without implicitly parsing source to repair a missing value.
Explicit Store facts, including facts under these same names, are independent data.
No migration step deletes or imports such facts.

## Machinery removed

- The write-only Generation running flag, `enterRunning`, its phase module and
  Host calls. Ordinary edits need no running-phase permission. The actual idle
  admission for explicit intents/commits and native retirement is retained.
- Implicit reconciliation from `CompiledUnit state`. It now reads recorded
  status. Explicit `Generation reconcileCandidate:` still validates managed code.
- Lifecycle cardinality declarations, automatic activation-event records,
  supersedes/event links, and fact-based numbering/current-generation state.
- Generation's Store basis checks, fact inverse journal operations and Store
  compensation banks. The remaining Globals reservation belongs to explicit
  binding rollback, not to lifecycle metadata.
- Recursive image retirement and broad `retireDurableEntity:` retractions.
  Image composition replaces the current root. Records that callers retain keep
  their fields and annotations; otherwise ordinary GC can reclaim them.

A retained record is descriptive: a saved `#active` status or rollback flag is
historical data, not a revived process-local capability. Use the current
generation and explicit Generation operations when asking about live authority.

Historical inspection now means retaining a record deliberately. This does not
create a new automatic history facility. A tool can explicitly describe selected
records into Store using Reflect and the ordinary Store API.

## Explicit coordination retained

A prepared plan has six fields:
`[action, unit, name, priorUnit, expectedGeneration, successorGeneration]`.
The native token retains the relevant lifecycle field observations in addition to
its existing binding/source observations. A changed relevant field rejects a stale
commit; unrelated fact assertions do not expire a preparation. This is value
observation, not a new watcher or a method-identity guard. It does not promise
detection of every transient field edit that restores the original value.

Field writes, the current-generation root and method/Globals writes participate in
the existing explicit inverse journal. Compensation restores a still-owned write
and preserves a differing callback edit. Code selected by an entered call and code
held by captured Methods/Blocks remain rooted independently of lifecycle records.
Removing implicit facts does not authorize unsafe native unloading.

The whole-program ledger, frozen source registration, global Id/Globals checks,
exactly-one dictionary-epoch rule, image composition/adoption and Host intent
fulfillment remain for the next coordination slice. This implementation does not
claim to have delivered the full optional change-set architecture. In particular,
`state` no longer triggers global drift, but an explicit reconciliation still can.

## Transition and bootstrap

Rebuild images containing the old fact-backed lifecycle implementation. There is
no compatibility facade, fact-to-slot repair or fallback to source analysis to
recover missing compiled metadata. The prepared-plan API deliberately changes
from eight fields to six. The generic image codec and artifact byte codec are
unchanged; new reference slots use the existing object graph codec.

No native object family, bootstrap module or Id primitive was added. The isolated
build still contains 38 native libraries, four source-bootstrap modules and 23
authored tools. Full Store-free bootstrap remains a separate acceptance target;
this slice removes direct lifecycle storage dependencies, not Alien recipes and
the other remaining consumers.

## Acceptance

Qualification must include:

- Same-name explicit facts, named reflection, aliases, clones and side-effect-free
  status reads; field access with Store/db namespace names removed.
- Candidate inspection/mapping, stale relevant fields, unrelated fact writes,
  activation with Store/db names absent, exact rollback, field/current-root failure
  compensation and preservation of callback facts.
- Candidate code retention and eventual native reclamation.
- Retained historical records and annotations through mapped/rebuilt images,
  repeated image resaves, failed live loads and captured code survival.
- Source-free compiled startup with parsing/compilation disabled, the language
  suite, and source-loading/generation regression suites.
- Lean laws for unchanged annotations/retained records during root replacement,
  pure status reads and unrelated facts; negative controls that scrub facts or
  records must fail. Existing TLA+ outcomes remain required.

The initial native test fixtures required moving their hand-built carriers from
facts to actual authored record protocols. Failure injection now targets the
current-generation field owner and Globals capacity release, rather than removed
Store lifecycle operations. Original replaced authored Methods must be rooted
across collecting callbacks in those fixtures. Host runtime tests run in separate
processes because native module initialization is process-local and single-use.

## Qualification results

The final isolated runtime is `/tmp/birnam-lifecycle-cleanup/delivery-prefix`.
The final implementation passes 826 language assertions, the real Host
activation/rollback test, parser/compiler-disabled startup, and the lifecycle
field/alias/clone/annotation/status tests. The latter also exercises current
arithmetic replacement with counter reentry, public numbering-map access, explicit
nil constructor arguments, and field access with Store/db names absent.

The new `lifecycle-record-image-test.py` passes mapped and rebuilt execution after
its writer source is deleted. It checks shared identities, independent clones,
retained descriptions, explicit same-name annotations, numbering-map persistence,
new counter allocation, and both accessor and Reflect mutation after restoration.
It is included in the generation group of `host-cli-test.sh`.

The preceding accessor build qualified the same field layout and native code with
candidate inspection/mapping/retirement, all seven ledger modes, stale relevant
field rejection, unrelated fact writes between prepare/commit, commit with Store/db
names removed, exact inverse Methods/Globals, and selective failure compensation.
Its Host image test passed mapped/rebuilt/resaved successor activation and rollback,
failed live-load capability invalidation, retained captured code, and retained
historical fields/annotations. Artifact CLI checks passed inert compilation,
removed sources/assets, nested globals, dependency order and preflight failures.

The earlier record-representation build passed the complete generic image matrix,
including twenty resaves with a fixed live set, callback preservation on failed
live loads and snapshot-consistency rejection. It also passed all language-suite
postchecks. These broader runs preceded the final accessor simplification and
counter/constructor corrections; the receipt distinguishes the builds rather than
claiming every matrix was repeated on the last prefix.

Lean checked 336 theorems and rejected 91 mutations, including two new negative
controls for deleting retained records or explicit annotations during retirement.
All 209 existing TLA+ outcomes passed. The additional mutation-only change was
checked with the Lean gate separately; TLA+ model/configuration hashes were
unchanged. These are abstract contracts, not a proof of arbitrary replacement
methods or of the entire native implementation.

Two migration defects were reproduced and corrected before delivery: counter
publication initially overwrote a reentrant update to another name, and a
constructor cloned from an existing description initially retained old predecessor/
delta values when passed nil. The fixes read the latest numbering Map before
publication and assign both explicit constructor arguments unconditionally. No
monkeypatch watcher or method-identity guard was introduced.

The [measurement receipt](../measurements/lifecycle-record-owners-20261016.json)
records source hashes, build scopes, commands and logs. No CPU speedup is claimed;
this slice establishes ownership, removes mandatory fact work and deletes obsolete
coordination/retirement machinery. Remaining global coordination restrictions are
listed above and in the main Store plan.
