# 81AI — Object and Symbol printing defaults

> **Status:** Implemented, following the String split adapter in 81AH.

The remaining Collection implementation is one ordered coordinator: shared
Collection parent adoption/construction, cached selector roots, original
execution-table handoff and interleaved family creation/installation/publication.
Its method and storage bodies have all moved. Keep this unit intact; separating
its single parent identity from the sequence would add a boundary without moving
an independent policy. Continue with Prelude's three printing defaults.

Move Object printString/asString and Symbol printString into `birnam-printing`.
The module depends on String and loads immediately before Prelude. Initialization
only enables an immutable, process-pinned provider; it creates no objects, roots
or methods. Prelude calls its one-operation installer at the exact former
registration point, before Block and System setup. Preserve both selector interns
before the three registrations and their original target/order.

Supply current registration and Prelude's retained original execution table
separately. Retain the execution pointer before the first interning callback can
reenter Prelude initialization. Reinstallation uses current registration callbacks
and targets while methods keep reading the original table's live callbacks.
No native forwarding Method remains in Prelude. IdApi, Symbol interning/storage,
object layouts and image format remain unchanged.

Copy the three bodies verbatim. Object printString ignores its receiver, answers
a fresh String containing anObject for zero arity and nil otherwise. Object
asString interns printString and performs a fresh ordinary send on every call;
return its result unchanged, including null. Symbol printString calls the live
symbolName callback, returns nil for null names, copies the NUL-terminated byte
name with a leading # before calling String make, and preserves arbitrary name
bytes and make results. Do not add exact Symbol recognition, selector caching,
UTF8 conversion, escaping, or a printing/IO policy change.

Use an independent C byte/dispatch oracle with invalid arities and receivers,
empty/binary names, live callback changes, name mutation during construction,
null/alias results, inherited behavior, copied Methods, fresh dispatch and GC.
Compare callback traces with installed 81AH, including first-intern reentry,
reinstallation through a different table and changed registration targets.
Verify a fresh external debug install, source-free compiled module, saved v8
native Code image and baked executable before committing. No release validation.

## Validation

- The three printing bodies and two native send adapters match 81AH after
  normalizing formatting. The complete two-intern/three-registration block is
  unchanged, as are all 39 remaining nongeneric Prelude functions. Each extracted
  Method resolves into Printing; there is no Prelude forwarding body.
- The independent C oracle passes 1,104 byte/receiver cases in both normal startup
  and first-intern reentry. Callback traces match the previous installed 81AH
  build, development libraries and the fresh installed prefix. Cases include
  empty names, non-UTF8 bytes, embedded NUL termination, UTF8, invalid receivers,
  inherited behavior, ignored carriers at zero arity and rejection of nonzero
  arity before any callback.
- The selected nil identity and symbolName/send/String make callbacks remain
  live. Symbol names are copied before a make callback overwrites the borrowed
  buffer. The symbolName callback can supply a name for a null receiver; no new
  exact-type check is introduced. Null and alias results from make/send are
  returned unchanged. Object printing remains fresh under the default constructor.
- asString interns and sends on each call. Replaced printString methods and an
  effectful custom lookup are observed on subsequent calls. Copied native Methods
  survive public replacement. Nested printing and collection from intern, send,
  symbolName, make and dispatched methods pass; a later precise collection
  preserves the explicitly rooted result.
- Reentry from the first printString intern initializes Prelude through a second
  table before outer registration. Calls made from the registration callback
  still see the original execution table. Reinstallation retains that table while
  using the current registration targets and callbacks. Initialization itself
  creates no objects, methods or roots; neither installation nor reload adds a
  root provider or cached Oop.
- Descriptor admission rejects null, wrong version/size, a missing operation and
  retargeting, and accepts repeat attachment of the same pointer. Development and
  installed libraries match the four-symbol inventory, relocatable install name
  and system-only linkage. Prelude's sole export change adds the Printing
  dependency. Symbol storage/interning, IdApi v7 and image encoding are unchanged.
- Focused Printing, Boolean (34 cases), Float (676 arithmetic cases) and Word
  (81 equality cases) scripts pass. The neighboring native fixtures also pass
  against the installed prefix. All 145 stdlib assertions pass. ABI tests (four),
  Prelude tests (two), host tests (86, one ignored), builder tests (four), and
  ABI/Printing Clippy with warnings denied pass. No release validation was run.
- A fresh external debug prefix contains 32 native libraries and 28 compiled
  modules. All 1,591 source hashes stayed unchanged during installation; this
  validation record was added afterward. The install-smoke inventory now includes
  Printing and the preceding Map/String-adapter extractions. Its package list and
  all 60 required libraries were checked against the installed tree; this is an
  inventory check, not a claim that the full install-smoke script was run.
- Both the prior build and new prefix pass dynamic startup, a source-free
  compiled module, a saved v8 native Code image and a baked executable. These
  preserve Object/Symbol printing, UTF8 names with spaces and #, aliases and
  interned identity, previously rendered text, fresh Strings, inherited asString,
  live Object/Symbol printString replacement and GC. Printing is observed loaded
  during new startup, image and baked runs.

Receipts are `.cargo-target/81ai-{check,build,compare,printing,boolean,float,word,
unit,clippy,prefix,prefix-native,image-baseline,image,stdlib}.log`, plus source
hashes and per-mode baseline/development/installed printing traces.

## Next candidate

Extract System's four GC control methods: gc, liveObjects, autoGc: and preciseGc:.
Keep System family creation/adoption and the existing selector-interning and
registration sequence in Prelude. Preserve the original execution table, live
Boolean identity tests, callback order, Integer construction after collector
queries, receiver returns and malformed-argument behavior. The collector itself,
its native attachment and IdApi fields are separate boundaries; this next step
only moves the existing language-facing control methods.
