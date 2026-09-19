# 81T — Word representation above Id

> **Status:** Implemented, following the Word method extraction in 81S.

Word takes ownership of its canonical behavior and zero prototype, eight-byte
unsigned payload, constructor, reader, exact recognizer and native roots. Migrate
Id's remaining Machine consumers, TCC and Image to the explicit Word dependency;
remove `word_vt`, `mkWord`, `wordValue` and `isWord` from IdApi. The current v6
table has 37 fields / 296 bytes. Historical v5 evidence remains unchanged.

Use the established primitive-table/pure-codec boundary from Integer and Float.
Ordinary clients copy current operations; internal pure payload reads use the
immutable codec. There is no Word optimizer declaration or allocator-context
guard. ForeignAddress remains a distinct family and does not acquire Word.
Preserve TCC's generated-code symbol names and unsigned-value recognition, Image's
inherited recognition and boxed Word bytes, Machine's frame identity values,
fresh boxes, exact recognition, and the three existing Word methods.

Word has no numeric dependency cycle. It still needs preparation before ordinary
publication: a replaceable owner/export callback can reenter Machine's Word
consumer. The host therefore calls Word's preparation entry, attaches its complete
primitive table to the host and combined runtime, then invokes `module_init` with
Integer attached. Prepare Word before Float and Integer initialization too, so
their publication callbacks retain access to the existing Word consumers. Word
activation remains at its existing slot after Float. Preparation registers partial
roots before creating either identity. Owner association, exports and methods follow only after primitive
attachment. This uses the same fixed lifecycle as Float without a cycle resolver.
Every published callback's native library remains pinned, including on failure.
Repeated initialization preserves canonical identities and namespace rebinding
but reinstalls the three methods, matching the method-only module.

Review checks: no raw Word representation in Id's Values/bootstrap; no retained
IdApi compatibility fields; no ordinary callback while holding RuntimeState;
no replacement of inherited Image recognition with exact recognition; no change
to ForeignAddress; per-runtime test instances rather than shared native state;
module roots survive namespace rebinding. Update current native fixtures to mount
the Word primitives before calling consumers. Numeric module enumeration follows
module construction, as with the previous representation extractions.

Validate unsigned boundary bits, distinct runtimes, partial-root collection,
publication reentry and live operation/allocator replacement. Retain the frozen
image oracle with only counted call substitutions. Run focused debug startup,
source-free compiled module, saved native-code image and baked-image checks, then
commit. No release validation or new functionality.

## Validation

Focused debug checks passed:

- 248 Id tests (16 ignored), including separate runtime identities, namespace
  rebinding and root survival, and a live constructor wrapper that collects and
  reenters the runtime. The Word bits codec, four ABI tests, 86 host tests (one
  ignored), four builder tests, 11 Image tests and one TCC test also passed.
- The independent C Word oracle passes 81 equality pairs, unsigned boundary
  printing/conversion, raw payload and codec canaries, lazy errors, exact versus
  inherited recognition, preparation GC, publication reentry, live operations,
  reinitialization and replacement before/after Prelude. It passes against both
  development dylibs and the installed debug prefix. The prefix also passes
  Float's 676-case oracle, Integer's 405-case oracle, numeric API-context and
  closure API-context fixtures.
- The independent IdApi v6 C client passes with 37 fields. Storage observation,
  described-graph tracing, live collection and all nine floor-discovery cases
  pass. The frozen image authority admits only counted Word call substitutions;
  all eight controls pass. ABI and Word Clippy pass with warnings denied.
- Standalone Id and Word dylibs match their 31- and five-symbol inventories and
  relocatable install names. The installed Word dylib also matches and links
  only the system library. Id has no normal dependency on Word; its test owner
  uses a dev dependency. Historical v5 manifests and fixtures are unchanged.
- A fresh external debug prefix contains 22 native libraries and 28 compiled
  Birnam modules. All 1,461 source hashes remain unchanged through installation.
  Startup, a compiled module with its source removed, a saved v8 image containing
  native Code and a baked executable all pass. Both image modes exercise Word
  printing, conversion, alias identity, Word-only equality, live dispatch and
  collection, and confirm dynamic Word loading.

Local logs are `.cargo-target/81t-{unit,consumers,native,clippy,observation,
tracing,live,discovery,prefix,prefix-native,image}.log`. No release validation ran.

## Next candidate

Boolean's existing methods are a small independent extraction from Prelude.
They need object-model's private-behavior operation for False, but no new native
representation interface. Keep the singleton objects in Id for that checkpoint;
removing their shared identity fields requires its own boot and caller audit.
String ownership and cleanup remain a separate representation/lifetime task.
