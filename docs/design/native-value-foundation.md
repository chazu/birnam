# Native value foundation

The agreed split extends Word, separates Bytes from String, and puts explicit
native operand adaptation in Execution. Id remains the dispatcher. There is no
new omnibus primitive service or duplicate byte-memory prototype.

| Owner | Responsibility | Current implementation |
| --- | --- | --- |
| Word | Owned unsigned 64-bit words, wrapping arithmetic, division/remainder, bit operations, logical shifts and unsigned comparison | Rust |
| Bytes | Owned buffers, bounded access and slicing, append, hashing, generic caller-schema framing and indexed views | Rust |
| Execution | Explicit Object/Word/Bytes carrier conversion and synchronous calls through current Id messages | Rust |
| native-values | Word range conversion and hexadecimal printing; existing Boolean/Integer/Bytes conversion policy | Birnam |
| Integer | Signed checked arithmetic and language numeric behavior | Rust cold seed; opt-in Birnam arithmetic pilot |
| String/StringBuilder | Text behavior over Bytes references; neither owns byte vectors | Rust; full port remains |
| Boolean | Canonical Boolean policy | Birnam production policy; Rust cold-bootstrap seed retained |
| Vector, Map | Persistent collection algorithms | Rust; rewrites remain |

This checkpoint implements the owner split and production Boolean policy.
Integer, String, Vector and Map still have production rewrites remaining.
Rust required for a clean cold bootstrap is retained explicitly; Rust
that is no longer needed for that bootstrap can be removed. A production Birnam
implementation and a retained Rust bootstrap implementation are tracked separately.
This supersedes the earlier crate-deletion criterion.
Incoming adaptation is now implemented. Execution and PortableCode initialize
before Integer, String, Float, Vector and Map. Compiler capture construction uses
ordinary Object Model reference slots, so it no longer requires Vector or Integer.
The remaining bootstrap work is admitting the first compiled value definitions
before the compiler/Store/reflection services that currently use those values.
The five native value crates are still required by that later startup stage.

## Primitive contracts

Word initializes after Object Model, before Integer and String. It needs neither
of those factories. `fromBits:` and `bits` retain their native Word carrier
contracts. The new arithmetic methods take Word Objects and return fresh Word
Objects. Addition, subtraction and multiplication wrap modulo 2^64. Division
and remainder reject zero with INVALID_REQUEST. Logical shifts at least 64
produce zero. Integer's signed overflow policy is not moved into Word.
`asInteger` and `printString` are now authored methods.

Bytes has its own standard descriptor, state, behavior and existing prototype
name. Construction copies native input before callbacks. Owned-buffer maps are
authoritative; diagnostic payload mirrors do not grant access to foreign storage.
Buffer keys are weak and GC finalization removes owned storage. Tagged views
retain Rust byte snapshots and scalar framing metadata, never language Objects.
The existing bulk algorithms are moved without rewriting their loops.
`byteAtWord:` and `sliceFromWord:toWord:` use raw Word indices and return Unit
when out of bounds. `appendBytes:` copies its borrowed input before callbacks.
String and StringBuilder trace their Bytes reference through their own GC owner.

## Explicit outgoing ABI

```birnam
let bits := [Execution wordResultOf: 42 selector: #word];
[Execution sendNativeTo: Integer selector: #fromWord: arguments: #[bits]
  signature: "ow" resultFactory: Word]
```

`sendNativeTo:selector:arguments:signature:resultFactory:` takes a signature
whose first byte describes the result and remaining bytes describe arguments:

- `o`: Object; nullable Object results become canonical nil.
- `w`: native Word. Arguments are obtained by the value's current `bits` method;
  results go to the explicit result factory's current `fromBits:` method.
- `W`: optional native Word result; Unit becomes nil, otherwise it boxes like `w`.
- `b`: native Bytes. Argument borrows are copied and owned through the target
  callback; result borrows are copied immediately before `fromBytes:`.
- `u`: require a Unit result, represented as language nil.
- `x`: discard a successful result, represented as language nil.

The argument sequence supplies `sizeWord` and `atWord:` through Id (Vector
implements these messages). Enumeration does not box Integer indices. The
signature must match its arity, at most 128 arguments. Every Object
is rooted through the call using Execution's existing scoped roots. No mutable
owner-state borrow crosses a callback. Native failures retain their status/detail
at this boundary. Source's existing failure handling remains in force.
Zero-argument `wordResultOf:selector:` and `bytesResultOf:selector:` use the
current Namespace Word/Bytes factory. No foreign payload is read and no foreign
method entry is cached. Ordinary source sends retain their existing conventions.

Synchronous source bodies retrieve captures through Execution's typed
`Block captureWord:of:` message. This removes Integer construction from ABI
bookkeeping, while preserving Block ownership and current Id lookup.

The authored `nativeValues` bootstrap artifact contains the displaced conversion
policy. Its bodies are synchronous. It loads after Lang and before SystemGC, Printing and NativeLoader.
Changing storage ownership requires fresh images; old installed artifacts and
images are not acceptance evidence for this split.

## Explicit incoming ABI

```birnam
let nativeBody := [Execution block: [:self :bits | bits]
  withNativeSignature: "ww"];
[Reflect defineMethod: #echoWord: on: SomeObject as: nativeBody];
```

A signature is copied into immutable CodeObject metadata. Normal methods have no
signature and keep their existing calling convention. The first byte describes
the native result; remaining bytes describe native arguments. `o`, `w` and `b`
mean Object, Word and Bytes. `u` requires a language nil result and returns native
Unit; `x` discards a successful result. `W` and `B` allow language nil to become
native Unit instead of a Word/Bytes result. The signature includes exactly the
body's parameters excluding its first receiver parameter, with a 128-argument
limit. Invalid metadata is rejected before method installation.

Execution copies every incoming byte borrow before any constructor or lookup
callback. It wraps Words and Bytes through the current Namespace factories and
passes only Objects to the authored body. Results are unboxed through current
`bits`/`bytes` messages. Byte results are copied to Execution's single owned
return buffer; native callers must copy the borrow before sending another message.
This keeps returned storage alive after the temporary language result is unrooted.
No foreign layout or cached foreign method is used.

`block:withNativeSignature:` materializes a synchronous body when needed. Named
method adapters can still use Execution's registered frames, but typed bindings
always enter the carrier adapter rather than the engine's direct/inlined entry.
Capturing a Method preserves its signature. CodeObject `withProgram:` and
`withoutSourceProvenance` preserve it. The getter returns a fresh Bytes snapshot.

Images preserve this metadata as an optional third CodeObject field. Existing
two-field records remain valid. ImageFormat validates signature grammar and arity
before native code mapping; Image restores the metadata before preparing bindings.

## Bootstrap progress

Object Model's `allocateReferenceSlots:` accepts a native Word count and makes
ordinary, typed reference storage. Generated synchronous and resumable code fills
those slots via `slotAt:in:put:` and constructs a Block with
`fromMethod:codeObject:referenceSlots:`. Execution reads them through Object Model
messages, validates that every slot is a reference, and snapshots/roots all values.
This is capture storage, not a second collection implementation.

Execution and PortableCode look up higher-level factories only when an operation
needs them. Their constructors require none of the five remaining value owners;
a production load hook verifies all five bindings are absent at initialization.
This breaks part of the bootstrap cycle. The remaining compiler/reflection startup
still uses the native value crates and has not been replaced.

Incoming/early-bootstrap qualification is recorded separately below.

## Validation

Passed: workspace compilation; 76 compiler/backend/primitive unit tests; twelve
runtime checks across eleven integration binaries (ABI, numeric constructors,
numbers, collections, SystemGC, tagged tables, text/errors, compiler, Store,
method observation and native loading); all 39 native boundary audits; exact
installed receipt; fresh startup/tool images; source-free compiled loading,
mapped/rebuilt images and a baked executable; REPL arithmetic, FizzBuzz and
collections. The REPL session took 6.4 seconds. No exhaustive Image GC or complete
authored-language suite was run. The installed user prefix is unchanged.

Acceptance prefix: `/tmp/birnam-foundation/final-prefix`. Logs are in
`/tmp/birnam-foundation`: `final-units.log`, `final-install.log`,
`final-focused.log`, `text-final.log`, `loader-final.log`, `image-smoke.log`,
`audit.log` and `receipt.log`. `text-final.log` supersedes the last allocation-count
assertion failure in `final-focused.log`.

Constructor stress remains enabled through all native value constructors and
extensions, then returns to the ordinary GC budget before authored-tool startup.
This keeps the constructor regression focused instead of collecting an expanding
compiler heap after every allocation. The same probe exposed an older Range issue:
its authored Integer `to:` installation overwrote a replacement from the numeric
floor. Range now preserves that existing method on first initialization; explicit
recompilation still installs the module's new definition.

Text allocation diagnostics count both each String wrapper and its Bytes backing.
System count diagnostics include real Word-carrier and argument allocations; they
do not promise the removed native helper's allocation graph.

### Incoming ABI qualification

The production incoming-call regression passes: early initialization without the
five value owners, full Word bits, nullable results, exact arity/type rejection,
collecting constructors, copying all byte arguments before a callback mutates the
original buffer, captured Method aliases and immutable signature snapshots.
Source-free mapped and rebuilt images also preserve Word/Bytes/nullable signatures
and an alias made by capturing the original Method. Fresh-prefix qualification passes: 77 ABI/compiler/backend unit tests, nine
focused runtime binaries, exact installed receipt and all 39 native boundary
audits. The executable Machine fixture also passes in the diagnostic library
copy. Fresh startup/tool images and the permanent mapped/rebuilt image smoke
pass. No exhaustive whole-language or Image GC suite was run. No commit or
user-prefix replacement was performed.

Acceptance prefix: `/tmp/birnam-incoming/final-prefix`. Evidence is in
`/tmp/birnam-incoming/{units.log,install.log,final-focused.log,image-smoke.log}`.
`owner-focused.log` records the executable Machine fixture's pass. The earlier
hybrid-prefix image rejection restored an older startup image; regenerating the
prefix is necessary when changing authored ImageFormat admission methods.

Lazy factory lookup required strengthening Integer's `canOmitIndexing` proof:
only its canonical factory may authorize omission. The discarded-clone fallback
regression passes after this change. The materialization fixture now observes the
actual `captureWord:of:` interface; the old Integer-indexed capture operation is
still available to language callers.

## Boolean slice

Checkpoint commit: `089a5a99` contains the foundation and incoming ABI work.
`modules/boolean.birnam` is the next slice. It implements canonical Boolean
`not`, eager `&`/`|`, chosen-branch conditionals, lazy `and:`/`or:` and printing.
It keeps the existing identities and the native Rust module used by cold bootstrap.
Its compiled artifact is `booleanValues`, loaded after `nativeValues`. The Rust
seed is not removed or relabeled as an authored implementation.

Word adds generic `sameObject:with:` (a 0/1 Word over opaque identities) and
`selectIfZero:ifNonzero:` (select an Object without evaluating it). These provide
the lower-level branch choice needed to define Boolean conditionals themselves.
All selection and callback calls remain Id messages.

The incoming ABI adds required non-null Object arguments (`O`) and deferred
results (`v`). A deferred authored selector returns its chosen thunk; Execution
sends its current `value` method after the selector body returns. This preserves
all native result carriers and exact failures, including Unit, Word, Bytes and
NativeEntry, without adding Boolean-specific behavior to Execution or Word.
Signatures remain CodeObject metadata and use the existing image recovery path.

The direct compilation/probe and fresh cold-seed build pass. Production
qualification passes for authored Boolean truth tables, identity/invalid receivers,
strict arity/null inputs, chosen-only callbacks, all native result carriers and
exact failures, collecting callbacks, Module reloads, incoming ABI, BlockLoop,
compiler/deoptimization, numeric loading, SystemGC and text/Error ownership.
Fresh startup/tool images and source-free mapped/rebuilt images pass. REPL
arithmetic completes in 5.9 seconds. No exhaustive suite was run.

The BlockLoop fixture retained an unrooted raw Error method entry across replacement
and collection. Source methods have collectible contexts, so the fixture now keeps
its corresponding Method until restoration. The debugger reproduction and original
BlockLoop check both pass after that correction.

Acceptance prefix: `/tmp/birnam-boolean/final-prefix`; logs are in
`/tmp/birnam-boolean/{units.log,install.log,other-tests.log,loop-compiler.log,image.log}`.
The user prefix is unchanged. Boolean is committed as `804de64a`.

## Integer arithmetic slice

Boolean checkpoint commit: `804de64a`. `modules/integer.birnam` now defines signed
checked `+`, `-`, `*`, `/` and `mod:` policy with generic Word operations. Addition
and subtraction test signed overflow; multiplication bounds unsigned magnitudes;
division truncates toward zero, remainder follows the dividend's sign, and the
MIN / -1 overflow and MIN mod -1 zero cases are explicit. It preserves exact
Error-family propagation and Float dispatch.

Integer's existing native representation, constructors, conversion, comparison,
printing, collection hashing and optimization support remain required by the
cold bootstrap. They are retained, not renamed. This is an arithmetic-policy
slice, not a completed rewrite of all Integer responsibilities. Public arithmetic
installation consults the seed owner's `hasBootstrapArithmetic:` proof, so a
replacement installed while native dependencies load is preserved. Optional
Word results are now available at the outgoing Execution boundary as well.

The arithmetic boundary matrix passes all 500 cases, plus exact Error propagation,
mixed Float addition, repeated module loading and collecting execution. All native
representation accesses and calls go through current Id messages; there are no
foreign payload reads or retained foreign executable entries.

Default admission is blocked by performance qualification: in separate runs the
numbers fixture rose from roughly 1.2 seconds to 93 seconds with authored arithmetic enabled, and
a fresh installation failed to publish its tools image after a long preparation.
The image failure was the compiler's aggregate generated-source limit, now
removed and verified against the failing tools snapshot (see below).
Therefore the installer
compiles and publishes `integerValues` as an **optional** artifact, records it in
`optional_modules`, and excludes it from default bootstrap and tool images.
Explicit admission is `[Module load: #integerValues]`. Normal startup retains the
native arithmetic seed. This is a correctness-qualified pilot, not a completed
production Integer migration. Further source/compiler optimization and performance
qualification are required before default admission.

No bootstrap-required Rust is removed. The native owner adds only a seed-method
identity proof used to preserve replacements during source policy admission.

The optional configuration qualifies in a fresh installation at
`/tmp/birnam-integer-optional/final-prefix`: both default startup/tool images,
console/CLI startup probes, exact install receipt, workspace compilation and four
installer tests pass. Focused runtime checks pass for the 500-case Integer matrix,
repeated artifact admission, precise collection, earlier arithmetic replacements,
default numbers and incoming ABI. A separate full-host probe confirms ordinary
`Module load:` admission from the saved default image and the transition from the
native seed to authored arithmetic. The subsequent cap-removal check qualifies
saving and mapping the tools snapshot with the Integer candidate enabled.
No exhaustive suite was run.

### Tools-image failure diagnosis

`birnam-tools.image` is a saved runtime snapshot containing the standard library,
console dependencies and all 23 authored tools. CLI tooling restores it to avoid
reloading and recompiling those modules. It is separate from the smaller
`birnam-bootstrap.image` used for normal source/console startup.

Replaying the original early Integer load order reproduces the failure: the
bootstrap image saves, all tools load, and the tools save answers nil with a
generic rejected report. Native Image tracing confirms that graph construction
and the first ImageFormat structural composition pass. Codegen then refuses the
native-emission batch before TCC compilation or file publication.

The exact refusal is `generated source exceeds bundle bound`.
`rust/birnam-codegen-runtime/src/backend.rs` previously limited the combined generated C
source in `backend::all` to 128 MiB. The failing tools snapshot has 5,636 emission
requests. `Plan::new` requests both resumable and synchronous implementations of
each portable program plus a named-method adapter for every binding. Adding the
authored Integer policy pushes this larger snapshot over that aggregate limit;
the bare and normal-startup snapshots remain below it and save successfully.
This limit concerns temporary generated C, not the final executable's size.

Error reporting obscures the cause: `image::operation` maps
`backend::Failure::Rejected(message)` to nil, discarding the message. Image then
cannot read a byte result and supplies its generic rejection. The installer only
observes that the expected file was never published. This is separate from the
arithmetic runtime slowdown, and does not demonstrate an invalid Integer policy
or broken Id dispatch.

The user chose to remove the arbitrary aggregate 128 MiB limit. `backend::all`
now emits the requests without that aggregate refusal. The same tools-only
replay saves a 235 MiB image successfully in about 147 seconds. Loading that
exact image with compiler routes replaced by rejecting callbacks succeeds in
about 7 seconds with zero compiler calls. The runtime arithmetic performance
gate remains separate; Integer is still an optional production artifact.

User-image load policy is explicit: use existing machine code unless rebuilding
was requested; compile an image without machine code; and propagate any mapping
failure without attempting a rebuild. The existing loader already implements
this policy. A new fresh-process regression check covers successful mapping,
failed mapping with no compiler fallback, code-free image compilation and an
explicit rebuild of a compiled image. All four cases pass. Workspace compilation
also passes. No streaming-emission change was needed for this fix. Other backend
rejections still lose their diagnostic message and remain a separate follow-up.

Evidence: `/tmp/birnam-integer-image-diagnosis/{bare.log,minimal.log,early.log,
instrumented.log,backend-refusal.log}`. The original-order replay failed in about
140 seconds; the tools-only replay failed in about 78 seconds with the precise
backend refusal captured. Temporary native tracing and bootstrap wiring have
been removed from the working tree. No full installer or exhaustive test suite
was rerun for this diagnosis.

Cap-removal evidence: `/tmp/birnam-integer-image-diagnosis/{cap-removed.log,
load-policy.log,tools-mapped-policy.log,cap-check.log}`. Qualification used a
temporary prefix; the user installation was not changed.
