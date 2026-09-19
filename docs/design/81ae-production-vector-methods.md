# 81AE — Vector methods and scoped sorting roots

> **Status:** Implemented, following shared collection value policy in 81AD.

Move the twelve native Vector method bodies and their thirteen bindings from
Collection into Vector. Keep their registration point and selector order. Copy
the sorting root scope into Vector's existing root provider; retain Collection's
cached parent, Map and selector roots at its existing registration point.

The first Collection-selected IdApi remains Vector's method table, and each
operation reads its current callbacks. Installation retains the first three
selectors supplied by Collection. Reinstallation uses the caller's table for
registration while preserving canonical identities, selected table and selector
identity. No extra interning, construction, publication or root registration is
introduced. Layouts, trie storage and persistent allocation order stay unchanged.

The immutable Vector v2 descriptor removes count/at, whose production consumers
are now private Vector methods. It adds method installation and the two native
iteration bodies used directly by Map. Installation returns the native comma
Method supplied to Range; Range retains that first callback. Public Vector
method replacements must not intercept Range concatenation or Map iteration.
The descriptor has eleven function pointers and is 104 bytes on arm64. Remove
v1 exports and migrate all production consumers; retain v1 only in baseline C
fixtures. The host and installer load collection-value before Vector and Range.

Preserve borrowed argument validation, nil/null distinctions, inherited receiver
handling, exact right-hand Vector checks, callback order, and live Integer and
String capabilities. Vector indexOf: uses the shared native value policy.
Sorting still computes every key eagerly and then performs stable insertion
sort, accepting only the selected table's exact true object from each fresh
comparator send. Root copied elements and returned keys throughout callback
reentry and nested sorting, including early-return cleanup. No new behavior or
public selector is added.

Compare extracted bodies mechanically and use the independently installed 81AD
build as the behavioral oracle. Check copied method ownership, callback traces,
malformed arguments, reentrant GC and root cleanup, live dispatch, Map direct
iteration, Range native concatenation, and construction/adoption traces. Verify
a fresh external debug install, source-free compiled module, v8 native Code
image and baked executable. No release validation.

## Validation

- All twelve moved method bodies match 81AD after normalizing whitespace and
  qualifying private storage calls. Existing Vector storage/helper functions
  also match. Thirteen bindings resolve into Vector; new and clone still share
  one implementation. Collection retains no Vector method body or sorting stack.
- The independent C method oracle passes against 81AD, development libraries and
  the fresh installed build with byte-identical callback traces. It exercises
  malformed arities/carriers, nil/null elements, slicing, reversal, concatenation,
  printing, inherited receivers, retained method pointers, iteration early exits,
  direct Map iteration and first-table/selector retention across reinstallation.
- Stable eager-key sorting preserves equal-key order. Key and comparator
  replacements take effect on subsequent sends; non-true comparator objects
  remain false. Nested key callbacks run sorts during precise GC. Exact root
  counts track both scopes, and key finalizers prove roots are released after
  normal completion and null-key early return. Empty/singleton sorts preserve
  receiver identity. Collection and Vector retain their existing root providers.
- Five Vector construction/adoption/reload traces and five Range traces match
  the previous installed build byte-for-byte. Persistent Vector checks reach
  32,801 elements. Range's native Vector callback/reentry trace and the 1,267-case
  shared comparison/hash oracle also match in development and installed runs.
- Descriptor admission rejects null, wrong version/size, all eleven missing
  operations and retargeting; repeated attachment of the same pointer succeeds.
  Vector and Range match their export inventories, relocatable install names
  and system-only linkage. Collection's sole export delta is Vector v1 to v2.
- ABI tests (four), Vector kernel tests (four), host tests (86, one ignored),
  installer tests (four) and ABI/Vector Clippy with warnings denied pass. The
  installed Range oracle passes 648 cases, String passes 410 cases and 18 numeric
  boundaries, and the full stdlib runner passes all 145 assertions.
- A fresh external debug prefix contains 29 native libraries and 28 compiled
  modules. All 1,553 source hashes stayed unchanged during installation; this
  validation record was added afterward.
- Both the previous installed build and the new prefix pass the same dynamic
  startup, source-free compiled module, saved v8 native Code image and baked
  executable checks. These cover persistent versions, aliases, Range/Map/String
  consumers, sorting with GC inside the key callback, iteration and live method
  replacement. The native Vector library is observed during startup, saved-image
  and baked execution. No release validation was run.

Receipts are `.cargo-target/81ae-{check,unit,clippy,storage,range,compare,prefix,
prefix-native,image-baseline,image,stdlib}.log`, plus the per-fixture baseline,
development and installed traces.

## Next boundary

Extract Map representation, canonical objects, HAMT node allocation and native
lookup/update helpers into a Map module. Preserve Collection's construction,
layout and publication order, existing node layouts, persistent sharing,
collision handling and shared value/hash policy. Start with storage and its
actual consumers; keep Map methods and the remaining String split adapter as
separate subsequent extractions. Audit traversal helpers against their callbacks
before choosing which operations cross the first Map boundary.
