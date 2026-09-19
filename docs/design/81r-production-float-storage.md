# 81R — Float representation above Id

> **Status:** Implemented, following 81Q.

Float takes ownership of its canonical behavior and zero prototype, eight-byte
payload, construction, reading, exact recognition and native roots. All real
consumers acquire that explicit dependency. IdApi v5 removes `float_vt`,
`mkFloat`, `floatValue` and `isFloat` (41 fields, 328 bytes); historical manifests
remain intact. No Float numeric optimization or language functionality is added.

## Construction and alternatives

Three designs were examined: deferred peer attachment, preparing both numeric
families before activating either, and a live primitive table paired with a pure
storage codec. Deferred attachment after Float initialization leaves Integer
methods unusable if the replaceable owner/export protocols reenter during Float
publication. A general cycle resolver adds machinery this fixed pair does not
need. Prepare Float before Integer instead: Float preparation registers partial
roots and constructs a complete primitive table without installing methods or
sending owner/publication messages. Attach it to the host and combined runtime,
then initialize Integer with Float attached. At Float's existing module slot,
attach Integer and run ordinary `module_init` to publish Float and install its
seventeen methods. Libraries remain pinned from their first callback publication.
Repeated completed initialization preserves replaced methods.

Ordinary consumers copy live operations before calling them. Machine's internal
reads and graph materialization use the immutable pure payload codec: no Id
calls, allocation, collection, reentry or mutable module state. The codec accepts
IEEE bits, preserving NaN payloads and negative zero. Graph construction keeps
its existing allocation/root schedule and bypasses ordinary constructor wrappers
as before. Float has no allocator-context optimizer guard because no existing
Float optimization requires one.

## Invariants and validation

Preserve exact versus inherited recognition at each consumer, fresh boxes, live
method/constructor replacement, Integer conversion bounds, error behavior and
image bytes. Canonical Float identities become module roots, not permanent floor
objects. Per-runtime tests own separate instances rather than using native
process-global dependency slots. Validate bits, distinct runtimes, collection,
constructor replacement and initialization reentry; retain the independent image
oracle. Check current C/Rust ABI geometry and declared exports. Finish with debug
startup, source-free compiled module, saved native-code image and baked image
checks, then commit. No release validation.

## Evidence

Focused debug validation passed:

- 247 Id tests (16 intentionally ignored), including separate-runtime Float
  identities, IEEE payload bits, namespace rebinding and module-root survival.
  The affected native packages passed 172 tests; the explicit fresh-process
  Relational output fixture also passed after mounting both numeric dependencies.
- Float's C oracle passed 676 arithmetic cases, conversions, errors, fresh boxes,
  raw IEEE bits, exact recognition, collection during preparation, Integer
  `asFloat` reentry from owner publication, live constructor/allocator replacement,
  repeated activation and method replacement before/after Prelude. The installed
  debug prefix also passed this probe and the 405-case Integer oracle, numeric
  optimizer-context fixture and closure-context fixture.
- Current C IdApi geometry/client, protected storage observation, independent
  described-graph tracing and live-GC traversal passed. ABI, Float and Integer
  Clippy checks passed with warnings denied. All nine selected/competing/failing
  floor-discovery cases passed. The independent image authority admits only the
  counted Float call substitutions; its seven negative/control tests passed.
- Standalone Id and Float dylibs match their 30- and 5-symbol inventories and
  relocatable install names, linking only the system library. Id has no normal
  dependency on Float; the per-runtime test owner uses a dev dependency.
- A fresh external debug prefix contains 21 native libraries and 28 compiled
  Birnam modules. Source hashes remained unchanged during installation. The
  Float image probe passed dynamic startup, a compiled module after removal of
  its source, a saved v8 image containing native Code and a baked executable.
  The image exercises arithmetic, conversion, alias identity, Float-only equality,
  live method replacement and collection, and verifies dynamic Float loading.

Local logs are `.cargo-target/81r-{unit,consumers,native,prefix-native,clippy,
observation,tracing,live,discovery,relational,prefix,image}.log`; the independent
current manifest is `abi/idapi-v5.json`. No release validation was run.

## Next candidate

Word is the next small representation family. Start with its existing methods
in Prelude, then review the four IdApi fields and raw storage consumers before
moving representation. Pointer-valued operations and Alien/ForeignAddress use
need their own caller/lifetime audit; do not infer that every raw address should
become a Word or merge the two families.
