# Generation publication over Id

`Generation` is a separately loaded native module implemented in Rust. It owns
candidate admission and inert mappings, preparation tokens, the managed binding
ledger, commit and exact rollback, compensation, retirement,
and image recovery. It also owns the source registry and its staged image
exchange so publication updates the registry and ledger together without a
callback between the two updates.

The constructor receives the completed Namespace. Objects, canonical bytes and
ordinary Vectors cross the interface; compiler state and Rust IR never do.
Codegen supplies canonical source metadata through `compiledSourceMetadataFor:`
and candidate factory inventories through `candidateInventory:selection:`.
Generation uses NativeBackend for C/descriptor emission, TCC for owned native
mappings, Execution for executable references and authored ordinary objects for durable records.
Compiled metadata reads reject missing/malformed metadata and never parse source
as a fallback. Explicit candidate compilation and inspection may ask Codegen to
analyze source.

The publication messages retain their existing selectors: candidate compilation,
inspection/mapping, successor projection, prepare/commit, rollback, adoption,
reconciliation, retirement and telemetry. Production callers now send these
messages to Generation; Codegen has no publication aliases. The unused running-phase handshake has been removed. CompiledUnit,
DispatchGeneration and EvolutionIntent continue to own authored policy.

`sourceRegistry` returns `[#sourceRegistryV1, [name, compilationRecord], ...]`.
There is no registration-sealing API or flag. The old Boolean-header shape is
rejected; regenerate images carrying that schema. Registration uses `recordCompiledSource:named:` and
`moduleSourceFor:`. Image integration uses `prepareSourceRegistry:`,
`swapSourceRegistry`, `beginImageRecovery:`, `recoverImageGeneration:` and
`cancelImageRecovery`. The retained exchange stage supports exact compensation.
All transient candidates, tokens, ledgers, source registries and receipt objects
are visited through the module's registered roots. Creating-thread execution and
existing busy/idle admission checks remain enforced.

See the [decomposition roadmap](../../docs/design/native-module-decomposition-roadmap.md)
and [compiled-unit lifecycle](../../docs/design/47-compiled-unit-lifecycle.md).

## Publication behavior

The pure generation algebra in Codegen retains the frozen dispatch-base digest,
selection composition/replacement, dependency ordering and residual-initializer
checks. Candidates compile to checked object bundles with source and selection
metadata. Inspection retains an immutable byte snapshot; mapping relocates that
snapshot without recompiling or publishing methods and Globals. The native
`candidate` test exercises both synchronous and resumable selections, captured
helpers, collection and author-file changes after inspection.

`prepareActivation:plan:` copies the publication plan, checks the full successor
projection and stable target ownership, and roots exact direct bindings and
projected Globals. Preparation runs no candidate factory. Tokens retain relevant
values and the source revision, not global dictionary/Globals counters. Current
binding-owner stamps bound each validation pass across callbacks; monotonic tokens
are consumed by `discardPrepared:`, releasing preparation-only roots.

`adoptInitialGeneration:` records a canonical initial source/selection snapshot
and live portable bindings once. Adoption rejects unavailable targets, missing
or unexpected bindings, and portable Method bodies that differ from the final
source definition in dependency order, including aliases of one dictionary.
Comparison preserves lexical captures and the literal compile-time embedding
rewrite without reopening assets. Capture-free projected global Blocks must also
match the final source definition; missing, non-Block, captured or different-body
values cannot establish the initial ledger. Dependency overrides are compared
against the last definition. The transient ledger retains these Globals and
displaced Methods for inverse operations. Preparation checks its selected
observations and source lineage; `reconcileCandidate:` checks the complete ledger
and rejects drift without adopting the new binding. Unrelated Store facts remain outside that ledger. Repeating initial
adoption validates the current values against the recorded baseline. Disagreement
has no lifecycle side effects, and restoring the same bindings can restore agreement.

`commitPrepared:` and `commitPreparedRollback:` consume their token on every
attempt and require Execution's quiescent native boundary. Candidate construction
runs once and retains the exact helpers and Methods across publication retries.
Tracked method changes, projected Globals and owned lifecycle fields use inverse journals;
Globals reservations cover binding publication, compensation and rollback. Lifecycle
fields require no Store capacity reservations, automatic audit facts or event records. A failure
after mutation restores candidate-owned changes while retaining unrelated callback
facts and differing field edits. Successful publication switches the source registry and complete binding
ledger together. The `generation_transaction` test covers synchronous/resumable
activation, replacement/addition/removal, failure compensation and exact rollback.

`fulfillEvolutionIntent:` admits an EvolutionIntent only at Execution's idle
boundary, then scopes permission for its authored `fulfill` policy to commit.
Running Machines and nested fulfillment cannot enter that scope; native failures
and returned Errors always close it. Preparation Errors flow unchanged through
the commit selectors. Constructing branch Blocks or making untracked edits between
preparation and commitment does not expire the token.

Installed Host and published-project console tests exercise authored activation
and exact rollback. Image stages suspend all old candidate capabilities before
publication and retain their roots until the application outcome is known.
`recoverImageGeneration:` verifies the admitted image identity and releases old
capabilities. It does not invoke authored adoption policy, replace descriptive
records or create a new ledger. Execution independently owns restored Methods.
Basic live Image application failure abandons old capabilities instead of
restoring them over a partial world. Cancellation before application can restore
the suspended capabilities without changing the prior world.

`discardCandidate:` rejects active, prepared and rollback-reachable candidates,
removes only the admitted author-file contents, and publishes the discarded state
before releasing native roots. A process-local `DiscardReceipt` separates logical
discard from physical disposition. Live Methods, Blocks and Machines retain their
Units through Execution; GC and TCC reclaim them at an idle checkpoint. Weak TCC
mapping identities drive `retainedGenerationCount` and `retainedGenerationBytes`
without themselves keeping code alive. File removal is retryable but is outside
Store compensation: a failed discard can leave one or both author files removed.

Generation adoption installs no Store observer. Explicit transactions send their
Method writes to the current vtable owner, preserve returned Errors/native failures,
and inspect the resulting binding. They do not count dictionary mutations or
require a particular non-error return value. A setter may perform other edits;
conditional compensation preserves those that differ from the batch's writes.
A setter that leaves a different binding in place cannot claim success. Selected
postcondition checks, selective compensation and quiescent commit checks remain. Ordinary edits are detected when
a consumer explicitly reconciles its revision.

Prepared observations compare affected target/behavior identities, exact entries,
projected Globals (including absent versus present nil), lifecycle fields and the
source revision. Restoring the same binding/Global values between checks is not
itself a conflict merely because the global counters advanced. Source-registry
ABA still changes its explicit content revision. Activation/rollback compare only
their selected bindings/Globals and lifecycle fields, including after callbacks
and compensation. Edits to another managed module do not cause a conflict or get
implicitly certified. Scoped conflicts do not publish whole-generation drift.
The existing adopted source lineage remains an explicit candidate-tool restriction.
Ordinary startup and module loading do not adopt or seal it.
See [scoped transactions](../../docs/design/scoped-generation-transactions.md).

`validateImageGeneration` stays strict for callers asking whether the managed
revision is current. Image uses `prepareImageSnapshot` to check operation exclusion
and the imageability of current entries at registered authored selectors. It does
not compare bodies with source, reconcile a ledger or modify lifecycle fields.
Opaque native replacements are rejected even after loading without a baseline.
Image's consistency stamp still rejects detected dictionary/Globals/fact-state
changes during serialization without replacing the destination.

Every loaded image starts without a transient ledger or activation capabilities.
Saved generation records remain descriptive and alias-preserving. Explicit
`DispatchGeneration adoptCurrent` creates a predecessor-free source baseline and
validates live code using the existing strict adoption contract. It does not mint
component units; failed adoption restores the prior current root. Ordinary startup does not adopt; explicit validation no longer publishes sticky drift.
This does not make Store or Generation optional in the full image/bootstrap path.
See [explicit image adoption](../../docs/design/explicit-image-adoption.md).

The native `materialization` test also retains a reflected Method after its source
Block is released, transfers ownership to an alias binding on another Block,
and removes that final binding. Source, synchronous and resumable cases each run
three cycles; both live TCC Unit counts and relocated native bytes return to the
receiver baseline, then to zero after releasing the receiver. This replaces
`scripts/method-unit-lifetime-smoke.sh` and requires TCC resource metrics, enabled
by `scripts/native-message-test.sh`.

Image source staging now retains an owner revision. `swapSourceRegistry` publishes
only at its captured revision; `rollbackSourceRegistry` restores only its own still
current publication. A later source record or transaction replacement is preserved.
`abandonImageRecovery` discards suspended/current preparation and adoption authority
after failed image application; it does not undo the live world's changes. Entered
Methods retain their code independently through Execution.

## Retained compilation metadata

Registry entries now retain ordinary compilation record values instead of relying
on Store metadata facts attached to source Strings. A source-map revision carries
its own record at index 7, while index 3 retains the original String identity.
`compilationForSource:` is an identity lookup; `replaceCompiledSource:named:` changes
an existing registration explicitly. Replacement creates no method edits or facts.

Content revisions cover record changes and invalidate stale ledgers/prepared tokens,
including A-to-B-to-A changes. Successful activation
and rollback install the source records and the ledger's matching content revision
together, after all callback-capable validation. Explicit rollback still retains the
exact prior record references. See [the owner contract](../../docs/design/retained-compilation-records.md).

## Owned lifecycle records

`CompiledUnit`, `DispatchGeneration`, `EvolutionIntent` and `CompiledOperation`
use ordinary reference slots with named `slotNames` descriptions. Reflect can
inspect and edit those fields; explicit same-name Store facts are independent. Native coordination uses
the current named field accessors, avoiding operational slot-inventory scans.
The current-generation reference and numbering map are authored local Cells.
`state` reads the recorded status without reconciling the whole ledger; explicit
`reconcileCandidate:` and validation remain available.

Publication plans now contain six values: `[action, unit, name, priorUnit,
expectedGeneration, successorGeneration]`. Automatic activation-event records and
the Store basis have been removed. Preparation captures the lifecycle fields it
reads, so unrelated facts no longer invalidate a token. Binding-owner and source
revision checks remain while the whole-program coordination model is narrowed.

Image composition replaces its current root without erasing historical records or
walking an old generation graph to retract facts. Unreferenced records follow GC;
callers retaining records keep their values and annotations. Restored inverse
capabilities remain unavailable. Rebuild images created with the old fact-backed
record layout; no fact-to-field compatibility adapter or user-fact scrubber exists.

See [opt-in baselines](../../docs/design/opt-in-generation-baselines.md) for removal
of startup/worker adoption and sealing. Source-registry transactions retain their
owner stamps for coherent image exchange; this does not add checks to ordinary
message sends. Candidate lineage coupling and baseline renewal remain follow-up work.
