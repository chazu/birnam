# 81AA — Range representation and construction

> **Status:** Implemented, following Range methods in 81Z.

Move Range's canonical behavior/prototype, two-RAW-i64 payload, layout,
constructor and Integer `to:` from Collection into `birnam-range`. Remove the
second payload definition and cached Range roots from Collection. Its remaining
`reversed` and `,` implementations obtain bounds through an immutable, pure
Range operation and retain their existing direct Vector calls. IdApi and Image
encoding stay unchanged.

Preserve Collection's construction sequence rather than eagerly creating Range
at library load. Replace the method-only construction descriptor with an explicit
Range descriptor: prepare behavior/adopt identities, finish the default prototype,
apply layout, install methods, publish, and read bounds. Collection invokes each
at the exact former position; it retains no Range identity in its runtime state.
The prepare result is used only for the existing incomplete-export validation.
The descriptor is immutable and process-pinned. Remove the old production
method-only interface; retain its independent historical C definition for the
baseline oracle. This is a family construction dependency, not an aggregate
Collection compatibility facade.

Cold behavior creation remains after Vector behavior and before Map behavior,
with Collection as parent. When Vector is exported, adopt RangeVt then Range in
the original import order, irrespective of Range's own export predicate. Finish
an absent prototype after the Vector prototype and before Map's prototype,
using the selected live ordinary alloc callback for 16 bytes initialized to 0,0.
Existing nonnull prototypes, including rebound values, are retained. Apply the
same two-RAW layout after Vector layout and before Map-node layout.

Install the existing eight Range bindings in order, then Integer `to:` before
Map methods. Reinitializing Collection reapplies layout and reinstalls these
bindings while retaining the initial identities, selectors and selected IdApi.
Under Collection's existing Vector-absent publication branch, associate the
Range owner and publish RangeVt then Range at their original position. Do not
recheck Vector inside the Range publisher: Collection has already exported it
by that point. Range module_init itself only prepares the service.

Range owns its partial and completed identity roots. Register its provider before
preparation and retain each returned identity before the next foreign callback;
add the cached selectors once method installation reaches its original point.
The selected table passed to Collection, not the one used to load the Range
library, continues to govern construction and ordinary methods. `to:` keeps exact
Integer checks, argument guards, left-to-right live value reads, current alloc,
canonical behavior and raw bounds writes. It adds no finalizer or normalization.
The bounds operation reads only the payload and cannot call Id, allocate or GC.

Compare production bodies and foreign-callback order against 81Z. Validate normal
construction, adoption, absent adopted prototype, live alloc/read callbacks,
namespace rebinding, reload, roots, GC and method-code ownership. Run the existing
independent arithmetic/iteration oracle plus focused debug installed startup,
source-free compiled module, captured-Range native Code image and baked execution.
No release validation or new language behavior.

## Validation

- `empty_range` and Integer `to:` match the committed Collection bodies modulo
  formatting. All six existing Range Method bodies and their bounds helpers are
  unchanged. The two remaining Vector-backed methods have exactly one bounds
  access substitution each. Collection retains no Range payload/layout, cached
  identities, constructor or Integer `to:` implementation.
- The independent C construction oracle passes against 81Z, development modules
  and the new installed prefix. Its prototype/import/alloc/intern/layout/method/
  owner/publication traces match byte-for-byte through initial construction and
  two reloads in five modes: fresh construction, adoption, missing adopted
  prototype, rebound nil prototype, and Range-only exports with no Vector.
  Root-provider registration deliberately moves earlier to its owning module;
  the trace compares the existing construction operations, not that registration.
- The oracle verifies partial identity roots and the completed five roots,
  prototype retention, canonical parent behavior, inherited clone, unchanged
  Integer recognition/read ordering, exactly one live 16-byte allocation,
  overwrite of the allocation's initial bytes, no finalizer, GC, alias rebinding,
  and retention of the first selected table across reload with another table.
  Native address inspection places Integer `to:` in the Range library.
- The existing independent Range oracle passes 648 boundary/index cases and
  iteration/printing/reentry tests against both 81Z and this extraction. It also
  rejects null, malformed, missing-operation and retargeted Range dependencies.
- ABI tests (four), Collection tests (four), host tests (86, one ignored), and
  installer tests (four) pass. Range and ABI Clippy pass with warnings denied.
  Development and installed Range match their five-symbol inventory, relocatable
  install name and system-only linkage. Collection replaces its method-only
  dependency symbol with the Range ownership dependency; there is no production
  compatibility interface.
- A fresh external debug prefix contains 27 native libraries and 28 compiled
  modules. All 1,522 source hashes stayed unchanged throughout installation;
  subsequent edits only record validation results.
- Installed StringBuilder's four modes, String's 410 byte/index/slice cases and
  18 numeric boundaries, nil's 30 receiver/method cases, and all 145 stdlib
  assertions pass after this extraction.
- Installed startup, a compiled module after source removal, a saved v8 native
  Code image and a baked executable pass the Range oracle: captured alias
  identity, two RAW bounds, construction, indexing/membership, Vector results,
  stackless iteration, live replacement and GC. Range loading is observed.
  Id, IdApi v7 and Image source/encoding remain unchanged.

Receipts are `.cargo-target/81aa-{check,build,baseline,native,construction,unit,
clippy,prefix,prefix-native,image,stdlib}.log`, with per-mode baseline, development
and installed construction traces. No release validation was run.

## Next boundary

Extract Vector storage and its native helpers next. String splitting, Map's
key/value projections and the two remaining Range methods all call `empty_vector`
and `vector_push`; Range concatenation also calls the Vector Method directly.
Vector's node representation, canonical identities, layouts and pure access
operations should move with their implementation before those consumers move.
Preserve Collection's shared parent and the interleaved construction/publication
schedule through explicit stages, as for Range. Keep path sharing, RAW/OOP
layouts, selected allocation callbacks, sorting roots and direct native access;
do not substitute overridable Vector sends for private helper calls.
