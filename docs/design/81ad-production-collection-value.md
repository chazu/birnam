# 81AD — Shared collection value comparison and hashing

> **Status:** Implemented, following complete Range ownership.

Move Collection's private `same_value`, `map_key_hash`, `map_mix` and
`map_fnv1a` into `birnam-collection-value`. Vector `indexOf:` and Map lookup/update
consume one immutable native comparison/hash descriptor. Remove the redundant
Map equality forwarding helper and migrate its actual callers. These operations
remain private collection policy; add no language selectors or general equality
semantics.

The service owns no Oops or layouts and performs no construction, registration,
publication or allocation at module load. Pass the caller's selected IdApi with
each operation, preserving live vtable and Symbol-name callbacks. Attach the
same mutable Integer, Float and String operation tables; read their current
callbacks at the original points. Calls are synchronous and may reenter through
those callbacks. Retain no incoming pointer or borrowed byte/name extent.

Preserve identity and null shortcuts, ordered exact Integer/Float checks and
reads, String behavior/byte comparisons, identity-only fallback equality, and
the existing hash policy: mixed Integer bits, full String bytes, NUL-terminated
Symbol spelling, normalized Float zero with other IEEE bits retained, and mixed
object addresses. NaN equality still succeeds only through identical object
identity; no public `=` or hash send is introduced. Preserve short-circuit order,
empty/null String hash cases, Symbol null handling, and borrowed data lifetimes.

Load the service before Collection, after its numeric/String dependencies. Its
immutable 32-byte descriptor contains version/size and two native operations.
Collection retains Integer and String dependencies used by its methods, while
Float now belongs solely to the extracted policy. Id, IdApi, Vector storage,
Range and Image do not change.

Compare exact copied functions and native consumer callback traces against 81AC.
Use an independent scalar/hash oracle, including integer limits, Float zero and
NaN, byte strings, symbols, nulls, identity/inheritance, callback mutation and
reentry, GC, Map persistence and Vector indexing. Verify a fresh debug install,
source-free compiled module, v8 native Code image and baked executable before
committing. No release validation or new behavior.


## Validation

- All four extracted function bodies match 81AC exactly apart from formatting.
  Vector indexing and every Map lookup/update call use the native dependency.
  Collection retains no duplicate policy functions or equality forwarding helper;
  its unused Float dependency is replaced by the collection-value dependency.
- The independent C oracle passes 1,267 cases across 35 representative values,
  including every pair in the equality matrix and every complete 32-bit hash.
  Test-only collision records expose the hash through ordinary Map lookup, so
  the same oracle verifies the previous installed build without accessing its
  private Rust functions. Development and installed callback traces match 81AC
  byte-for-byte. The new descriptor's direct results also match the oracle.
- Cases include identity/null shortcuts, Integer limits, distinct numeric types,
  signed Float zero, infinities and NaNs, empty/UTF-8/binary Strings, Symbols,
  ordinary objects and inherited numeric receivers. Public equality/hash methods
  are replaced with failing sentinels without intercepting the native policy.
- Live Integer/Float reads and String bytes are observed; null/empty String hash
  extents and null Symbol names retain their old handling. String callbacks
  reenter Vector indexing and collect. Changed Symbol behavior/name callbacks
  in the selected table are observed, including alternate per-operation tables
  through the descriptor. Reload retains Collection's original selected table.
- Real Map updates preserve older versions, equal-valued keys, and unequal keys
  sharing a hash (Integer/Float zero and Symbol/String spelling). Explicit roots
  preserve test inputs and persistent versions across precise GC. The new service
  registers no root provider or family objects.
- Descriptor admission rejects null, wrong version/size and missing operations
  before attachment, and retargeting afterward. Development and installed
  libraries match the six-symbol inventory, relocatable install name and
  system-only linkage. Collection's sole symbol delta replaces Float attachment
  with collection-value attachment.
- ABI tests (four), host tests (86, one ignored), installer tests (four), and
  collection-value/ABI Clippy with warnings denied pass. Range's existing native
  checks pass; installed Range's 648 cases, String's 410 cases and 18 numeric
  boundaries, unchanged Range/Vector construction traces, Vector persistence
  through 32,801 elements, and all 145 stdlib assertions pass.
- A fresh external debug prefix contains 29 native libraries and 28 compiled
  modules. All 1,549 source hashes stayed unchanged during installation; only
  this validation record was edited afterward.
- The previous and new installed builds pass the same dynamic-startup,
  source-free compiled-module, v8 native Code image and baked-executable scenario.
  It covers typed Integer/Float/Symbol/String keys, signed zero, UTF-8, freshly
  constructed equal Strings, identity aliases, persistent Map updates, Vector
  indexing, live method replacement and GC. The new library is observed during
  startup, image and baked execution. Id, IdApi v7, Vector storage, Range and
  Image source/encoding remain unchanged.

Receipts are `.cargo-target/81ad-{check,range,unit,clippy,prefix,prefix-native,
image-baseline,image,stdlib}.log` and `value-{baseline,native,installed}.log`.
No release validation was run.

## Next boundary

Move Vector's existing methods and scoped sorting roots into Vector. Install
methods at the current Collection registration point, preserving the shared
parent, first selected table, selectors and fresh block/comparator dispatch.
Use the extracted value policy for `indexOf:` and retain all temporary element
and key roots across callback reentry. Supply the native comma implementation
from Vector to Range without changing Range's direct-call behavior. Keep this
method/root move separate from Map storage and String splitting.
