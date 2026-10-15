# Method publication ownership

2026-10-14. **Design and executable characterization; migration not implemented.**
This is the remaining publication part of step 5 of the
[optional Store decision](optional-store-and-relational.md). Globals has already
moved. The accompanying [audit](store-removal-audit.md) checks the completed slices.

The [2026-10-15 failure-boundary experiment](publication-failure-boundary.md)
finds a fresh-process synchronous-artifact route, but its native failure channel
is lost after mapped/rebuilt image restoration. Subsequent user direction rejects
maintaining two language error conventions and removes backwards compatibility as
a requirement. The [shared-invocation investigation](shared-invocation-investigation.md)
now takes priority: ordinary message/result semantics must be independent of
adapter representation, and per-definition generated adapters may be replaced by
shared forwarding code. The [binding constructor](method-binding-constructor.md)
is implemented and qualified as the first slice; default construction and the
publication owner migration remain pending.

## Authority and boundaries

Id's live vtable dictionaries remain authoritative. Reflect remains the public
inspection and editing facade. Store descriptions do not determine whether a
method exists or is callable. Direct Id mutation remains available at the floor;
managed publication adds policy and notifications for its clients, not a universal
permission check on every dictionary edit or send.

The destination is ordinary Birnam code and inspectable owner state. Do not move
Store's publication fields into a new permanent Rust registry. A minimal build
seed may support source installation, but must have an explicit replacement point.
No method identity guards, observer-address caches, automatic callback replay or
new universal event bus are needed for this migration.

## Existing observable sequence

The native characterization is
[`publication_contract.rs`](../../rust/birnam-siphon/tests/publication_contract.rs).
These are current low-level Store behavior, not proposed nicer semantics:

| Boundary | Current behavior to preserve |
| --- | --- |
| Reflect entry | Check Generation's ordinary-mutation policy before constructing/publishing a Method. Instance definitions obtain a private vtable when necessary. |
| Managed authorization | Send the registered authorizer `value`. Only exact true allows publication. Other loaded values, including false or Error, return unchanged. Native failure propagates before mutation. |
| Validation | Removal checks stable/exported vtable ownership and direct presence; absence returns `#missingDirectMethod`, not an inherited copy. Add retains its existing dictionary/result contract. |
| Dictionary edit | Invalidate old descriptive rows and send the live dictionary edit. Id commits before its raw `methodChanged:on:` notification. |
| Raw notification | Store currently removes stale mirror rows across its stores, without outgoing callbacks in its canonical handler. A failing raw observer can leave the dictionary changed and no current mirror row. |
| Description | Successful add installs its mirror row before managed notification; remove leaves no row. Rows are non-durable implementation descriptions, not arbitrary user facts. |
| Managed notification | Send `managedMethodChanged:on:` after dictionary/description work. Loaded results, including Error objects, are void results and are ignored. Native failures propagate after commit. |
| Completion | Store returns itself on success. A post-commit notification failure does not automatically undo the method edit. Generation's separate journal performs selective compensation where required. |

Tests distinguish denial, native authorization failure, raw failure, managed
failure, ordinary Error return, replacement and missing removal. A failing managed
callback writes an unrelated explicit fact; that write survives. Existing Generation
transaction tests cover the larger compensation protocol. This table does not
claim that all possible callbacks or nested compiler paths have been characterized.

Raw notification roots belong to Id, the caller. Registration can be cleared or
replaced during lookup or callback execution. The entered receiver and selector/
vtable arguments must remain reachable until the send completes, including native
failure. Id now uses a nested temporary root scope, independent of Store and GC
policy. Registration is for future notifications, not the lifetime of an entered
notification. The observer regression exercises replacement, reentry, collection,
self-removal, failure and reclamation after return.

## Proposed owner and API

**Proposed name:** `MethodPublication`, bound in Namespace as well as available to
source. Rust consumers currently resolve native owners through Namespace; a
Globals-only object would not replace that dependency. All bindings remain ordinary
and replaceable. Reflect's user-facing define/remove selectors remain unchanged,
including returning the original target on successful publication and forwarding
a veto/error unchanged. The owner-success receiver is an internal protocol detail.

| Proposed owner message | Contract |
| --- | --- |
| `addMethod: selector on: vtable with: method` | Authorize, validate, publish and notify. Return the resolved publication owner on success; preserve current veto/error behavior. No automatic fact description. |
| `removeMethod: selector on: vtable` | Remove the direct binding under the same policy. Expose inherited behavior naturally. Same missing-method result. |
| `setMethodMutationAuthorizer: object` | Preserve current single-registration policy and same-object idempotence. Ordinary owner state, inspectable through its protocol. |
| `setMethodMutationObserver: object` | Preserve the required Generation observer and exact managed notification semantics. This is not an optional catalog subscriber list. |

Resolve and retain the owner once for an operation. Standard Store facades,
including cloned Stores, use that same managed policy owner; their explicit
Store receiver selects the description destination. Do not duplicate Generation
policy into every Store. Rebinding the export selects future operations; entered
work keeps its retained receiver and continues to use live sends.

The old setter spellings are deliberate compatibility choices, not new global
rules about replacing methods. Their existing registration policy can later be
changed deliberately through the live protocol. Implementation must expose named
state and must not make native-family internals writable through foreign offsets.

A Store-free owner must accept an already constructed Method on the Id/Execution
floor. This is the first operational independence target. It does not establish
that the compiler, image construction, Generation or full bootstrap is Store-free.

## Explicit Store participation

Keep explicit Store `addMethod:on:with:` and `removeMethod:on:` as explicit-description
entry points. Calling them opts that operation into the existing description
behavior; Reflect's new default does not. Both must share the same authorization
and publication contract, rather than maintain two competing method authorities.

The explicit-description path needs a per-operation description action between dictionary
success and managed notification. That action must be explicit in the internal
protocol, with its own failure tests; do not implement it by describing after the
public operation returns, which reverses notification order. Its exact selector
and source-level failure transport remain implementation gates below. An absent
action must not resolve Store or allocate a catalog. No ambient global switch is
needed to disable facts.

Existing stale-row invalidation remains a responsibility of an opted-in description
owner. Do not make a Store callback mandatory just to use Id. A retained snapshot
needs no invalidation subscription; any live description must explicitly arrange
one and describe its lifetime. This slice does not promise a generic live projection
API. Store and Relational's own bootstrap descriptions must remain available.

At cutover, remove only positively identified private automatic mirror rows, never
all facts having a vtable/selector shape. Explicit assertions and their history
belong to their users. Measure residual reachability after cleanup; merely stopping
new rows does not release old roots.

## Cutover, Generation and images

Route Reflect, Codegen/Image installation, Alien wrappers and Generation through
the resolved owner in one coherent slice. Reflect and Image currently compare
success with Store identity; compare with the actual owner selected for that
operation instead, retaining their public return contracts.
Generation must still register its authorizer/observer and acknowledge the exact
expected selector, vtable and Method. Unrelated callback changes remain drift and
must survive compensation. No rollback by restoring an entire old dictionary.

Bootstrap must install the seed early enough for authored source, install the
Birnam owner once its dependencies exist, transfer only necessary state, then
retire the seed's default authority and mirror roots. Record the exact installation
order and demonstrate no hidden Store lookup on the floor. Avoid claiming that a
seed is temporary while the new owner still delegates every operation to it.

Backwards compatibility is not required for this migration. Image save/load and
rebuild must preserve the chosen ordinary result protocol, identities and active
owner state. Namespace rebinding still participates in transactional recovery and
restores the prior resident owner on failed activation. This is a current-world
correctness requirement, not a requirement to retain legacy publication adapters.

Native Store hook fields are currently process-local, not durable fact data.
Moving them into ordinary slots must not accidentally serialize a process-local
Generation guard, candidate, callback or native address. Specify the persistent
projection and hook reinstallation before image activation, then test mapped,
rebuilt and relocated images and activation failure. The new owner must not become
a second durable authority for Id dictionaries.

## Remaining implementation gates

The architecture is settled, but the migration is **not yet mechanically ready**.
These are bounded experiments/specification work, not permission to simplify away
observable behavior:

1. Demonstrate exact native-failure transport through a small authored publication
   body and native caller. Execution converts some native failures to language
   Error values. An Error-valued void notification must still be ignored, while a
   native failure propagates after commit. `Execution evaluatePreservingFailures:` exists, but
   wrapping an entire body can also change failure handling *inside* callbacks.
   Prove the intended boundary with callback-internal catch/return tests before
   choosing a wrapper or proposing a new primitive. The experiment above rejects
   ambient preservation and qualifies only the fresh synchronous-artifact boundary;
   its reproduced image adapter defect must be corrected before adoption. This
   does not require preserving two result conventions: subsequent user direction
   requires one ordinary message/error protocol across representations.
2. Specify and test the per-operation opt-in description action, including
   reentry, failure and ordering relative to Generation's managed observation.
3. Write the concrete bootstrap/image admission and hook-lifetime transition,
   including the exact rollback point and the current public success-result behavior.
4. Inventory every publication consumer and seed mirror root, then establish the
   Store-absent precompiled-Method floor before changing full startup.

If one of these requires a new language semantic rule, stop and report it. Do not
turn an existing native failure into an ordinary Error merely to make the port
shorter, and do not retain a permanent Rust publication implementation by default.

## Verification scope

`MethodPublication.tla` models authorization, commit, raw callback self-removal and
collection, description ordering, managed failure, void Error return and temporary
root release. Deliberately broken variants roll back a committed edit, notify too
early, treat a void Error as veto, omit receiver protection or leak it. The model
abstracts away method lookup, heap layout, source/native bridging and Generation's
journal; it is not a proof of the Rust or future Birnam implementation.

Before cutover, run the characterization against both owners; add native/source
failure boundary tests, direct/inherited method tests, live replacement and GC,
Generation exact compensation/drift tests, image recovery, Store/Relational
self-description, full language and REPL gates. Default publication must create
zero descriptive facts while explicit Store publication retains its selected
semantics. Installed release qualification is separate from these checks.
