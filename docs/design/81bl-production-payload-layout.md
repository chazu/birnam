# 81BL — Native payload-layout descriptions

> **Status:** Implemented against completed 81BK (`d7638dea`). Final installed
> startup, compiled, image, baked and portable recovery/resave checks pass.
> Debug validation only.

[81D](81d-id-api-partition.md) places descriptions of user payloads above Id,
shared by GC and Image. Extract the existing owned byte prefix, repeated tail
and exact-vtable selection into `birnam-payload-layout`. Id retains the vtable
registry and the lifetime of each attachment. This introduces no schema,
inheritance rule, language method, collection policy or image encoding.

## Existing laws and ownership

Before extraction, `vtable.rs::VtableRecord` owned a boxed byte prefix, a tail byte and a
presence bit. An unset record owns no allocation. A set record with an empty
prefix is distinct from an unset record and also needs no byte allocation.

The public setter first performs ordinary owner/observation admission. It then
rejects a null prefix with a nonzero count, copies the supplied bytes, looks up
the exact vtable, replaces the old description and synchronizes the raw mirror.
The copy precedes membership lookup. A zero count never reads the supplied
pointer. Every byte value is accepted, including values outside the named slot
kinds. Preserve the exact diagnostics and the absence of a mutation-epoch bump.

`slotKind` admits, requires exact vtable membership, returns `SLOT_UNKNOWN` for
an unset layout, selects the prefix byte for an in-range index and otherwise
returns the tail. It does not inherit the parent's layout. Inherited trace-hook
lookup is a separate operation and stays unchanged.

Internal presence checks return false for an invalid or unset vtable. The
all-Oop extent check also returns false in those cases, tests only the requested
prefix and uses the repeated tail only when the requested extent exceeds the
prefix. A zero-word extent is true for any explicitly set layout, including a
RAW tail. These reads allocate nothing and invoke no language/native callbacks.

The 80-byte raw vtable payload remains unchanged: prefix pointer at offset 48,
count at 56, tail at 64, presence at 65 and owner at 72. Empty prefixes expose a
null pointer. Unrelated dictionary/owner/trace-hook changes resynchronize this
mirror without changing the description. Borrowed bytes remain valid until
replacement or record destruction. Dead collectable records release their
layout before the heap detaches their storage; neither description pointers nor
the module add roots or introduce an address-keyed registry.

## Selected native boundary

The module owns prefix allocation/copy/release and the interpretation of the
description. A plain C view carries pointer, count, tail and presence. A distinct
owned carrier transfers that view's backing bytes exactly once; no Rust `Box`,
`Vec`, private registry or Runtime address crosses the ABI. The unset carrier is
inline and allocation-free. Id stores the carrier alongside the exact registry
record and retains the module's canonical destructor for that record's lifetime.

An inert, immutable factory is attached and pinned before Id initialization.
It creates a public capability from a copied floor descriptor and supplies
canonical copy, destruction and callback-free view interpretation. Attachment
accepts the same pointer again and rejects retargeting. Canonical operations do
not consult the mutable public descriptor. Ordinary siphon mounting publishes
no objects or methods.

The public capability exposes only setting and querying a slot kind. Its floor
descriptor supplies ordinary admission, exact-record replacement and a borrowed
exact-record view. Setting copies the floor descriptor, admits without holding a
state borrow, copies input through the module, then transfers ownership to the
floor replacement callback. The callback performs the existing membership check,
releases the old carrier and synchronizes the raw mirror. Querying admits, takes
the exact-record view and interprets it synchronously. No borrow survives reentry.

Internal construction/GC/recovery/optimizer paths use canonical module operations
at their current call points. They must not acquire public admission during
restricted storage observation or go through instrumentable public callbacks.
Independent and moved Runtime instances retain independent records; the public
floor adapter selects the existing process Runtime (the active Runtime in tests).
The factory and code stay pinned until all carriers and consumers are gone.

## Callers and selection

| Caller | Selection and ordering to preserve |
|---|---|
| Id Machine construction and internal layout operations | Existing Runtime admission and construction points, before public Runtime publication where applicable |
| Integer, Float and Word construction | Snapshot the selected layout operation with the existing API callbacks before `proto`; register partial roots and preserve family adoption/publication ordering |
| String preparation | Read the selected capability at the existing post-`proto` setter point; preserve native allocation and cleanup ownership |
| Range, Vector and Map layout stages | Pass the incoming layout capability explicitly to the stage; retain node/family ordering, adoption behavior and original incoming API selection |
| Store and PEG | Save with the same Runtime used by their existing registration helpers |
| Prelude, Error, String Builder and Isolate | Incoming capability for incoming construction API |
| Artifact | Propagate selection through its construction helper and saved Runtime as required by each existing API use |
| Codegen | Incoming capability for bring-up layouts; first saved execution capability for lazy Block construction |
| TCC | Incoming capability for ForeignAddress family preparation; saved first capability for Unit layout assignment on every initialization and for later construction |
| Portable Code | Save the capability with the same first execution API for later construction |
| Relational | Incoming capability for LogicVar construction; first saved execution selection for later callback families; explicitly migrate its independent native-output fixture |
| ForeignAddress helper and Alien/TCC callers | Pass the matching layout capability through family creation/allocation, preserving live presence/import/export calls |
| Image | Incoming capability for report-family creation; saved execution capability for all four existing slot-query sites |
| Native Loader | Forward the capability saved with the first execution API on every repeated initializer call |

Collector edge tracing retains the current per-object precise/presence read and
per-slot queries. Storage observation retains its callback-free presence check.
Recovery retains exact membership, trace-hook refusal and its special slot-zero
check even for an empty aggregate. Closure-elision, virtual-aggregate,
discarded-clone and unboxed-numeric checks retain their current read points,
short-circuiting and extent arithmetic. No layout snapshot is cached across a
callback or used to skip a later live read.

IdApi v17 removes `setLayout` and `slotKind`: 21 fields, 168 bytes. Native
initialization gains a tenth argument; Native Loader becomes v10. Early String
preparation becomes v4; Float/Word preparation and Range/Vector/Map installation
become v6 where their preparation or layout stage gains the explicit capability.
Live value-operation/codec contracts do not otherwise change. Migrate native
callers, C/Rust fixtures, function-identity checks, exports and current manifests
together. Preserve historical fixtures rather than rewriting their provenance.
No API-address lookup, aggregate facade or production callback-slot loan is used.
Image encoding remains v8; native consumers require rebuilding.

## Independent evidence required

Archive `d7638dea` source and the final 81BK installed debug prefix before
implementation. Compare against its v16 implementation and headers, not a
new-to-new surrogate. Exercise unset/empty/prefix/tail layouts, arbitrary bytes,
large indices, input copying, replacement using the old borrowed prefix, raw
mirror stability after unrelated mutations, exact child noninheritance and
all-Oop extent boundaries. Confirm no mutation-epoch change and release at
collectable-record retirement and Runtime destruction.

Compare owner/observation failures, invalid membership and null-prefix diagnostic
precedence. Instrument early value preparation and repeated initializer selection
without retaining production callback-slot adapters. Test admission reentry,
independent/moved Runtime instances and canonical operation independence from
public descriptor instrumentation. Exercise layout changes during tracing and
image encoding callbacks and retain recovery/optimizer refusal cases.

Verify the actual installed module's function ownership at siphon startup,
source-free compiled execution, saved v8 native-Code image recovery and baked
execution. Run affected ABI/admission, stdlib, GC, recovery and image-backed
worker checks. Record the exact source/artifact relationship and remaining gates
before marking this extraction complete; do not run release validation.

## Completed implementation and focused evidence

The module, floor attachment, explicit caller migration, startup/install wiring,
current ABI manifests and independent C fixtures are implemented. Historical ABI
fixtures remain unchanged. The following debug checks pass:

- Workspace build and all-target check; affected all-feature checks.
- 280 focused Rust tests (13 ABI, 262 Id, 5 Payload Layout), with 15 existing
  timing tests ignored; 74 current C fixture syntax checks and the v17 C client.
- A counted canonical destructor proves one release on borrowed replacement,
  one at dead-record retirement while the heap allocation remains attached,
  and one for the retained record at Runtime teardown before capability disposal.
- Nine exact old/new stdout, stderr and exit-status comparisons against archived
  `d7638dea` source and its installed v16 floor, including diagnostic precedence.
- Nineteen existing native-consumer transcripts and 27 early value/layout
  consumer transcripts match the historical implementation against the final
  installed prefix; error-presence admission and copied numeric API context
  checks also pass. Range/Vector trace identity resets are explained below.
- Twenty-one host factory/capability admission cases pass against both the debug
  build and the fresh installed prefix. The installed v17 C client also passes.
- Fresh final debug installation contains 47 native libraries and 28 authored
  modules. All 2014 source-file hashes remained unchanged through installation
  and were rechecked before this evidence-only documentation update.
- The installed Payload Layout smoke verifies function ownership at startup,
  source-free compiled execution, v8 native-Code image recovery and baked
  execution, including copied prefixes, callback-time layout replacement during
  collection, nested collection, aliases and repeated native loading. A second
  probe replaces the public query callback, changes a later slot during the
  encoding pass, proves rejection of that newly unknown slot, restores the
  description and saves/loads the same Range successfully.
- Installed stdlib: 145 passed, zero failed. `git diff --check` passes.
- Actual old/new image-backed workers agree byte for byte on ready, twelve wire
  values and orderly stop. Floor discovery, all 75 library export deltas and 31
  native manifests pass. Id and Payload Layout link only system libraries.
- Final installed portable-v8 checks pass for boxed identity, nested source-free
  meaning, CodeBinding recovery, ordinary and forced-rebuild entry, baked entry,
  resaving and rejection of live boundary capabilities.
- Nine adjacent installed image suites pass: GC Settings, Root Registry, System
  GC, Method Observation, Native Loader, Namespace, Namespace Enumeration,
  Selector Enumeration and Block Loop. These ran on the original implementation
  prefix; final startup/image, worker and consumer checks were repeated after
  the ABI metadata and TCC selection corrections below.

The independent TCC fixture first passed against the archived v16 implementation
and failed against the initial extraction: a repeated initializer assigned the
Unit layout through the incoming capability instead of the saved execution
capability. The correction restores the original selection. Its transcript now
matches v16, including mutation of the saved callback and recreation of the
ForeignAddress family through the incoming capability. The focused debug gate
runs this regression. The current v17 manifest also names the moved
`PayloadLayoutV1.set.kinds` contract; its descriptor fingerprint is
`97ed7a6b5af6afd36285011148f5a58d1ef19e7bdb9c21f03296ba89e8b835cc`.

Range/Vector trace fixtures retained temporary addresses across precise GC,
occasionally assigning a reclaimed object's old trace number to a new allocation.
The same current binary reproduced the instability. Resetting only the fixture's
trace identities before the next capture phase fixes the comparison without
adding roots or changing runtime execution. The exact same reset is applied to
a separate historical fixture copy; the archived production source and headers
are untouched. The full 27-case comparison and thirty isolated old/new repeats
of each affected mode pass after this correction.

Local receipts are under `.cargo-target/81bl-*`; final installation, source hashes,
consumer comparisons, host/ABI checks, stdlib, floor discovery, inventory, worker
and image receipts use the `81bl-final-` prefix. Earlier independent diagnostics
are in `81bl-layout-oracle.json`; the TCC regression is in
`81bl-tcc-layout-{old-before,new-before,final}.log`. The final installed prefix is
recorded in `.cargo-target/81bl-final-prefix-path`. These are local evidence,
not checked-in artifacts. No release validation was run.
