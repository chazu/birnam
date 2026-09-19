# 81AC — Range's Vector-producing methods

> **Status:** Implemented, following Vector storage in 81AB.

Move the existing `reversed` and `,` Method bodies from Collection to Range.
Use Range's private bounds reader and the verified immutable Vector storage
dependency. Keep all guards, Integer construction, traversal direction, i128
arithmetic, allocation order and direct native calls. Range concatenation checks
the exact Vector behavior, constructs its prefix and then calls the existing
Vector comma Method directly. Retain that function at initial Range installation
instead of changing the call into an overridable send.

Update Range's immutable construction descriptor to v2: installation takes the
one process-pinned native Vector comma function, and Range owns all eight method
bindings. Remove the now-unused exported bounds operation and production v1
provider/dependency symbols. Retain the independent v1 C definition for historical
binary comparison only. Collection keeps Range's staged construction calls but
no Range method bodies or payload access.

Load the Vector service before Range so its descriptor can be attached before
Range module_init. Neither service constructs family objects at library load;
Collection's interleaved preparation, layout, method installation and publication
stay in their original order. The first selected Collection IdApi, canonical
identities, selectors, root ownership and direct Vector comma function survive
reload. Ordinary operations still observe the selected table's current callbacks.
The empty Range allocator takes the canonical behavior directly so prototype
creation need not fabricate method runtime state before the callback is installed.

Compare copied bodies and construction traces against 81AB. Extend the independent
native oracle for both moved methods: short extreme/descending bounds, inherited
Range receivers, exact Vector recognition, invalid argument carriers, live Integer
and Id callbacks, bounds snapshots, Vector mutation during prefix construction,
reentry/GC, replaced public Vector methods, and reload with another table/callback.
Verify focused debug installed startup, source-free compiled module, v8 native
Code image and baked execution. Keep Id, IdApi, Vector storage and image encoding
unchanged. Commit this move separately; no release validation or new semantics.


## Validation

- Both moved bodies match 81AB apart from private bounds access and the retained
  native Vector comma call. The other six Range bodies, root provider and
  preparation/layout/publication stages are unchanged. Collection has no Range
  Method bodies or bounds access left. The constructor helper now takes only
  the behavior it uses; Integer recognition/read and allocation traces match.
- The independent native oracle compares reversal and concatenation callback
  traces byte-for-byte against the previous installed build. It covers nine
  short/extreme/descending intervals, inherited Range receivers, null and
  inexact Vector arguments, invalid arity/carriers, current Integer construction,
  bounds mutation at allocation, argument-carrier mutation during prefix
  construction, the second exact Vector check, GC and replaced public Vector
  methods. Both methods preserve direct native calls with no additional sends.
- Reinitializing through another IdApi retains the first selected table.
  Reinstalling Range with a different native comma callback retains the initial
  callback too. The C v2 admission checks reject null, wrong version/size and
  every missing operation before attachment, and reject descriptor retargeting
  afterward. Native address inspection locates all eight Range bindings in the
  Range library.
- Range's five construction/adoption modes match 81AB foreign-callback traces
  through initialization and two reloads, both in development and the installed
  prefix. Its existing 648 boundary/index cases and GC/iteration/printing/root
  checks pass. Installed Vector persistence through 32,801 elements and direct
  String/Map/Range consumers retain the 81AB construction/allocation trace.
- ABI tests (four), host tests (86, one ignored), installer tests (four), and
  Range/ABI Clippy with warnings denied pass. Installed String's 410 byte/index/
  slice cases and 18 numeric boundaries, plus all 145 stdlib assertions, pass.
  Range's six-symbol inventory, relocatable install name and system-only linkage
  match. Collection replaces only its Range v1 dependency symbol with v2; the
  old production provider, bounds operation and dependency are absent.
- A fresh external debug prefix contains 28 native libraries and 28 compiled
  modules. All 1,538 source hashes stayed unchanged during installation; only
  this validation record was edited afterward.
- Both installed Range and Vector image scenarios pass dynamic startup,
  source-free compiled modules, saved v8 native Code images and baked executables.
  They cover captured Range bounds and aliases, persistent Vector predecessors,
  concatenation/reversal, Map/String consumers, stackless iteration with GC,
  and live method replacement. Native module loading is observed. Id, IdApi v7,
  Vector storage and Image source/encoding remain unchanged.

Receipts are `.cargo-target/81ac-{check,range,unit,clippy,prefix,prefix-native,
image,vector-image,stdlib}.log`, the `vector-{baseline,native,installed}` callback
traces, and per-mode Range construction traces. No release validation was run.

## Next boundary

Vector methods are next, but their `indexOf:` shares Collection's private
`same_value` helper with Map key equality. Audit that common comparison and
Map's matching key-hash policy as a small prior extraction so Vector and Map
can consume the same native policy without copying it or routing through public
sends. Preserve identity shortcuts, Integer/Float/String recognition and reads,
Float zero/NaN behavior, Symbol-name borrowing, selected Id callbacks and call
order. Then move Vector's methods and scoped sorting roots at their current
registration point, including ownership of the native comma implementation.
