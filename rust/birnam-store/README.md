# Store and Cells over Id messages

Local mutation now defaults to the authored [Cell owner](../../modules/cell.birnam)
over ObjectModel slots. Store's duplicate `localCell:` variants and tagged local
records have been removed. `CellVt` here contains only explicit fact-backed handles.
Images containing the removed local representation must be rebuilt.

`Module load: #store` activates an authored entry, which loads
`lib/native/libbirnam_store.dylib` with Namespace through current Id messages.
Store is absent from the core bootstrap/compiler closure. Store depends only on the
common Id ABI. It publishes completed `Store`, `StoreVt`, `CellVt` and `db` objects.
Globals is constructed separately by its authored owner; Store never publishes or
replaces it. `db` initially names Store; repeated loading in the same
Namespace returns the published Store.

Store retains the existing fact-log, cardinality, prospective history, retention,
compaction and indexed-snapshot algorithms. Facts compare entity, attribute and
value by identity. Each native Store instance owns its facts and each Cell owns
its Store, entity and attribute references. Those representations are private:
there are no foreign payload reads, service tables or encoded function pointers.

## Language protocol

Arguments and answers below are language Objects, including boxed Integers and
canonical Booleans. Wrong arities and unowned receivers fail the native protocol. Domain errors answer current Error objects. Dependencies are
opaque receivers selected at construction; their methods are looked up on every
send. Integer conversion and Error construction therefore observe replacement.

| Receiver / selectors | Contract |
|---|---|
| Store `new` | Independent seeded Store; the schema itself is data. |
| `assert:attr:value:`, `retract:attr:value:` | Schema-aware mutation; duplicate assertions and absent retractions do not advance the basis. Answer Store, or a resource Error. |
| `rawAssert:attr:value:`, `flip` | Raw historied assertion; leave raw mode. Preserve the existing image/bootstrap behavior. |
| `currentValue:attr:`, `currentValue:attr:ifAbsent:` | Current value; absent is nil or the result of sending `value` to the fallback. Bound nil is present. |
| `rawCurrentValue:attr:` | Object reference or null Oop for absence. Language execution converts null to nil; native consumers can distinguish it. |
| `hasValue:attr:`, `count:attr:`, `cardinality:` | Presence, cardinality-aware count, and schema cardinality (default `#one`). |
| `attrCount:`, `attrAt:index:` | Current attributes of an entity; out-of-range native result is null. |
| `history:attr:`, `basis`, `oldestRetainedBasis` | Retained transition count and exclusive transaction watermarks. |
| `valueOf:attr:asOf:` | Value strictly before the supplied basis; unavailable history answers `historyNotRetained`. |
| `retainCurrentOnly`, `retainSince:`, `retainLast:`, `compact` | Existing retention policy and explicit compaction. Cannot recover previously discarded history. |
| `retainedFactCount`, `retainedFactBytes`, `factBytesLimit`, `factRecordBytes` | Actual retained-fact diagnostics and configured semantic fact ceiling. |
| `retentionPolicyCode`, `retentionPolicyArgument` | Current-only = 0, keep-since = 1, keep-last = 2. |
| `reservedCompensationFactCount`, `reservedCompensationFactBytes` | Outstanding candidate compensation reservations; consumed or released through the owner messages described below. |
| `liveTriplesDo:`, `liveTriplesAsOf:do:`, `liveTriplesSince:do:`, `liveTriplesBetween:and:do:` | Snapshot current/as-of triples, then send the block `value:value:value:`. Bounds preserve the existing inclusive-lower/exclusive-upper semantics. |
| `cellFor:attr:` | New Cell retaining this Store and the exact entity/attribute. |
| Cell `value`, `set:`, `history` | Fresh sends to its Store. `set:` returns the Cell on successful assertion and otherwise propagates the Store result. |

`BIRNAM_STORE_COMPACTION_TRANSITIONS` retains its default of 4096;
`BIRNAM_STORE_FACT_BYTES_LIMIT` defaults to 64 MiB. Automatic compaction honors the
retention floor. Exhaustion returns `historyResourceLimit` without committing the
requested fact or advancing the basis. Physical compaction may already have run.

## Independent bindings and optional method descriptions

[Authored Globals](../../modules/globals-owner.birnam) owns bindings independently
of Store. The native Globals seed/facade, import and automatic-fact cleanup are
removed. Explicit `#__globals__` facts are ordinary independent data; they do not
change bindings, and binding writes do not change Store's basis. Loading Store
after Globals preserves the binding facade, storage owner and existing values.
See [the ownership contract](../../docs/design/owned-globals.md).

The existing MOP responsibilities are preserved through messages:

| Store selector | Contract |
|---|---|
| `setMethodMutationAuthorizer:` | Register one rooted Object receiving `value`; only true authorizes managed mutation. Re-registering the same receiver is allowed. |
| `setMethodMutationObserver:` | Register one rooted Object receiving `managedMethodChanged:on:` after managed mutation; same one-time registration rule. |
| `addMethod:on:with:` | Authorize, clear the old mirror, send the behavior `addMethod:as:` with an owned Execution Method, append its Method mirror and notify the observer. |
| `removeMethod:on:` | Authorize, require an owned or Namespace-exported behavior and a direct binding, clear its mirror, send `removeMethod:` and notify. |
| `backfillMirror` | Enumerate current Object-derived behaviors/selectors through Id; add missing primitive mirror rows without advancing the basis. |
| `methodChanged:on:` | Invalidate the selector/behavior mirror after a direct Id dictionary write. |

These are explicit Store operations. Ordinary Id dictionary edits do not enroll
Store or notify it. Call `methodChanged:on:` explicitly to invalidate a retained
method description after an external edit, or take a fresh `backfillMirror`
snapshot. The removed Id notification/entry interface is not recreated here.
Mirror retirement deletes only marked mirror rows, preserving explicit facts.


The authorizer now accepts an Object instead of a native function address, and
managed installation accepts an owned Execution Method instead of a raw entry.
Reflect's ordinary define/remove operations send directly to the vtable owner and
create no mirror facts. Store's protocol is an explicit description facility. Private compensation
algorithms remain tested migration inputs; their retired native table is not
exported.

## Collection, callbacks and lifetime

The module instance registers `rootsDo:`, `referencesOf:do:` and `retire:` through
Roots and GC. Store and Cell allocations are opaque zero-payload identities;
their weak owner registries hold the native state. Reachable Stores enumerate all
retained facts, including historical/retracted values. Cells enumerate all three
references. Retirement removes dead registry entries and drops their Vec resources
before address reuse. The registries themselves do not keep their keys alive.

Every operation roots its inputs before any dependency send. Iteration and image
export install a rooted snapshot before callbacks begin. A callback can clear,
compact or rewrite the original Store and collect without invalidating remaining
snapshot rows. No Rust registry borrow crosses a send. Callback failure propagates
and drops temporary roots, just like successful completion. Store mutation sends
GC `prepareAllocation`, so mutation during tracing/finalization is rejected.

`rootsDo:` and `referencesOf:do:` take visitor Objects and send `visit:` for each
reference through current Id lookup. GC's allocation inventory and release also
use messages; see the [GC protocol](../birnam-collect/README.md).

## Image transfer and remaining integration

The existing language transfer protocol is migrated:

- `imageFactsInto:` sends `e:a:v:tx:op:historied:` to its collector. Derived mirror
  facts and attributes marked `imageTransient` are omitted.
- `imageLoadBegin` clears facts and reservation/retention state.
- `imageAddFact:attr:value:tx:op:historied:` imports a row with Integer transaction,
  Integer operation (0 assertion, 1 retraction), and Boolean history status.
- `imageLoadEnd:raw:oldest:policy:argument:` restores retention metadata, validating
  nonnegative/range/Boolean constraints before applying it.

[Relational](../birnam-relational/README.md) consumes these Store messages.
Image recovery and authored task/resource policy still use this Store through
owner messages; removing their mandatory dependency is remaining work. Their end-to-end checks run in the native
message and installed Host groups; the transfer tests here isolate Store behavior.

## Validation

Candidate compensation uses `reserveCompensationFor:count:` and
`releaseCompensationFor:` on the Store receiver. Reservations retain their owner
and count against the fact limit until consumed or released.
`restoreValue:attr:expected:previous:reservation:` accepts native Unit for absent
expected/previous values, distinct from an Object containing language nil. It
compares one single-valued fact and spends one reserved record atomically; a
conflict leaves both the fact and reservation unchanged. It does not restore a
whole Store snapshot or undo unrelated callback facts. The native
`store_compensation` binary verifies these properties at the resource limit.

`mise run native-messages` builds and audits Store's real dylib alongside the
production prefix. The Store binary covers schema/history/retention, errors,
Cells across precise GC, reclamation, live replacement of Store and Integer
methods, collection during initializer validation, destructive collecting snapshot
callbacks (success and failure), image row transfer, phase rejection and bounded
resource refusal/recovery. It also checks independent Globals and explicit fact history, MOP
permission/notification and mirror invalidation, rooted hook receivers, live
replacement of Id's notification target method, and compiled Store/Cell/Globals
consumers.

`mise exec -- cargo test --offline --locked -p birnam-store --lib` preserves ten
algorithm tests, including the independent scan oracle across 64 histories and
before/after compaction. Two manual timing investigations remain ignored.

## Optional image state

Native `imageStateRevision` returns the process-local data revision as a Word.
Optional image saving observes it to detect fact, policy, compaction or reservation
changes, including changes that do not advance the durable fact basis. Revisions
remain attached to Store identities and are not persisted as image data.

`swapImageStateWith:` is the selected forward exchange for explicit Store image
participation. Failed live loading retains partial effects; it does not invoke an
inverse. The unused conditional-exchange variant has been removed. Core images
perform neither Store revision reads nor exchanges.

## Oop integration (2026-10-26)

The native module now declares Methods in its object-file descriptor and binds
them to its Namespace Oop. Private Store state stays with this owner; no native
state pointer or entry address is sent through Id. Retirement receives dead Oops
directly. Mirror snapshots use `allocationsDo:` and `bindingsDo:`; they do not
restore the removed count/index enumeration interface. Signed numeric conversions
use Integer and Word messages, and native absence remains distinct from bound nil.

`scripts/oop-optional-test.sh` qualifies explicit compiled activation, retained
source tests, history and compensation limits, collecting snapshots, Cell and
method replacement, queries, and provider image recovery. The older native test
harness still requires its separate Oop migration.
