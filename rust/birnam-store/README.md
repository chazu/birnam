# Store and Cells over Id messages

Local mutation now defaults to the authored [Cell owner](../../modules/cell.birnam)
over ObjectModel slots. Store's `localCell:` variants and tagged local records
remain for explicit legacy callers and image compatibility; core authored modules
no longer use them. `CellVt` here is the Store-owned legacy/fact-handle family.

Bootstrap loads `libbirnam_store.dylib` with Namespace after the value and
collection owners, before Execution and the compiler. Store depends only on the
common Id ABI. It publishes completed `Store`, `StoreVt`, `CellVt`, `db`, `Globals`
and `GlobalsVt` objects. `db` initially names Store; repeated loading in the same
Namespace returns the published Store.

Store retains the existing fact-log, cardinality, prospective history, retention,
compaction and indexed-snapshot algorithms. Facts compare entity, attribute and
value by identity. Each native Store instance owns its facts and each Cell owns
its Store, entity and attribute references. Those representations are private:
there are no foreign payload reads, service tables or encoded function pointers.

## Language protocol

Arguments and answers below are language Objects, including boxed Integers and
canonical Booleans. Wrong operand kinds, arities and unowned receivers fail the
native protocol. Domain errors answer current Error objects. Dependencies are
opaque receivers selected at construction; their methods are looked up on every
send. Integer conversion and Error construction therefore observe replacement.

| Receiver / selectors | Contract |
|---|---|
| Store `new` | Independent seeded Store; the schema itself is data. |
| `assert:attr:value:`, `retract:attr:value:` | Schema-aware mutation; duplicate assertions and absent retractions do not advance the basis. Answer Store, or a resource Error. |
| `rawAssert:attr:value:`, `flip` | Raw historied assertion; leave raw mode. Preserve the existing image/bootstrap behavior. |
| `currentValue:attr:`, `currentValue:attr:ifAbsent:` | Current value; absent is nil or the result of sending `value` to the fallback. Bound nil is present. |
| `rawCurrentValue:attr:` | Native Object or Unit for absence. Language execution converts Unit to nil; native consumers can distinguish it. |
| `hasValue:attr:`, `count:attr:`, `cardinality:` | Presence, cardinality-aware count, and schema cardinality (default `#one`). |
| `attrCount:`, `attrAt:index:` | Current attributes of an entity; out-of-range native result is Unit. |
| `history:attr:`, `basis`, `oldestRetainedBasis` | Retained transition count and exclusive transaction watermarks. |
| `valueOf:attr:asOf:` | Value strictly before the supplied basis; unavailable history answers `historyNotRetained`. |
| `retainCurrentOnly`, `retainSince:`, `retainLast:`, `compact` | Existing retention policy and explicit compaction. Cannot recover previously discarded history. |
| `retainedFactCount`, `retainedFactBytes`, `factBytesLimit`, `factRecordBytes` | Actual retained-fact diagnostics and configured semantic fact ceiling. |
| `retentionPolicyCode`, `retentionPolicyArgument` | Current-only = 0, keep-since = 1, keep-last = 2. |
| `reservedCompensationFactCount`, `reservedCompensationFactBytes` | Outstanding candidate compensation reservations; consumed or released through the owner messages described below. |
| `liveTriplesDo:`, `liveTriplesAsOf:do:`, `liveTriplesSince:do:`, `liveTriplesBetween:and:do:` | Snapshot current/as-of triples, then send the block `value:value:value:`. Bounds preserve the existing inclusive-lower/exclusive-upper semantics. |
| `cellFor:attr:` | New Cell retaining this Store and the exact entity/attribute. |
| `localCell:`, `localCell:prototype:` | Transient Cell without fact transitions; the optional Cell prototype supplies behavior. Its three image fields retain a tagged value, with zero history. |
| Cell `value`, `set:`, `history` | Fresh sends to its Store. `set:` returns the Cell on successful assertion and otherwise propagates the Store result. |

`BIRNAM_STORE_COMPACTION_TRANSITIONS` retains its default of 4096;
`BIRNAM_STORE_FACT_BYTES_LIMIT` defaults to 64 MiB. Automatic compaction honors the
retention floor. Exhaustion returns `historyResourceLimit` without committing the
requested fact or advancing the basis. Physical compaction may already have run.

## Globals and method hooks

The standard bootstrap now installs [authored Globals](../../modules/globals.birnam).
Its bindings and reservations are ordinary owned state; automatic global facts
are disabled. The native facade described below remains the seed and legacy-image
backend. Explicit facts retain their normal independent history/query behavior.
See [the ownership contract](../../docs/design/owned-globals.md).

Globals now also answers `liveBindingsDo:` and `nameOf:`. `liveBindingsDo:` takes
a visitor Object receiving `binding:value:` and snapshots the owner's current
bindings in existing attribute order, retaining names and values across collecting
or reentrant callbacks. It excludes absent values and includes bound nil. This
enumerates owned storage, rather than calling a potentially customized `at:` for
each row. Callback native failures propagate; ordinary callback return values are
ignored. No fact is created by inspection. `nameOf:` returns the first identity
alias or native Unit for absence, through the existing replaceable Store
attribute/read methods. These interfaces hide the `__globals__` representation
from Reflect and tools while retaining the current fact-backed storage.

Globals retains its Store-backed `__globals__` facts. `at:`, `at:put:`,
`at:ifAbsent:` and `includes:` send to Store's current read/assert/presence methods.
Global values participate in ordinary Store queries, prospective history and image
transfer. Missing reads answer nil; bound nil suppresses the fallback. The source
compiler publishes top-level bindings through this same `at:put:` protocol. Its
reads use `at:` and preserve the historical Namespace fallback when the answer is
nil. This source fallback is separate from `at:ifAbsent:`, which still distinguishes
a bound nil from absence.

The existing MOP responsibilities are preserved through messages:

| Store selector | Contract |
|---|---|
| `setMethodMutationAuthorizer:` | Register one rooted Object receiving `value`; only true authorizes managed mutation. Re-registering the same receiver is allowed. |
| `setMethodMutationObserver:` | Register one rooted Object receiving `managedMethodChanged:on:` after managed mutation; same one-time registration rule. |
| `addMethod:on:with:` | Authorize, clear the old mirror, send the behavior `addMethod:as:` with an owned Execution Method, append its Method mirror and notify the observer. |
| `removeMethod:on:` | Authorize, require an owned or Namespace-exported behavior and a direct binding, clear its mirror, send `removeMethod:` and notify. |
| `backfillMirror` | Enumerate current Object-derived behaviors/selectors through Id; add missing primitive mirror rows without advancing the basis. |
| `methodChanged:on:` | Invalidate the selector/behavior mirror after a direct Id dictionary write. |

Id `methodChangesTo:` registers one rooted notification receiver (or Unit to clear)
and returns the previous receiver. Store registers itself. Completed Id dictionary
add/remove/weak-binding operations send `methodChanged:on:` after mutation, with no
dictionary borrow held. This is fresh lookup, not a saved callback. Notification
failure propagates after the dictionary change; it does not roll the change back.

The authorizer now accepts an Object instead of a native function address, and
managed installation accepts an owned Execution Method instead of a raw entry.
Reflect's `defineMethod:on:as:` now sends this managed installation protocol.
Remaining deferred compiler callers must adopt those operands. Private compensation
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
Image recovery, authored task policy, candidate publication and Host/CLI now use
this Store through owner messages. Their end-to-end checks run in the native
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
resource refusal/recovery. It also checks Store-backed Globals history, MOP
permission/notification and mirror invalidation, rooted hook receivers, live
replacement of Id's notification target method, and compiled Store/Cell/Globals
consumers.

`mise exec -- cargo test --offline --locked -p birnam-store --lib` preserves ten
algorithm tests, including the independent scan oracle across 64 histories and
before/after compaction. Two manual timing investigations remain ignored.
