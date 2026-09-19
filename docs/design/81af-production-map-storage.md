# 81AF — Map storage and native HAMT helpers

> **Status:** Implemented, following complete native Vector ownership in 81AE.

Move Map's two-word payload, variable-sized HAMT nodes, canonical objects,
construction/layout/publication stages, allocation helpers, native lookup and
persistent update into `birnam-map`. Keep Map's thirteen method bindings and
argument/return conventions in Collection. Migrate their actual storage consumers.
No compatibility facade, new public selectors or language behavior is introduced.

Preserve MapVt then MapNode construction, or MapVt/Map/MapNode import order after
Range preparation. Finish the empty Map prototype after Vector and Range; apply
node then Map layouts at the old point. Set the owner and publish MapVt, Map and
MapNode after Range publication, inside Collection's existing Vector-absent
branch without repeating the predicate. Adopt nonnull prototypes unchanged.
The first selected Collection table remains the empty allocator source; supplied
tables drive construction and each lookup/update at the existing callback points.

The immutable native descriptor owns prepare, finish, layout and publication,
empty/update/find, count/root, node slot-count/slot and canonical behavior reads.
Collection keeps traversal helpers for key/value projection, block walking and
printing. They use the pure node readers, preserving the header snapshot, slot
read timing and callbacks. They gain no new recursive iterator or public sends.
These readers can disappear when Map methods move in a later extraction.

Map roots its three identities, including partial construction; Collection drops
those three caches and keeps its parent and four selector roots. Module load
only enables the provider. Load Map after collection-value and before Collection;
its sole native dependency is shared collection value/hash policy. Keep Id,
IdApi v7, Vector, Range, String and image encoding unchanged.

Preserve all bitmap/collision branches, traversal order, shared paths, same-value
identity returns, hash/equality callback order and live Id callbacks. In
particular, copy the existing shared MAP_ADDED flag and the full update sequence:
read root, reset flag, hash and associate, compare roots, read old count, allocate,
then read the flag. Reentrant updates may alter that flag; this extraction must
not replace it with a local result or otherwise change its current behavior.

Compare the copied storage functions and construction/update callback traces
against the independently installed 81AE build. Exercise deep shared paths,
collisions, repeated/identical updates, live callbacks, callback reentry and GC,
partial roots, adoption and reload, plus existing Vector/Range/value-policy
oracles. Validate a fresh debug install, source-free compiled module, saved v8
native Code image and baked executable, then commit. No release validation.

## Validation

- All fifteen storage helper bodies and the complete update sequence match
  81AE after normalizing formatting and the Family name. Collection has no Map
  payload/header definition, allocation helper, association routine or shared
  added flag. Map method entry points remain in Collection.
- Independent C construction traces match the previous installed build in five
  modes: normal, adopted, missing prototype, rebound prototype and Map-only
  exports. The latter still follows the existing Vector export predicate.
  Checks cover allocation ownership, node/payload layouts, explicit word
  initialization, publication reentry, partial roots, live allocator callbacks,
  canonical identities and reload with another table and rebound exports.
- Descriptor checks reject null, wrong version/size, every missing operation and
  retargeting before/after attachment; same-pointer attachment is idempotent.
  Map matches its four-symbol inventory, relocatable install name and system-only
  linkage. Collection replaces only its value-policy dependency with Map.
- The HAMT oracle retains four versions through 4,096 entries, checks every
  lookup, verifies old node contents and unchanged branches, and covers inherited
  receivers, nil values, missing carriers and null key/value rejection. Two real
  Integer keys sharing thirty low hash bits exercise six shared levels. Distinct
  NaNs exercise collision growth, replacement, identical-value returns and
  conversion with a different hash sharing ten low bits. Native node readers,
  key/value projection order and printing agree with the independent layout.
- Baseline, development and installed allocation/hash traces match. Callback
  cases preserve root-before-hash and count-before-allocation snapshots, live
  callbacks with GC, and the shared added flag under a nested overwrite. The
  nested update still changes the outer count exactly as before; no repair or
  local-flag substitution is included. Precise GC preserves retained versions.
- A C trace bookkeeping issue reproduced on both old and new builds: recycled
  addresses could reuse dead temporary object labels after GC. Resetting the
  test's identity registry before precise GC makes the comparison stable without
  changing runtime code. The affected case matched across 75 repeated runs.
- Existing five-mode Vector and Range construction traces, Vector method/sort
  traces, Range native concatenation and the 1,267-case value/hash oracle match
  81AE. Installed checks also pass Vector persistence through 32,801 elements,
  Range's 648 cases, String's 410 cases and 18 numeric boundaries, and all 145
  stdlib assertions. ABI tests (four), host tests (86, one ignored), installer
  tests (four), and ABI/Map Clippy with warnings denied pass.
- A fresh external debug prefix contains 30 native libraries and 28 compiled
  modules. All 1,565 source hashes stayed unchanged during installation. Only
  test trace bookkeeping and this validation record were adjusted afterward.
- The previous installed build and new prefix pass the same dynamic startup,
  source-free compiled module, v8 native Code image and baked executable
  scenario. It covers typed/colliding keys, persistent HAMTs and older versions,
  aliases, fresh equal Strings, key/value projection, live method replacement
  and GC. The new Map dylib is observed during startup, image and baked runs.
  Id, IdApi v7, Vector, Range, String and image source/encoding are unchanged.

Receipts are `.cargo-target/81af-{check,unit,clippy,map,range,compare,prefix,
prefix-native,image-baseline,image,stdlib}.log` plus storage and HAMT baseline,
development and installed traces. No release validation was run.

## Next boundary

Move Map's existing method bodies, traversal and printing helpers into Map.
Install the thirteen bindings at their current Collection point, retain its
first selected table and value/value:value:/printString selectors, and preserve
Vector's native projection/iteration calls. Once the actual consumers move,
remove the now-unneeded external payload/node readers and update the native
construction descriptor. Keep the remaining String split adapter separate.
