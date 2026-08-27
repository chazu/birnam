# 60J — Runtime Identity and History Compaction

> **Status:** Accepted design boundary, 2026-08-24; J1-J5 implemented and the
> indefinite-run bounded-churn acceptance closed 2026-08-27. Weak identities,
> native admission and reclamation, bounded Store retention, image compaction,
> candidate compensation, portable CodeBindings, and save/load/resave churn now
> satisfy the fixed-live-set equation below.
>
> This record explicitly amends Design 61's process-lifetime permanence of
> dynamically created vtables and Symbols and its append-only vtable-enumeration
> result. It does not change the IdApi v1 table layout, field offsets, or ABI
> fingerprint. Canonical bootstrap identities remain permanent. Dynamic
> identities are live while reachable from current state or retained history.

## 1. Requirement and bound

One Birnam process, including one continuously evolved image, must be able to
run indefinitely without memory retained merely because code, names, or facts
existed earlier in that process.

No runtime can place a constant bound on a program that deliberately retains an
unbounded live graph or requests unbounded history. Birnam's hard requirement is
therefore the following space-safety equation:

```text
resident semantic state
  <= live object graph
   + explicitly retained fact/history window
   + live identity metadata
   + active and quiescing code generations
   + configured bounded allocator/index slack
```

Creating and then abandoning definitions, vtables, symbols, facts, candidates,
or native generations must not add a term proportional to process age. An
explicitly unbounded retention policy is allowed, but it is reported as such and
is not the default production policy. Hard resource ceilings remain the final
backstop: crossing one answers a normal resource-limit failure rather than
exhausting process memory.

The default retention policy is **current-only**. Deeper temporal retention is
opt-in and contributes directly to the declared bound.

## 2. Three lifetimes, kept separate

Compaction is not one undifferentiated collector.

| Lifetime | Authority | Reclamation rule |
|---|---|---|
| Heap values | `birnam-id` heap and GC | Ordinary reachability at a safe point |
| Facts and history | Store retention policy | Current projection plus the explicitly retained basis window |
| Native code | Runtime registration plus code repository | Binding/read/Frame/Continuation/lease quiescence and no live vtable carrier |

Identity indexes sit between the first two. They canonicalize a live identity,
but they are not semantic roots. Store facts, exports, live object headers,
method dictionaries, Machines, Continuations, and registered native providers
are roots; the symbol interner, vtable enumeration vector, and lookup maps are
weak indexes over those roots. Generated capture and vtable caches are
conditional native roots while their mapped generation is admitted. Retirement
withdraws those owners before collection and physical unload.

This preserves Birnam's malleability invariant: everything in the current graph
or retained temporal window remains an object and remains reflectable. State
below the retention floor is intentionally unavailable, not silently replaced
by a plausible value.

## 3. Dynamic vtable lifetime

### 3.1 Permanent and collectable classes

The boot metaobject closure, canonical family vtables, internal dictionary
vtables, and other explicitly process-global floor identities remain permanent.
Vtables created after bootstrap by `proto`, `delegated`, or
`ensurePrivateVtable` are collectable.

A collectable vtable is live when reached by any of:

- a live object's header;
- an export or registered native root;
- a current or retained Store fact;
- another live vtable's parent, owner, or selector edge;
- a live Runtime registration or image-recovery authority that explicitly owns
  the identity.
- a root-aware native cache belonging to an admitted code generation.

The vtable registry does not make an entry live. The block-vtable catalog and
generated capture/vtable cache tables are explicit owners while their
generation can still construct or recover objects. They are withdrawn as one
retirement operation; none becomes process-lifetime history.

`IdApi.proto` does not imply permanence. Every native caller retaining its
result across a safe point must either report the Oop through a process-lifetime
root provider or place it in a generation-keyed cache table. Generated code
must not retain a raw Oop in an unloadable `static` cell or key a process-owned
root by source identity.

### 3.2 Trace law

Every marked heap object contributes its header vtable as a strong edge. A
marked collectable vtable contributes strong edges to:

- its metavtable header;
- its parent;
- its owner;
- every selector in its direct dictionary.

Native Method values and raw layout bytes are not Oop edges. Runtime-owned
layout bytes must outlive their vtable; a vtable may not borrow layout storage
from a reclaimable code mapping.

Owner is a strong semantic edge. The owner/header cycle does not leak because a
tracing collector reclaims the whole unreachable cycle.

### 3.3 Enumeration amendment

`vtableCount` and `vtableAt` enumerate **currently live registered vtables** in
surviving creation order. Their indices are cursors, not identities, and are not
durable. Collection may remove dead entries and close the gaps while preserving
the relative order of survivors.

An enumeration is stable only within one owner-thread quiescent extent in which
the caller performs no allocation, send, or collection. Existing runtime
backfill and image walks already materialize their foreign carriers before
calling out and must continue to do so.

This is a semantic amendment to Design 61's append-only result, not an IdApi
layout change. It is necessary for bounded in-process evolution: tombstones or
permanent shells would still grow in proportion to all identities ever created.

### 3.4 Sweep order

At one stop-the-world owner-thread safe point:

1. mark language, Store, execution, export, and active native-cache roots;
2. close header-vtable and vtable-metadata edges;
3. remove unmarked dynamic vtables from the registry and block catalog;
4. retire any registration whose final semantic carrier disappeared;
5. detach and finalize dead heap blocks;
6. reconsider quiescent native generations for physical unload.

Retirement requested from an ordinary send is deferred until that send's
outermost read guard drops. At that safe point the Runtime may suppress only
the target generation's registration-anchor roots, and only after proving it
has no reader acquisition, Frame, Continuation, explicit lease, or permanent-
vtable binding. An externally reachable collectable vtable still marks its
owner and therefore preserves the registration anchor. The private retirement
seam is idempotent so a later quiescent code-repository reap can observe the
completed Runtime retirement and perform physical unload without weakening the
ordinary-send read guard.

Registry removal happens before heap detach, so no registry can expose freed
storage. Native unload happens last, so no surviving method or layout carrier
can name an unmapped address. A failed retirement restores only cache/catalog
entries whose vtables survived collection; cleared slots rebuild on later use.

## 4. Dynamic Symbol lifetime

The symbol interner is a weak canonicalization index, not a process-lifetime
root. A Symbol remains live through ordinary Oop reachability, including Store
facts, exports, live vtable selectors, heap slots, native root providers, and
conservative stack roots. Equal bytes intern to the same identity while that
identity is live; after collection, re-interning the bytes may produce a fresh
Oop with the same Symbol value.

The internal Symbol dictionary is also weak. Export names and live method
selectors remain strong through their actual authorities; the dictionary does
not keep otherwise dead names alive.

`symbolName` and `exportedNameOf` answer borrowed NUL-terminated bytes. The
pointer remains stable while the owning Symbol is rooted, but callers may not
retain the byte pointer independently across a safe point. A native component
that retains a Symbol must retain and report the Symbol Oop through its root
provider; a component that needs independent bytes must copy them.

This narrows Design 61's runtime-lifetime byte-pointer promise. Before Symbol
collection is enabled, every in-tree caller must pass an audit proving it copies
the bytes or keeps the owning Oop rooted. Compatibility loading must refuse
identity compaction when an admitted external component has not declared this
borrow discipline. Configured interner limits remain the safe bound in that
compatibility mode.

The declaration and control names are frozen outside IdApi v1:

- an audited native dylib exports `int birnam_symbol_borrow_v1(void)` and must
  answer exactly `1`;
- the host calls the floor-private
  `int birnam_id_symbol_compaction_configure_v1(int enabled, size_t count,
  size_t nameBytes)` seam before that dylib's `module_init`;
- a missing or non-`1` declaration disables Symbol compaction before
  initialization and requires positive `BIRNAM_SYMBOL_LIMIT` and
  `BIRNAM_SYMBOL_BYTES_LIMIT` values;
- a ceiling already below current usage rejects admission, and crossing either
  admitted ceiling later terminates with an explicit resource-limit diagnostic
  because frozen `IdApi.intern` has no recoverable failure carrier.

The standard in-tree native stack exports the declaration. The floor begins in
compatibility mode during finite bootstrap and the host enables compaction as
soon as the first audited module is admitted. Once one undeclared process-life
module is initialized, that `LoadedStack` never re-enables compaction.

## 5. Store retention compaction

Store owns temporal meaning. GC must never decide which facts are old enough to
forget.

The Store maintains a durable monotonically increasing basis and an
`oldestRetainedBasis`. Compaction constructs a replacement fact vector before
publishing it. For each entity/attribute relation it retains:

- the baseline state needed to answer the first retained basis;
- every history-enabled transition within the retained window;
- only the current projection for history-disabled attributes;
- schema state required to interpret every retained transition.

The replacement preserves current query results and exact `asOf:` results at or
above the retention floor. An older request answers `historyNotRetained`.
Compaction never resets the durable basis or fabricates an earlier value.

History is **prospective**, not a current schema reinterpretation. Every fact
records whether its relation was history-enabled when that transition
committed. A transition of `cardinality` or `history` schema is always
historied. Turning history off affects later transitions only; it neither
retags nor deletes earlier history. A history-disabled relation maintains a
non-historied current projection which shadows, but does not destructively
rewrite, a retained historied predecessor. Compaction keeps the projection
needed at a later retained history-enable boundary. A temporal request crossing
an unavailable history-disabled interval answers `historyNotRetained`; it never
exposes the older predecessor as a plausible current value.

The first production policy set is deliberately small:

- `current-only` — the default; collapse to the current projection;
- `keep-since <basis>` — retain the exact suffix from one durable basis;
- `keep-last <count>` — derive a moving basis floor from a bounded number of
  committed Store transitions.

A later first-class snapshot value may pin a floor, but an untracked SmallInt
returned by today's `basis` protocol does not implicitly pin history. A caller
that needs a basis to remain queryable must install an explicit retention
policy before compaction.

Compaction is transactional. Allocation or validation failure leaves the old
fact vector and floor authoritative. After the replacement is published, its
dropped Oops cease to be roots and the following GC may reclaim their object,
vtable, and Symbol identities.

## 6. Image and journal behavior

`System saveImage:` is a compaction boundary, as Design 40 requires. It writes
the current projection and the configured retained history window, not dead
process indexes or retired code generations. The MOP mirror remains a rebuilt
cache and is never serialized independently.

An image load rebuilds weak symbol and vtable indexes from durable roots. It does
not recreate identities that existed only in discarded history. Resumable
records name stable implementation keys and manifest entries, never prior
process vtable indices or Oop addresses.

Incremental journal growth is separately bounded by periodic canonical image
compaction. Atomic replacement keeps the previous complete image recoverable
until the new compacted image is durable.

New canonical saves use **image-v6**. The outer authority, `Resumables`, Heap,
and Code records remain byte-identical to image-v5. Version 6 changes only the
header version and Facts payload:

| Offset | Width | Field |
|---:|---:|---|
| `0x00` | 8 | durable next-transaction basis, little-endian `u64` |
| `0x08` | 8 | `oldestRetainedBasis`, little-endian `u64` |
| `0x10` | 1 | retention policy: `0` current-only, `1` keep-since, `2` keep-last |
| `0x11` | 7 | zero, reserved |
| `0x18` | 8 | policy argument; zero for current-only |
| `0x20` | 4 | fact count, little-endian `u32` |

Each fact is `historied:u8`, `operation:u8`, `transaction:u64`, then the three
existing tagged fields for entity, attribute, and value. Boolean and operation
bytes are restricted to `0..=1`; `oldestRetainedBasis <= basis`; keep-since may
not name a future basis. The loader continues to admit frozen image-v5 bytes,
loads every v5 fact conservatively as historied, and assigns floor zero with
the current-only policy. It never reinterprets v5 bytes as v6.

## 7. Triggering, failure, and diagnostics

Compaction begins only at a Runtime safe point:

- no Machine is mid-step;
- no native/foreign callback owns an unreported Oop or borrowed Symbol name;
- code publication is quiescent;
- Store mutation is outside a transaction;
- GC and finalization are not already active.

The initial surface is explicit and image-save driven. Automatic Store
compaction is additionally growth-gated by
`BIRNAM_STORE_COMPACTION_TRANSITIONS` (default `4096`) and runs immediately
before the next semantic Store mutation. `BIRNAM_STORE_FACT_BYTES_LIMIT`
(default 64 MiB) bounds semantic fact storage; the rebuilt MOP mirror is live
derived state and is excluded. A mutation first attempts policy-governed
compaction, then answers an ordinary `historyResourceLimit` Error without
changing state if one more fact still cannot fit.

Store-backed Cells and Globals propagate that Error. Generated persistent
global writes compare the answer from `birnam_global_set` with the requested
value and return the Error to candidate activation, which rejects the candidate
and enters compensation. Resource failure is therefore language-visible and
recoverable; it is neither source-definition metadata nor a fatal native side
channel.

Candidate initializers reserve two compensation Fact records before the first
write to each distinct projected global, plus capacity for the forward write.
Two is the exact schema-malleable worst case: rollback may need to retract the
candidate value and reassert its predecessor when the attribute is cardinality
many. Repeated writes to the same key share one reservation and one journal
row. The reservation remains charged to its candidate while rollback is
available, is consumed immediately before compensation transitions, and is
released after rollback, drift, or discard. A failed forward mutation releases
only the reservation added for that write.

Store and Codegen exchange this authority through the host-mediated,
versioned `birnam_store_compensation_api_v1` /
`birnam_codegen_store_compensation_configure_v1` native seam. It is private
process state outside frozen `IdApi`, ordinary source sends, and image bytes.
Read-only Store diagnostics expose `reservedCompensationFactCount` and
`reservedCompensationFactBytes`; reserved records participate in every
semantic-fact capacity check.

Diagnostics report at least:

- live and retired vtable counts and bytes;
- live Symbol count and owned-name bytes;
- current and retained Store fact counts and bytes;
- oldest retained basis and retention policy;
- active, quiescing, retained, and unloaded generation counts and mapped bytes;
- the resource and semantic holder that prevented a requested reclamation.

Hard ceilings cover heap bytes, identity bytes, retained-history bytes, and
mapped-generation bytes. A ceiling failure is recoverable and identifies the
responsible class. The runtime must not continue allocating until the operating
system kills the process.

## 8. Delivery slices

### J0 — Contract and measurement

Freeze this record, add counters without changing lifetime, and publish one
fixed-live-set churn baseline for definitions, Symbols, facts, and generations.

### J1 — Weak dynamic vtables and 60G integration

Make post-bootstrap vtables collectable, trace header/vtable edges, make
enumeration weak, move layout bytes under Runtime ownership, migrate native
caches to explicit process or generation roots, condition block-catalog
ownership on generation admission, and let 60G unload a quiescent formerly
method-bearing generation.

### J2 — Weak Symbols

Audit all `symbolName`/`exportedNameOf` uses, enforce the borrowed-name rule,
make the interner and internal Symbol dictionary weak, and reclaim name bytes
with unreachable Symbol objects. Retain a compatibility-mode hard cap when a
loaded native component cannot admit compaction safety.

Implementation checkpoint (2026-08-24): the floor now has collectable Symbol
objects, a weak Symbol registry, weak MOP dictionary mirrors, explicit export
name/value roots, explicit fixed execution-object roots, and pre-detach pruning
for both dictionary entries and owned name bytes. The Image path copies every
borrowed Symbol/export name, and Codegen copies names before forwarding them to
an IdApi operation that may allocate. The remaining in-tree callers either copy
before such an operation or consume the bytes entirely within a synchronous
non-allocating extent.

Production remains in compatibility mode only until the host admits the audited
module declaration above. Undeclared modules cannot initialize without explicit
count and owned-name-byte ceilings. Test-only direct admission separately proves
the weak sweep without depending on dynamic-loader fixtures.

### J3 — Store retention compactor

Implement the three retention policies, transactional fact-vector rebuilding,
the durable floor, exact retained-window query behavior, and dropped-root GC
integration.

Implementation checkpoint (2026-08-24): Store facts carry prospective history
status; current-only, keep-since, and keep-last policies drive transactional
replacement-vector compaction; the durable floor gates temporal queries; and
dropped fact Oops cease to be Store trace roots. The explicit protocols are
`retainCurrentOnly`, `retainSince:`, `retainLast:`, `compact`,
`oldestRetainedBasis`, and `retainedFactCount`.

### J4 — Image and automatic operation

Compact on image save, rebuild weak indexes on load, add growth-gated safe-point
triggering and resource ceilings, and preserve recoverable image replacement.

Implementation checkpoint (2026-08-24): image save invokes Store compaction and
emits image-v6 retention metadata plus per-fact history status. Mapped, rebuilt,
and stale-authority recovery admit both v5 and v6; v5 facts migrate as historied
and v6 restores the exact floor and policy. Growth-gated automatic triggering,
the semantic-fact byte ceiling, ordinary Error propagation, image admission
against that ceiling, and fact-count/byte/floor/policy diagnostics are
implemented. Candidate-owned projected-global writes now reserve their exact
worst-case rollback capacity before mutation. The production host wires the
private Store/Codegen table before source bootstrap, the reservation remains
charged across the rollback-capable lifetime, and rollback consumes or releases
it without exceeding the semantic-fact ceiling. The image lifecycle smoke
proves the live reservation ledger moves `0 -> 2 -> 0` across successor
activation and rollback.

### J5 — Indefinite-run acceptance

Run bounded-memory churn with a fixed live set for at least:

- repeated replacement of one named method and its generated blocks;
- unique transient Symbols;
- history-enabled writes under each bounded retention policy;
- mapped successor activation, rollback expiry, discard, and unload;
- save, load, continued churn, and resave.

After warm-up and compaction, retained bytes must plateau within the configured
allocator slack. A control that deliberately retains more live data or an
unbounded history policy must grow and attribute that growth correctly.

Implementation checkpoint (2026-08-24): read-only `Codegen` diagnostics now
report `liveHeapBytes`, `liveSymbolCount`, `liveSymbolNameBytes`, and
`liveVtableCount` through floor-private counters outside `IdApi`. Three
successive 5,000-item transient Symbol batches plateaued at 110,816 live heap
bytes, 795 Symbols, 10,112 owned Symbol-name bytes, 474 live vtables, and 4,760
semantic Store bytes after collection. The same fixed-live-set plateau held for
5,000-item clone/vtable churn.

The required live method-replacement control exposed one remaining design
boundary. Three 2,000-item `Reflect defineMethod:on:as:` / `removeMethod:on:`
batches grew live heap bytes 334,248 -> 558,904 -> 782,904, live Symbols 4,796
-> 8,796 -> 12,796, and semantic Store bytes 164,760 -> 324,760 -> 484,760,
while live vtables stayed at 475. The cause is exact: every ordinary reflected
definition creates a unique `__rust_mblk_N` Globals fact and TCC trampoline,
and `LIVE_UNITS` roots that unit for process life even after removal. J5 cannot
close until live reflected definitions gain a bounded, quiescence-aware
implementation or are routed through managed stackless candidates.

[60K — Reified Portable CodeObjects](60k-portable-codeobjects.md) is the
accepted semantic design for that final boundary. It makes the Block/CodeObject and
current CodeBinding the semantic owners, keeps native materializations only
while live or quiescing owners require them, and adds the source-independent
image contract needed to avoid solving process churn by weakening image
durability. [Design 60L](60l-portable-program-and-image-v7-byte-grammar.md)'s
physical-format acceptance and K1-K6 implementation supplied the final
prerequisites for closing this control on 2026-08-27.

Implementation checkpoint (2026-08-27): K6 removed new portable Reflect
operations from the legacy v5/v6 source-replay journal; current image-v7
CodeBindings and their per-binding provenance are now the durable authority.
Three successive 5,001-operation define/replace/remove batches then plateaued
exactly at 171,152 live heap bytes, 888 Symbols, 11,831 owned Symbol-name bytes,
496 live vtables, and 5,120 semantic Store bytes. Each batch returned to zero
current bindings, zero retiring bindings, and zero generated binding bytes.
Three 5,000-item transient-Symbol batches and three batches under each of
current-only, moving keep-since, and keep-last retention also plateaued. A
deliberately fixed keep-since floor grew semantic Store bytes 45,200 -> 85,200
-> 125,200 and attributed the growth to retained history; returning to
current-only compacted it to 5,200. The same executable smoke saved image-v7,
loaded it in a fresh process, continued with another 5,001 reflective
operations plus Symbol and bounded-history churn, resaved, and booted the
successor image.

## 9. Acceptance boundary

The stackless runtime now satisfies the continuous self-hosted evolution gate:
60G proves native mapping reclamation and J5 proves the complete bounded-space
equation. 60H remains responsible for release rollout policy, not missing
runtime mechanism.
