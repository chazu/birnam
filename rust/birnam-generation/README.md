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

`sourceRegistry` returns `[sealed, [name, compilationRecord], ...]`; `sourceRegistrySealed`
returns a Boolean. Registration uses `recordCompiledSource:named:` and
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
projected Globals. Preparation runs no candidate factory. Relevant lifecycle field observations, Globals basis
and Id behavior epoch checks reject changes during callbacks; monotonic tokens
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
displaced Methods for inverse operations. Preparation checks
the complete ledger; `reconcileCandidate:` rejects drift without adopting the new
binding. Unrelated Store facts remain outside that ledger. Repeating initial
adoption validates the original snapshot and cannot clear an observed divergence.

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
the commit selectors so policy can commit immediately without constructing new
branch Blocks between token creation and validation.

Installed Host and published-project console tests exercise authored activation
and exact rollback. Image stages suspend all old candidate capabilities before
publication and retain their roots until the application outcome is known. After restoring dispatch,
`recoverImageGeneration:` enters the authored composition policy;
`adoptComposedGeneration:` is available only in that dynamic recovery scope and
validates the composition identity, source/selection authority and rollback-free
components before rebuilding the live ledger. Basic live Image application failure
abandons the old capabilities instead of restoring caches over a partial world;
success releases their roots. Cancellation before application can still release
staging without changing the prior world.

`discardCandidate:` rejects active, prepared and rollback-reachable candidates,
removes only the admitted author-file contents, and publishes the discarded state
before releasing native roots. A process-local `DiscardReceipt` separates logical
discard from physical disposition. Live Methods, Blocks and Machines retain their
Units through Execution; GC and TCC reclaim them at an idle checkpoint. Weak TCC
mapping identities drive `retainedGenerationCount` and `retainedGenerationBytes`
without themselves keeping code alive. File removal is retryable but is outside
Store compensation: a failed discard can leave one or both author files removed.

Generation adoption installs no Store observer. Explicit transactions send their
Method writes directly to the vtable owner and check the result and Id binding
epoch. Exactly one dictionary change is expected per write; extra callback writes,
including ABA, invalidate the transaction. This is a conservative data condition
on requested coordinated work, not a method-identity guard on ordinary sends.
Final ledger checks, selective compensation and quiescent commit checks remain.
Ordinary edits are detected when a consumer explicitly reconciles its revision.

`validateImageGeneration` stays strict for callers asking whether the managed
revision is current. Image uses `prepareImageSnapshot`: it performs the existing
lazy drift reconciliation, but permits a snapshot of changed live code. This can
mark opted-in revision records drifted and disable rollback; it does not replace
program bindings. Unimageable opaque native replacements of tracked bindings are
still rejected. A snapshot consistency stamp rejects detected dictionary, Globals
or fact-state changes during serialization without replacing the destination.

Matching managed images still use composition/adoption. Drifted revision records
reload as descriptive data with no transient ledger or activation capabilities;
actual saved Methods remain callable and editable. Explicit managed validation and
activation still reject stale/unadopted state. This does not make Store or Generation
optional in full image/bootstrap paths yet. Basic live loading now reports partial
application without automatic compensation.

The native `materialization` test also retains a reflected Method after its source
Block is released, transfers ownership to an alias binding on another Block,
and removes that final binding. Source, synchronous and resumable cases each run
three cycles; both live TCC Unit counts and relocated native bytes return to the
receiver baseline, then to zero after releasing the receiver. This replaces
`scripts/method-unit-lifetime-smoke.sh` and requires TCC resource metrics, enabled
by `scripts/native-message-test.sh`.

Image source staging now retains an owner revision. `swapSourceRegistry` publishes
only at its captured revision; `rollbackSourceRegistry` restores only its own still
current publication. A later record/seal or transaction replacement is preserved.
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
including A-to-B-to-A changes. Registration-policy sealing uses the existing control
revision and does not spuriously invalidate unchanged metadata. Successful activation
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
