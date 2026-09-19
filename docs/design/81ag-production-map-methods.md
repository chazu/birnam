# 81AG — Map methods, traversal and printing

> **Status:** Implemented, following Map storage in 81AF.

Move Map's twelve method bodies, thirteen bindings and three traversal/printing
helpers from Collection into Map. Keep installation after Range's methods and
before publication, with the same selector order. Retain the first Collection-
selected IdApi through Map's Family and the first value/value:value:/printString
selectors supplied by Collection. Extend Map's existing root provider to mark
those selectors; keep Collection's parent/selector roots and registration point.

Copy traversal, printing and method bodies, changing only native Map storage
calls to private functions. Preserve header snapshots and later slot read points,
Vector allocation/append order, borrowed carriers, nil/null distinctions and
exact first-table callbacks. Map do:/inject:into: still project values and invoke
Vector's native iteration bodies, without looking up overridden public Vector
methods. Preserve fresh fallback/block/printString lookup, callback reentry,
printing byte-copy order and early returns. Storage and shared added-flag timing
remain unchanged.

All production storage consumers move into Map. Replace the twelve-operation
v1 descriptor with five construction/installation operations: prepare, finish,
layout, publish and install. Remove public native empty/find/update/readers and
behavior access; retain their private implementations. The immutable v2 boundary
is 56 bytes on arm64. No v1 production compatibility layer is retained. Add
Integer, String and Vector dependencies to Map; remove Collection's unused
Integer dependency. Loading order already satisfies these dependencies.

Validate exact copied bodies, thirteen method owners and installation order,
first-table/selector retention, malformed arguments, nil/null fallback and block
results, native Vector dispatch, fresh callback lookup, traversal mutation/reentry
and GC, live String operations and printing failure paths. Compare independent
C traces with the previous installed build, retaining v1 support only in tests.
Run focused debug native checks, a fresh external install, source-free compiled
module, saved v8 native Code image and baked executable, then commit. Keep the
remaining String split adapter separate. No release validation or new behavior.

## Validation

- All twelve method bodies and three traversal/printing helpers match 81AF
  after normalizing formatting and private storage qualification. Every existing
  storage function also matches. All thirteen bindings resolve into Map, with
  new/clone sharing one function; Collection retains no Map method/traversal body.
- The independent method oracle produces identical callback/allocation traces
  on the previous installed build, development libraries and new installed build.
  It checks all method arities and carriers, present/missing fallback behavior,
  live selected true/false objects, live Integer allocation, and first-table and
  selector retention across reinstallation with another table and selectors.
- Existing null distinctions remain: fallback may return null directly;
  do: with a null carrier still projects values and returns the receiver;
  inject:into: with a null carrier still projects before returning nil. Null
  iteration results are ignored by do:/keysAndValuesDo:, while inject:into:
  stops after a null accumulator and returns nil.
- Projection reads the Map root after empty Vector allocation. Traversals retain
  their header snapshot and read later slots at the original points, including
  mutation from allocation or block callbacks. do:/inject:into: traverse the
  already-projected values. Replaced block and printString methods take effect
  on subsequent sends; public Vector and Map values overrides do not intercept
  private projection/iteration. Callbacks reenter projection and collect.
- Printing retains each key/value pair before the key callback, then observes
  changes to later keys. Live String bytes/make callbacks, null printed objects,
  null byte extents, replacement bytes, empty Maps and a null final make result
  preserve the previous outputs and early-return behavior. Map roots its three
  original identities and three first selectors after installation and reload.
- Five construction/adoption traces and the 4,096-entry HAMT oracle match 81AF,
  including deep shared hash paths, collisions, persistent versions and shared
  added-flag reentry. Existing Vector storage/method traces, Range construction
  and native concatenation, and the 1,267-case value/hash oracle also match.
  Installed Range checks pass 648 cases; String passes 410 cases and 18 numeric
  boundaries; Vector persistence reaches 32,801 elements; all 145 stdlib
  assertions pass.
- Map v2 admission rejects null, wrong version/size, all missing operations and
  retargeting, and accepts repeat attachment of the same pointer. The seven-symbol
  inventory, relocatable install name and system-only linkage match development
  and installed libraries. Collection's export delta replaces Map v1 with v2
  and removes Integer attachment. ABI tests (four), host tests (86, one ignored),
  installer tests (four), and ABI/Map Clippy with warnings denied pass.
- A fresh external debug prefix contains 30 native libraries and 28 compiled
  modules. All 1,569 source hashes stayed unchanged during installation; this
  validation record was added afterward.
- The previous and new installed builds pass dynamic startup, a source-free
  compiled module, saved v8 native Code image and baked executable checks. They
  preserve typed/colliding keys, persistent versions, aliases, projections,
  equal freshly constructed Strings, live method replacement and GC. Map is
  observed as a loaded library during startup, image and baked runs. No release
  validation was run. Payloads, layouts, storage semantics and image encoding
  remain unchanged.

Receipts are `.cargo-target/81ag-{check,map,compare,range,unit,clippy,prefix,
prefix-native,image-baseline,image,stdlib}.log`, plus per-fixture baseline,
development and installed traces.

## Next boundary

Extract the remaining String split adapter and its byte-copy helper. Preserve
its Collection-selected table, exact String checks, byte snapshots, empty/invalid
separator behavior, native Vector construction and its registration point before
Vector methods. Audit the late installation boundary: String loads before Vector,
so this must not introduce an early String-to-Vector dependency cycle. Keep the
remaining shared Collection parent/installation coordinator as a separate step.
