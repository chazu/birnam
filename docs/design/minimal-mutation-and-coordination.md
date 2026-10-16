# Ordinary mutation and explicit coordination

2026-10-15. **Agreed direction and initial feasibility investigation.**
The subsequent [ordinary editing/snapshot slice](ordinary-edit-and-snapshot.md)
implements the first runtime cutover. The [coordination cleanup](coordination-cleanup.md)
now implements default-mirror removal, direct Generation writes and the fresh/live
loader split with conflict-safe recovery. The later [basic live-loading decision](basic-live-image-loading.md) removes
automatic live-load compensation; users own their recovery snapshots. Remaining
owner migrations are still work. This supersedes the proposal to move Store's complete
publication machinery into a universal `MethodPublication` owner. The user also
selected retaining live in-place image loading as an explicit advanced feature.

The destination remains a fully malleable, eventually self-hosted Birnam. Id owns
its dictionaries; Execution owns Method/code lifetimes; ordinary sends remain
late-bound. Store and Relational remain available, self-describing opt-in facilities.

## Decision

An ordinary definition prepares a Method and changes the target dictionary through
its owner. It does not implicitly enter a managed generation, request authorization
from a global policy object, create facts, subscribe observers or create a rollback
journal. Removal edits a direct dictionary entry and naturally reveals inheritance.

Keep the existing `Reflect defineMethod:on:as:` and `removeMethod:on:` user facade.
Their implementation can compose the existing constructor and dictionary messages.
A separate mandatory `MethodPublication` object is not justified by this operation.
A small shared helper may be useful, but should not become another authority.

Stronger guarantees belong to explicit operations: preparing a coordinated change
set, adopting or checking a managed revision, retaining descriptions/history, or
replacing an already-running world from an image. Availability of those facilities
must not enroll every method edit in their policy.

## What remains essential

| Obligation | Owner and reason |
| --- | --- |
| Dictionary addition, replacement, removal and delegation | Id. The actual dictionary is the authority for ordinary lookup. |
| Current sends see current lookup | Id/Execution. Includes custom lookup and the implementation's later `value:` send. |
| Captured/entered Methods retain selected code | Execution. Replacing a dictionary entry must not rewrite an entered frame or reclaim its code. |
| Safe collection and retirement | Actual owners and caller roots, including collecting/reentrant calls. |
| Optimization validity and deoptimization | Existing execution/compiler machinery. Removing Store observers must not remove invalidation at the dictionary owner. |
| Exact target and result semantics | Reflect preserves instance/vtable interpretation, private-vtable behavior, direct versus inherited removal and useful errors. |

The lightweight core path does **not** require global mutation authorization,
a method fact mirror, Generation acknowledgements, all-world observation, or
compensation for a single ordinary edit. A primitive may fail after a real effect;
its caller must not infer that every Error means nothing changed. Optional consumers
must state their own result and failure contracts.

## Existing APIs already demonstrate the boundary

The following are existing messages, not proposed new services:

- `NativeBackend named:synchronous:` and TCC Unit `initializeWith:` can create a
  neutral resumable template on the build floor.
- A selected Method's `withImplementation:` constructs an independent binding.
  The production authored `MethodInvocation` policy already composes this operation.
- `Id ensurePrivateVtableFor:` supplies the appropriate instance dictionary when
  needed. Existing Reflect target interpretation must still be preserved.
- A vtable's `addMethod:as:` installs an owned Method through Execution and Id.
- A vtable's `removeMethod:` removes a direct entry. Its low-level native reply is
  an entry or Unit; Reflect's ordinary result adaptation belongs in its facade.
- `methodFor:` captures current lookup. Direct inventory is a separate operation.
- `Id bindingEpoch` exposes mutation observations for explicitly prepared work.

No new constructor, transaction service or event bus was needed for the probe.
The production source/artifact construction policy must continue preserving the
code actually selected by the supplied artifact; ordinary mutation does not justify
recompiling or substituting precompiled code by arity.

### Store/Generation-absent floor

`scripts/minimal-mutation-test.py` exports four ordinary Block programs from the
qualified runtime and exercises them in an isolated native-module directory.
Store, Relational, Generation and Bootstrap libraries are absent. Their Namespace
bindings, `db`, Lang and `MethodPublication` are absent; no raw observer is installed.

The native probe passes:

- Pure Method binding construction without changing the dictionary epoch.
- Installation, replacement, inherited lookup, direct removal and missing removal.
- Repeated warmed lookup observing later replacement; independent parent/child state.
- A captured Method remaining callable after its dictionary entry is removed.
- Replacing an implementation's `value:` changes the retained outer Method's later send.
- Replacement from inside an entered method, followed by precise GC: the entered
  Method/body survive until return and are reclaimed afterward.
- Distinct receiver/argument identities and unary forwarding.
- ABA changing the mutation epoch, so explicitly prepared work can reject stale input.

This uses existing production primitives and exported programs. It is not a full
Store-free source bootstrap, an authored compiler port, a new deoptimization
implementation, or qualification of every optimized path. Existing primitives'
recovery obligations remain unchanged. Missing removal currently advances the
coarse epoch even without an entry; consumers must treat it conservatively, not as
an exact count of semantic changes.

### Historical running-policy probe (before the runtime cutover)

`scripts/mutation-policy-boundary-test.py` saves a main Block and runs it in a fresh
image process after deleting its writer source. On that baseline it established that:

1. `DispatchGeneration current` is nil: no managed generation was adopted.
2. Ordinary Reflect definition still returns `#generationNotQuiescent`.
3. An existing direct vtable operation changes Integer's `squared` from 9 to 777.
4. The previously captured Method still returns 9 after collection.
5. Generation validation succeeds because there is no adopted generation to compare.

Thus the blanket running guard is not merely protecting an opted-in revision.
A separate existing `generation_ledger` raw-mutation control passes with an adopted
generation: explicit reconciliation detects drift without a managed Store callback
for that raw edit. This does not establish that simply deleting the hooks is a
complete integration; it supplies evidence for checking explicit operations rather
than prohibiting ordinary edits globally.

## Generation as an explicit consumer

Generation should retain preparation, validation, activation and rollback where
those features are requested. Its descriptors and journals describe an operation
or a selected revision, not a second authority for the entire live language.

Replace the global authorize/observe handshake for its own writes with direct
operation data. A prepared operation already knows its target, selector, intended
Method and observed prior state. Obtain or validate commit information through
that owner; do not rediscover the operation through a globally registered callback.
External edits make relevant observations stale. Reconcile or reject a stale
prepared operation at its boundary, while leaving ordinary execution usable.

Data checks for an explicitly conditional mutation are legitimate; method-identity
checks around ordinary authored sends are not proposed. A global epoch is already
available but can reject for unrelated edits. More selective revisions are an
optimization to justify later, not a reason to add subscriptions to every object.

Do not equate a change set with arbitrary-callback atomicity. State precisely when
writes become visible, what is conditional, what can fail after commit and what
compensation promises. If rollback is requested, conditional inverses and adequate
reserved capacity still matter. A later writer must survive a conflicting inverse;
never replay arbitrary user callbacks to manufacture success.

## Description is composition, not a compulsory hook in the core

The earlier result-aware publisher experiment shows that a tool can sequence
mutation, description and notification. It does not establish that every edit
needs such a pipeline or a description-action argument.

An explicit tool or Store facade may perform a normal edit and then capture selected
state. Snapshots disclose their coverage and observation basis/interval; they do not
claim to update automatically or describe one atomic whole-world instant. If a tool
needs a stronger joint guarantee, it requests that coordinated operation explicitly.
There is no default managed notification whose ordering forces a description step
into ordinary mutation.

Store can keep explicit description conveniences and its own/Relational's bootstrap
self-description. Retire only positively owned automatic mirror rows, not arbitrary
user facts. A live projection, if later needed, owns its subscriptions, failure
behavior and retention. Merely loading Store must not register a universal mirror.

## Fresh startup and advanced live restoration

Fresh-process startup is already the normal command-line image path. Host creates
a native runtime and loads the image before entering main. Prepared bootstrap image
failure does not silently fall back to source. Workers likewise admit their image
before executing the requested application entry.

The simplification is to distinguish two loader contracts, not pretend that startup
already lacks validation or that a fresh process has no bootstrap state:

| Mode | Required contract |
| --- | --- |
| Fresh startup | Validate and stage executable/image data; expose no application entry before success. On failure, discard the isolated startup runtime/process. No promise to restore a usable previous application world. |
| Explicit live loading | Preserve entered-code lifetime and report failed application as partial state, without automatic compensation. Users own their recovery snapshot. |

The user explicitly chose to **keep live restoration as an advanced feature**.
Its API is retained. The [reproduced rollback bug](store-refactor-concerns.md)
was subsequently fixed by the [coordination cleanup](coordination-cleanup.md).
Conservative conflict reports can still leave a partially loaded world. Retaining
live loading does not imply arbitrary-callback atomicity or require maximal rollback;
the subsequent basic-live-loading decision selects partial application without
automatic compensation.

The fresh-start loader mode is an explicit host/image contract. Basic live
`System loadImage:` now deliberately skips rollback under its newly selected
partial-application contract; that is distinct from startup's mandatory discard. Do not infer permission to discard a world solely
from a Generation flag. The fresh caller must guarantee discard on failure and
must not resume source fallback in the half-restored world. Resource validation,
source-free loading, native signature handling and ordinary code ownership remain.

Any future stronger live recovery would need scoped, owner-specific conflict handling. This does not justify
making a general cross-owner transaction framework part of every method edit.
Strong restoration promises should stay within that explicit capability, and
arbitrary external side effects cannot be promised reversible.

## Implementation sequence and disposition

Steps 1–4 below are delivered by the ordinary-edit/snapshot and coordination
cleanup slices. Step 5 remains. The descriptions preserve the implementation
intent; they are not instructions to repeat the completed cutovers.

1. **Separate core mutation and managed observation.** Retain Reflect's public
   define/remove interface and current source/artifact constructors, but route
   its final edit to the vtable owner. Remove the running-generation veto from
   ordinary edits and mandatory hook installation from generic host entry.
   Explicit Generation operations keep their phase/admission requirements.
2. **Make revision metadata descriptive.** Stop requiring an adopted managed
   source revision to authorize a general snapshot of live code. Snapshot actual
   Method/CodeObject state and preserve available provenance. Managed metadata
   must report drift honestly, be explicitly reconciled, or remain historical;
   never silently label modified code as the old revision. Native handles and
   otherwise non-imageable objects still require normal rejection rules.
3. **Remove the default mirror and its bootstrap registration.** Leave explicit
   Store descriptions/self-description available. This includes retiring default
   mirror roots; redirecting the frontend alone is insufficient.
4. **Split fresh and live image admission contracts.** Keep fresh startup isolated
   and discardable; fix conditional compensation in the retained advanced live
   path. Do not make the latter a prerequisite of the ordinary dictionary API.
5. **Continue remaining owner migrations.** Compiler metadata/source registry,
   recipes, lifecycle records and process state retain their own inspection and
   persistence obligations. Complete cold bootstrap with optional facilities absent.

The delivered qualification covers ordinary editing and subsequent saving/restoring
of edited worlds, explicit stale-generation rejection, artifact-selected code,
direct/inherited removal and execution ownership. Tests for blanket rejection in
running main, console, TCP REPL and workers were intentionally replaced; rejection
of stale/reentrant explicit transactions remains meaningful. Remaining migrations
must retain these distinctions. No permanent Rust publication registry or hidden
fact store is part of this design.

## Validation and limits

The [receipt](../measurements/minimal-mutation-20261015.json) records the isolated
floor, exported programs, runtime identities, running-policy characterization,
managed control and formal results. The new abstract model separates an allowed
ordinary edit from a stale prepared commit and preserves the entered selection.
Negative variants add a running veto, mandatory mirror, active-call rewriting or
stale-plan overwrite. The model assumes owner-level mutation semantics; it is not
a proof of the compiler, whole runtime or future image architecture.

This initial investigation changed only tests, models and design documents. Its
later [runtime follow-up](ordinary-edit-and-snapshot.md) changes ordinary editing
and snapshot admission; it does not complete all ownership and image work above.
