# Native value foundation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

The agreed split extends Word, separates Bytes from String, and puts explicit
native operand adaptation in Execution. Id remains the dispatcher. There is no
new omnibus primitive service or duplicate byte-memory prototype.

| Owner | Responsibility | Current implementation |
| --- | --- | --- |
| Word | Owned unsigned 64-bit words, wrapping arithmetic, division/remainder, bit operations, logical shifts and unsigned comparison | Rust |
| Bytes | Owned buffers, bounded access and slicing, append, hashing, generic caller-schema framing and indexed views | Rust |
| Execution | Explicit Object/Word/Bytes carrier conversion and synchronous calls through current Id messages | Rust |
| native-values | Word range conversion and hexadecimal printing; existing Boolean/Integer/Bytes conversion policy | Birnam |
| Integer | Signed checked arithmetic and language numeric behavior | Birnam checked arithmetic by default; Rust cold-bootstrap seed retained |
| String/StringBuilder | Text behavior over Bytes references; neither owns byte vectors | Rust; full port remains |
| Boolean | Canonical Boolean policy | Birnam production policy; Rust cold-bootstrap seed retained |
| Vector, Map | Persistent collection algorithms | Rust; rewrites remain |

The current implementation includes the owner split, production Boolean policy
and default Birnam Integer arithmetic. Integer responsibilities beyond arithmetic,
String, Vector and Map still have production rewrites remaining.
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
signature and keep their existing calling convention. After an optional `!`
prefix, the first byte describes the native result; remaining bytes describe native
arguments. The prefix asks Execution to notify the receiver through `nativeFailed:detail:` on
incoming adaptation or invocation failure, retaining the original failure. Result
`m` executes an ordinary deferred native-message request with scoped cleanup; see
the [Trace execution boundary](../../guide/trace.md#native-execution-boundary). `o`, `w` and `b`
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

At the initial arithmetic checkpoint, default admission was blocked by performance qualification: in separate runs the
numbers fixture rose from roughly 1.2 seconds to 93 seconds with authored arithmetic enabled, and
a fresh installation failed to publish its tools image after a long preparation.
The image failure was the compiler's aggregate generated-source limit, now
removed and verified against the failing tools snapshot (see below).
That checkpoint published `integerValues` as an **optional** artifact, recorded
it in `optional_modules`, and excluded it from default bootstrap and tool images.
Explicit admission was `[Module load: #integerValues]`. The later source cleanup
and guarded Word pass described below supersede this optional configuration:
Integer arithmetic is now admitted by default. This remains an arithmetic slice,
not a completed rewrite of every Integer responsibility.

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
gate remained separate at that checkpoint; Integer was still optional then.

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

### Integer arithmetic benchmark, 2026-10-06

The [measurement artifact](../../measurements/integer-arithmetic-20261006.json)
records matched native/authored runs on Apple M3 with release native libraries.
The benchmark calls current Id selectors on fixed Integers 12345 and 37, checks
each result through the Integer owner, excludes bootstrap/module admission,
warms each operation, and reports medians of three 500-operation trials. It runs
the two variants sequentially in separate processes. It measures ordinary
dispatch, not compiled Birnam loop optimizations. The driver itself is a dev
build; these numbers describe this boundary and are not pure CPU instruction
throughput or a whole-startup benchmark.

| Operation | Native, GC off | Authored, GC off | Authored, normal GC |
| --- | ---: | ---: | ---: |
| `+` | 2.51 us | 1100 us | 1248 us |
| `-` | 2.44 us | 1216 us | 1463 us |
| `*` | 2.56 us | 1894 us | 2257 us |
| `/` | 2.35 us | 1867 us | 2250 us |
| `mod:` | 2.45 us | 1918 us | 2264 us |

There are two concrete sources of cost:

1. Generated C's selector cache keys its 512 entries by C-string pointer rather
   than spelling. TCC retains repeated literal strings at distinct addresses.
   A diagnostic records all 512 slots occupied with only 13 unique spellings.
   Misses then scan the entire table, including frequently used root operations.
   macOS sampling attributes approximately 47% of worker samples to PCs inside
   the emitted `_selector` function, identified through ELF symbols. Increasing
   only cache capacity to 8192 reduces addition to 429 us and the selector-like
   sampled leaf count from 1993/4227 to 17/3343. Keying the original 512-slot
   cache by spelling instead reduces addition to 465 us. These are temporary
   diagnostic variants, not a shipped cache fix.
2. Source control flow and generic ABI conversion produce large allocation and
   message workloads even after that cache cost disappears. One addition creates
   117 objects: one Integer result, 31 Words, six Vectors, 14 Blocks, 28 behavior
   objects, and approximately 37 other/capture-storage objects. GC charge is
   5944 bytes versus 16 bytes and one result object natively. Ordinary collection
   adds roughly 14% to addition here; GC is not the primary steady-state cause.
   With budget 1, addition takes approximately 2.42 seconds versus 9.08 ms
   natively, explaining why collection-stress fixtures amplify the regression.

The captured control-flow count follows the source: nested `ifTrue:ifFalse:`
expressions construct both branch Blocks before selection. Synchronous lowering
constructs reference-slot capture storage and invokes Block's constructor for
each literal; Block construction delegates a behavior and admits its CodeObject.
All these operations remain messages. Residual profiles are led by Id lookup,
dispatch, root bookkeeping, copying and allocation, rather than integer machine
arithmetic. This ordinary-send benchmark does not exercise the fused native
predicate path, so losing that optimization is not needed to explain this gap.

Next work is specific: fix the spelling cache, simplify repeated operation/sign
dispatch and branch nesting in Integer source, then profile again before adding
guarded compiler optimization of closure/Word temporaries. Preserve observable
method and allocation replacement behavior and deoptimize when the relevant
assumptions change; do not substitute native signed-Integer policy for the port.

Reproduce with `BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib`,
`CARGO_TARGET_DIR=.cargo-target/decomposition` and `BIRNAM_INTEGER_BENCH_MODE`
set to `native` or `authored`, then run
`mise exec -- cargo test -p birnam-siphon --test integer_performance --offline --locked -- --ignored --nocapture`.
Optional `BIRNAM_INTEGER_BENCH_OP`, `_GC`, `_COUNT` and `_TRIALS` variables narrow
the run. Prefixes for cache probes and raw logs/profiles are under
`/tmp/birnam-integer-benchmark/`. Production compiler/source changes were not
made; temporary tracing was removed from the working tree.

### Selector cache fix and Integer source cleanup

The spelling-keyed selector cache is now implemented in the compiler prelude.
The cache still roots its Symbols in their Unit and every actual send calls Id.
A generated-C regression exercises 700 distinct literal addresses with one
spelling, fresh dispatch results, and a genuinely full cache's intern fallback.
It failed on the old pointer-keyed cache and passes on the new implementation.

Integer now reuses persistent operation Blocks instead of building nested
operator-selection branches. Addition and subtraction use Word XOR/AND sign
tests for overflow. Magnitudes and sign restoration use unsigned Word masks;
Word only supplies generic unsigned operations. Error precedence, Float
dispatch, zero division, overflow and cold-bootstrap method replacement remain
covered by the focused tests. No foreign representation or method context is
read, and no primitive operation bypasses its current Id message.

| Operation | Original authored | Fresh optimized | Improvement |
| --- | ---: | ---: | ---: |
| `+` | 1100 us | 279 us | 3.94x |
| `-` | 1216 us | 279 us | 4.35x |
| `*` | 1894 us | 410 us | 4.62x |
| `/` | 1867 us | 362 us | 5.16x |
| `mod:` | 1918 us | 363 us | 5.28x |

These are the same fixed-input GC-disabled median measurements as above.
The preliminary hybrid prefix measured addition at 265 us and remainder at
356 us; the table reports the fresh release installation instead. Addition
now creates 72 objects versus 117: closures 14->8, behaviors 28->16 and Words
31->16. GC charge falls from 5944 to 4712 bytes. It remains much slower than
the native seed. This source-cleanup checkpoint retained opt-in admission and
did not yet implement guarded Word allocation elimination; the following section
records that subsequent work.

Qualification prefix: `/tmp/birnam-integer-optimized/final-prefix`. Fresh
startup/tool images, exact receipt, workspace compilation, backend tests,
Integer matrix/GC/type errors/all mixed Float operators, native replacement
preservation, Boolean and incoming ABI checks, compiler/deoptimization checks
and source-free mapped/rebuilt typed-image smoke all pass. No exhaustive suite
was run. The user installation is unchanged. Logs are under
`/tmp/birnam-integer-optimized/` and `/tmp/birnam-integer-benchmark/`.


### Guarded Word temporary elimination and default Integer

Integer arithmetic is now loaded from `integerValues` by default, after
`booleanValues`, in both artifact installation and bootstrap admission. The Rust
Integer representation and native arithmetic seed remain available for a cold
bootstrap. Source installation still preserves earlier method replacements.

The synchronous compiler recognizes trees of 4–32 known unsigned Word
operations whose inputs are lexical reads and whose intermediate results have
one use. It emits a bounded version-1 byte plan and sends
`tryPrimitiveExpression:` to the current Word owner through Id. This native ABI
message receives the plan as Bytes and inputs as Objects. Only Word reads its
own storage; signed overflow, Float dispatch and other Integer policy remain in
Birnam. No resumable Frame slots or branch Blocks are removed.

Word accepts a plan only with current default lookup and allocation permissions,
Object Model's allocation-omission permission, recognized Word inputs and the
owner's original primitive entries. Primitive meanings are cached by Id's
behavior epoch; a final epoch fence detects changes made during guard callbacks.
A refusal returns Unit and executes the original message tree once. An older
Word owner lacking the protocol also takes that fallback. Errors propagate
normally. The pass does not cache operation results or substitute native signed
Integer arithmetic.

The focused generated-method regression reduces four Word allocations to one
escaping result. Replacing `bitXor:` or the allocator forces the original tree;
collection during permission checks preserves inputs; changing a method during
a positive permission check triggers fallback with one replacement invocation.
The authored addition probe reduces Word allocations from 16 to 13, total
objects from 72 to 69 and GC charge from 4712 to 4664 bytes. Eight branch Blocks
remain: eliminating them would require preserving observable resumable Frames.
This is a bounded Word-temporary optimization, not general closure elimination.

The existing native Integer predicate optimizer correctly declines authored
arithmetic. Its regression now verifies that refusal and retained semantics,
while still requiring allocation savings when native arithmetic is selected.
The benchmark defaults to authored arithmetic; `BIRNAM_INTEGER_BENCH_MODE=native`
restores the retained five native seed entries after bootstrap for its control.

Qualification used `/tmp/birnam-integer-default/final-prefix`: a fresh release
installation published startup and tools images and passed its exact receipt.
Workspace compilation, four backend unit tests, selector-cache regression, Word
unit test, Integer boundary/type/Float/GC/reload checks, earlier replacement
preservation, compiler/deoptimization checks, guarded Word mutation tests and
source-free mapped/rebuilt typed-image smoke pass. FizzBuzz produces the expected
20 lines. Logs are under `/tmp/birnam-integer-default/`, notably `tests.log`,
`guard-mutation-final.log`, `unit-final.log`, `final-check.log` and `image.log`.
No exhaustive suite was run; the user installation remains unchanged.

New arithmetic timing runs were noisy and do not establish another CPU-speed
improvement beyond the earlier cache/source measurements. A matched console
startup pair measured old/new wall times of 35.39/34.64 seconds and user CPU of
7.92/12.69 seconds; this is not evidence of a startup speedup. Sampling the slow
fresh installer found temporary-file I/O dominating TCC work. Allocation savings,
correct fallback and default admission are the verified outcomes of this slice.


### ABI literal elimination, local fusion and authored helpers

The three follow-up optimizations are implemented, enabled in synchronous
emission, and exercised with freshly rebuilt release owners and freshly compiled
Integer artifacts. They keep cross-owner operations on current Id messages.

1. **ABI argument literals.** `abi_arguments.rs` recognizes nonescaping Vector
   literals of up to eight lexical values consumed by Execution's native adapter.
   Generated code sends `tryNativeArguments:` with a temporary operand array.
   Execution checks its selected ordinary adapter; Vector's
   `canOmitNativeArguments` checks its own factory, constructor/add/read methods,
   lookup and allocation permissions, including backing-node allocations. Both
   fence the behavior epoch. Signature inspection uses String/Bytes owner
   `tryPrimitiveBytes` queries: they prove the original accessors and return a
   borrow for immediate copying, subject to the caller epoch fence. Custom
   `bytes` accessors are never run speculatively. Refusal reconstructs the literal and performs the
   original call once. Target dispatch, Word/Bytes adaptation, borrowed-byte
   copying and result construction share the ordinary adapter's implementation.
2. **Word fusion across locals.** The producer follows single-use local bindings
   through a contiguous pure region. Calls, construction, unused expression
   statements and captured/escaping locals prevent admission. On refusal, the
   original producers and lexical aliases are evaluated in source order. A
   generated four-operation method using three locals allocates one Word with
   default permissions and four when the allocator is replaced.
3. **Authored helper candidates.** The compiler derives whole-body Word plans
   from actual synchronous PortablePrograms, with 4–32 primitive operations.
   Within that complete pure body, shared locals are allowed only when every use
   is inside the plan; each producer is evaluated once. Generated Methods attach
   the plan through `withWordExpression:bindings:parameters:`. Named synchronous
   adapters opt into selection through `withWordHelperAdapter`. Execution validates
   the current Block invocation and owner capture accessor, then sends the plan
   and expected epoch to Word's `tryPrimitiveExpressionAtEpoch:`. Word fences
   that earlier epoch as well as its own primitive/allocation assumptions. No
   Integer helper names or signed policy are recognized in Rust. Resumable and
   typed carrier adapters keep their ordinary boundaries; one-operation helpers
   are excluded because their guard overhead was not worthwhile.

Execution keeps the entered synchronous invocation active while checking a
helper candidate, so guard callbacks cannot report the runtime idle or drain
entered code. Candidate metadata contains only owned plan bytes and indices;
method capture retains candidate metadata and the original implementation/owner.
Replacements of Block invocation, capture access, Word primitives, Vector
construction/read methods or allocation hooks restore ordinary execution.
A regression replaces a signature byte accessor that changes Vector construction;
it runs once, after the original literal construction. Another regression changes
Block invocation after a positive allocation grant and
collects during that callback: the new invocation runs exactly once.

The following are sequential fixed-input, GC-disabled medians of three trials
of 500 sends per operation. They exclude bootstrap, artifact loading and the
allocation census. The timings below are diagnostic: repeated baseline/final runs varied
substantially, including large baseline spikes. These runs do not establish a
reliable throughput improvement for the three passes; allocation savings are
the repeatable outcome. Guard overhead remains a performance target.

| Operation | Before these three | ABI literals | Across locals | Helper candidates |
| --- | ---: | ---: | ---: | ---: |
| `+` | 778.97 us | 277.93 us | 282.98 us | 287.14 us |
| `-` | 906.06 us | 277.98 us | 284.96 us | 285.52 us |
| `*` | 1089.99 us | 421.88 us | 420.10 us | 415.97 us |
| `/` | 488.30 us | 360.11 us | 368.55 us | 366.99 us |
| `mod:` | 372.79 us | 357.33 us | 366.19 us | 363.78 us |

The allocation savings are deterministic in this fixture. ABI literal elimination
removes all six temporary Vectors and their nine backing allocations per
operation. Addition falls from 69 to 54 objects and 4664 to 2000 charged bytes.
The helper pass additionally removes nine Words from each multiply/divide/
remainder operation (33->24, 30->21 and 29->20 respectively). Final GC charges
are 2960, 2528 and 2496 bytes for those operations. Branch Blocks still exist;
this work does not implement arbitrary call-site inlining, resumable helper
inlining, branch elimination or authored Integer predicate fusion.

Validation passes: 27 owner/backend unit tests; seven focused runtime tests
covering ABI literals, local fusion, authored helpers, Integer's 500-case matrix,
incoming carriers, collecting bootstrap replacements and compiler/deoptimization
semantics; and source-free mapped/rebuilt image checks with helper source and
compiled helper artifact deleted. The latter is reproducible with
`scripts/word-optimization-image-smoke.sh PREFIX`. Workspace compilation also
passes. The final example check runs FizzBuzz through an explicitly requested
source bootstrap and compares all 20 output lines.

Diagnostic prefixes are `/tmp/birnam-integer-three/{abi,locals,helpers}-prefix`;
these copy the previous qualified installation and replace changed release owners
and Integer artifacts. They are not fresh full installations and their old install
receipts do not qualify the modified contents. Raw timings, test logs and image
checks are under `/tmp/birnam-integer-three/`; the measurement JSON includes all
60 per-step trials plus 60 baseline/final repeats with GC disabled and normal GC. No exhaustive suite or new startup timing was run. The user
installation remains unchanged, and the changes are uncommitted.


### Bounded paired microbenchmark tooling

The next performance work uses [short paired microbenchmarks](microbenchmarks.md)
through `scripts/microbench.py`. The runner controls program and typed-body
factory emission explicitly, alternates baseline/candidate batches in one
process, records thread CPU and elapsed time, and asserts allocation differences
before timing. Six fixtures cover nested/local Word fusion, ABI literals,
authored helpers and isolated Integer add/multiply policy. Measurement is capped
at ten seconds; a busy machine yields uncertainty rather than an idle-host wait.
A separate two-to-five-second macOS sampling mode runs one selected variant.
The first full acceptance run needed 1.26 seconds of setup and 6.01 seconds of
measurement. Detailed message/lookup/guard counters remain a later addition;
physical allocation counts already work without changing runtime dispatch hooks.


### First microbenchmark-guided optimization slice

Word-plan and small ABI marshalling scratch buffers now avoid unnecessary native
heap allocations. Vector caches unchanged native method meanings by Id epoch,
while checking live allocator/GC permissions and fencing callbacks each time.
The new public Integer fixture uses real source policy and public operator sends,
with source methods switched through Id between untimed warmups.

The runner now fixes its batch size after one calibration: the previous adaptive
count oscillated across GC thresholds and created an order-dependent ABI result.
Corrected ABI measurements verify the earlier allocation-elimination benefit;
they do not establish an additional CPU gain from these small changes. Public
Integer arithmetic still gives an inconclusive roughly 3% difference.
See [microbenchmark details](microbenchmarks.md) for limits and measurements.


### Public Integer path diagnosis

The [complete Integer investigation](integer-performance-investigation.md) finds
roughly 4232 Id messages per optimized addition. Eight branch Blocks still need
capture storage, behavior delegation, CodeObject shape inspection and entry
installation. Root/name traffic dominates message counts; Block construction
accounts for about 27–30% of sampled path time, and normal GC also matters.
The next targets are admitted CodeObject shape reuse in Block construction and
root/name work, ahead of further Word scratch-buffer tuning. No arithmetic
implementation or Id semantics were changed by this investigation.


### Guarded resumable call preparation (2026-10-10)

The native backend now emits an optional call-preparation attempt before ordinary
and tail calls with up to 16 arguments. Execution copies the receiver/arguments
into the existing descriptor call window and publishes the same checked action,
omitting the transient Integer objects otherwise used to index those operations.
Source values, frame layouts, continuation edges and budget stops remain intact.
No language control or graph policy moves into Rust.

Admission requires the current Machine read/write/call implementations, canonical
Namespace lookup and matching Integer factories, and the existing Integer
allocation-omission permission. Final epoch, factory and activation checks follow
callbacks. A failed guard returns to the original sequence before any frame write;
already completed source effects are not replayed. Once committed, normal driver
dispatch still chooses the target through Id. No permission survives into the
next call. This form needs no virtual-frame reconstruction because no logical
frame or inspectable source value was removed.

The [integration fixture](../../../rust/birnam-siphon/tests/compiler/frame_call.rs)
checks constructor/getter, frame-access, call/tail-call, namespace and GC-admission
replacement effects; collection during guards; constructor mutation after a
positive grant; namespace rebinding without an epoch change; optional-protocol
removal; propagation of an entered UNBOUND failure; exact input identity; and
matching one-step frame snapshots with collection between stops (normalizing
only the separately created Machine identities).

The [paired measurements](../../measurements/resumable-call-preparation-20261010.json)
use the same source compiled with masks 0 and 32, a fixed 16 MiB budget and
release owners with uninstrumented Id. Compilation and allocation census are outside
timed batches. Three-second runs produced:

| Fixture | Candidate/baseline CPU | Paired samples | Objects per invocation |
| --- | ---: | ---: | ---: |
| Three two-argument calls | 0.629 (95% interval 0.616–0.642) | 22 | 52 → 28 |
| Three unary calls, captured result | 0.790 (0.785–0.793) | 30 | 36 → 24 |

An independent untimed Id census for the two-argument fixture counted 1,155 → 663
messages and 41 → 17 Integer constructions per invocation. No dispatch methods
were replaced to obtain those counts. These are local fixture results, not a
whole-language speedup claim. Earlier exploratory timings are superseded by the
paired records. Reproduce with `scripts/microbench.py --prefix PREFIX --case
resumable-calls --seconds 3` or `--case resumable-unary`; add `--counts-only` for
the separate census. The new default compiler mask is 61; mask 29 disables only
this new pass while preserving the earlier synchronous passes.

Two other targets were investigated. Reusing the existing Id matcher to batch
inline-control guards showed no measurable CPU benefit (7.65 versus 7.67 ms in
the small 180-invocation probe), so that experiment was removed. General branch-
Block elimination still requires a separate escape/recovery proof and potential
materialization of inspectable blocks; it is not silently enabled by this change.
Adaptive GC scaling remains absent.


Validation passed: 105 ABI/Codegen/Execution/backend tests, four benchmark-reporter
tests, and the production compiler, Machine/control, Method lifetime, BlockLoop,
generation projection/ledger/transaction integration binaries. The fresh offline
cold prefix `/tmp/birnam-frame-opt/accepted-prefix` passes exact receipt and
588-file Rust/Birnam source comparison, seed exclusion and parser/compiler-disabled
startup. Its fixed GC boundary, authored graph oracle and freshly compiled
bootstrap-loop tests pass. Source-free mapped and rebuilt recovery passes for
Trace, Word helpers/fixed ABI conversion, and the new
[`frame-call-image-smoke.py`](../../../scripts/frame-call-image-smoke.py), which
withdraws the optimization protocol, collects, and verifies ordinary fallback
plus a subsequent low-level target binding change. The new image test is wired
into the host image gate.

The existing Word image smoke initially failed identically on the previous and
new prefixes: it called the retired general outbound ABI. The fixture now uses
the current fixed Word conversion and passes. No runtime compatibility shim or
relaxation of generation quiescence was added. Logs and rejected experiments are
under `/tmp/birnam-frame-opt/`. Validation is focused, not full release qualification;
the normal user installation is unchanged.


### Trailing-copy fusion and full control measurements (2026-10-10)

The preceding call-preparation pass was measured in the full authored control
unit, not just bootstrap loops: local paired loop/fold fixtures used roughly
40%/35% less CPU with masks 29/61. The remaining eight-iteration loop had 113
ordinary frame reads but only one `returnAt:` message; the fold had 53 reads and
one return. That evidence favored frame access over a separate return guard.

An experimental guarded closure-capture batch reduced allocation but did not
establish a meaningful CPU gain in longer samples. It was removed. Blocks are
still constructed normally; no general closure-elimination or virtual-value
recovery mechanism was introduced.

The retained pass folds a contiguous suffix of local binding copies into the
already guarded call window. It shares the existing guard sequence, validates
all sources first, simulates sequential aliases/overwrites, then writes every
original frame slot and publishes the normal call or tail-call action. It cannot
cross source operations or persistent global publication. Guard refusal resumes
the original sequence before any frame write, without replaying earlier effects.
Default mask is now 125; mask 61 provides the previous implementation.

[Paired local evidence](../../measurements/frame-copy-fusion-20261010.json), at the
fixed 16 MiB GC budget, shows:

| Fixture, masks 61/125 | Candidate/baseline CPU | Allocation evidence |
| --- | ---: | --- |
| Alias chain and call | 0.690 (95% interval 0.686–0.694), 42 pairs | 22 → 16 objects/invocation |
| Full authored loop | 0.972 (0.968–0.980), 26 pairs; inconclusive at the 3% margin | 441 → 407 Integer allocations/invocation |
| Full authored Vector fold | 1.002 (0.997–1.017), 19 pairs; no meaningful difference | Identical counts; no eligible trailing copies |

The loop's untimed Id census falls from 18,417 to 17,703 messages; the fold remains
at 11,832 with zero copy-batch invocations. These are fixture results, not a
whole-language speedup claim. Reproduce through `scripts/microbench.py` with
`--case resumable-bindings`, `stackless-loop` or `stackless-fold`.


Validation for this follow-up passed 107 ABI/Codegen/Execution/backend tests,
four benchmark-reporter tests, and the production compiler, Machine/control,
Method lifetime, bootstrap-loop, generation and generation-transaction binaries.
The copy oracle checks all 729 three-copy plans over three slots against ordinary
sequential storage. Compiler tests reject fusion across literal work, persistent
publication and over-limit copy lists. Both optional protocols have matched
replacement, collecting-guard, post-grant mutation, missing-protocol, exact failure
and paused-frame regression coverage.

A fresh offline cold build at `/tmp/birnam-frame-followup/accepted-prefix` passed
its exact receipt, 589-file source comparison, and parser/compiler-disabled
startup. Source-free mapped/rebuilt images passed withdrawal of both frame
protocols and later target replacement, plus the existing Trace recovery check.
The installed allocation-accounting and bootstrap-loop tests passed. Authored task
scheduling passed all 171 checks, including quanta 128 and 1 and native boundary
checks. Logs are under `/tmp/birnam-frame-followup/`. This is focused acceptance,
not release qualification; the normal user installation is unchanged.
The 16 MiB default remains fixed, without scaling.


### Trace and control candidate investigation (2026-10-11)

No additional production runtime optimization was retained from this round.
The [evidence record](../../measurements/trace-candidates-20261011.json) preserves
measurements, rejected prototype sources, replacement results and formal checks.
The fixed 16 MiB budget, installed runtime and current source policies are unchanged.

**Frame access.** The current full-heap Trace probe against the preceding fresh
cold prefix took 11.24 seconds, charged 118,501,768 bytes and collected eight
times. The subsequent three baseline repetitions had a 10.659-second median.
These are diagnostic observations, not a change from the previous release or a
paired language-wide speedup. `birnam-build` compiles Trace with synchronous
bodies; its bootstrap traversal uses the native Vector fold. The resumable-frame
passes therefore do not directly address this measured path. No full-stdlib
Trace timing is claimed.

For general resumable code, call arguments and trailing copies already share an
admitted owner batch. Remaining literal writes and capture construction cross
potential callbacks. Moving their guard across those callbacks requires renewed
binding/permission checks and preservation of the exact already-written frame;
retrying a constructor on refusal would replay effects. The earlier capture-read
batch failed to establish a CPU gain, so it was not reinstated. This round's
frame-access investigation was source/contract analysis, not a new frame-access
speedup experiment. The production compiler regression, including both guarded
call protocols, replacement, collection, refusal and paused frames, passed again.

**Indexed transport.** Two temporary versions kept scalar allocation delivery
and the ordinary current `Storage observe:` dispatch. An optional visitor method
negotiated before facts; roots/edges used positions in the sorted snapshot, never
allocation identities. Trace still performed duplicate-identity/object checks,
endpoint validation and graph/report policy. No graph logic moved into Storage.

* Scalar indexed endpoints: three alternating-order process pairs had median
  report time 10.659 -> 10.342 seconds (about 3%); median process CPU was
  10.89 -> 10.58 seconds. Allocation rose 118,501,768 -> 119,061,920 bytes.
  The initial apparent 10% gain did not survive repetition. This is insufficient
  evidence to accept the additional interface and protocol complexity.
* Bounded byte batches: up to 256 root indexes or 128 edge pairs per callback,
  encoded as little-endian u32. The first exploratory report took 14.63 seconds,
  despite reducing allocation to 113,364,528 bytes and collections to seven.
  Authored byte-offset arithmetic and iteration were more expensive than the
  removed callbacks. A separate repeat confirmed 14.62 seconds versus a fresh
  10.59-second baseline; process CPU was 14.86 versus 10.82 seconds.
  Existing tagged-record decoders do not directly provide
  the fixed-width index stream needed here. No new native decoder was added
  merely to salvage this candidate.

Both transport prototypes were removed. Their normal full-heap probes completed
with ordered reports, but they were rejected before complete adversarial graph,
image or cold-install qualification. The transport proofs described below are
abstract reusable work, not proof that either rejected implementation is correct.

**Collection iteration.** A temporary authored Vector fold reused two handlers
instead of constructing branch captures on every iteration. At compiler masks
125/125, 30 paired batches gave CPU ratio 0.858 (95% within-run interval
0.853–0.872). Across eight invocations, Block allocations fell 152 -> 8 and
allocation charge 80,640 -> 59,328 bytes.

That apparent win is **not semantics-preserving**. Replacing the current Boolean
`ifTrue:ifFalse:` with an answer of 777 made the original empty fold return 777;
the candidate returned 0. It switched selectors and bypassed a current control
replacement. It also changes fresh Block identities. The candidate was removed.
A future compiler specialization needs current control/constructor permissions,
real recovery of observable values and continuation state, and revalidation
across body callbacks; normal-result agreement is inadequate.

**Retained verification support.** The benchmark runner now supports independent
control-policy sources at identical compiler masks, records both policies, and
provides an untimed collecting Boolean replacement check. The unchanged policy
passes (777/777), while the faster invalid prototype fails (777/0). An identical
policy/mask timing control reports no meaningful difference.

`IndexedSnapshot.lean` proves root/edge sequence round trips under an explicit
left-inverse index mapping, injectivity, preservation of any graph policy on the
decoded inputs, and preservation of committed prefixes. `IndexedSnapshot.tla`
checks a bounded transport lifecycle with current callbacks, validation before
reporting, inventory protection and cleanup; intentionally broken bounds,
stale-dispatch, replay and early-release variants must produce counterexamples.
`SemanticsExamples.lean` adds a non-refinement witness for the unguarded fold.
These contracts do not verify byte encoding, Rust/C translation or arbitrary
owner declarations. No compiler optimization is enabled by them.

Final verification passed: 152 Lean theorems with standard-kernel axiom audit,
27 required rejected Lean mutations, and 99 TLC cases (24 passing models and
75 required counterexamples). The production compiler integration binary passed,
as did all four benchmark-reporter tests. The matching-policy collecting
replacement control passed and the rejected fold failed that same gate as
required. Runtime sources match the preceding commit; no cold installation or
release qualification was repeated for these tooling, proof and evidence changes.

### Validated dictionary positions (2026-10-11)

The [application workload](../../design/application-performance.md) sampled Id dictionary
lookup as its largest individual CPU leaf (345 of 2,532 samples). A bounded
256-entry hint table now accelerates dictionaries using the derived key index;
small dictionaries retain their cheaper linear scan. Each hint contains only a
slot number. Lookup still verifies live dictionary membership, checks the current
selector at that slot, and reads the current Operand/NativeEntry. A stale,
colliding or out-of-range hint falls back to the original lookup. Parent links,
absence and executable method/context results are never cached. The table adds
4 KiB per Id instance on the supported 64-bit layout and holds no Object roots.

Method replacement at an unchanged slot takes effect immediately. Slot shifts,
weak-key removal, parent changes and retired/reused dictionary records remain
ordinary live lookups. Custom lookup, receiver reassociation, DNU and generated
optimization guards are unchanged. No source values/frames/allocations disappear,
so fallback needs no materialization or effect replay. Existing cold boot and
image restoration create empty hints; no durable format changed.

`SlotHints.lean` proves that a validated position in the current unique-key
dictionary agrees with ordinary lookup. `SlotHints.tla` requires counterexamples
for missing key validation, cached old values and cached parent results. Rust
checks compare with an independent linear scan across mutation, index shifts,
opaque shadows, retirement/re-registration and deliberately poisoned hints.
These proofs model the owner-local lookup rule, not the complete Rust compiler.

A first hint implementation applied to every dictionary did not establish a useful
gain and was replaced by the larger-dictionary-only version. The retained version
measured approximately 4.8% less warm CPU on the 50-record application, using the
same published benchmark artifacts in baseline/candidate/candidate/baseline order.
This is a local separate-process comparison, not a language-wide speedup claim.
See [raw evidence](../../measurements/log-summary-20261011.json) for validation and
source/library hashes. The normal installation is unchanged.
