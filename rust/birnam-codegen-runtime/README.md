# Source compiler over Id

Codegen consumes Lang CST messages and produces canonical portable programs and
compiler metadata. [Generation](../birnam-generation/README.md) owns publication,
rollback, source registration and image recovery. Codegen calls the separately loaded [NativeBackend](../birnam-native-backend/README.md)
through Id for native emission, slot coloring and descriptors. `compile:` returns
an owned resumable Method, `eval:` runs it on a real Machine, and `emit:` returns
the generated C String. TCC compiles and owns each Unit; its initializer consumes
Namespace and the Unit owner through the existing native module ABI.

Reflect `defineMethod:on:as:` compiles the retained named-adapter algorithm. Its
Method keeps both the code Unit and the implementation Block. Calls perform fresh
`value:` lookup on that implementation; replacement does not rewrite a captured
Method or a suspended frame. No generated Runtime service catalog is imported.

Installation sends `Store addMethod:on:with:` with the selector, target behavior
and owned Method. Store runs its existing authorization, mirror and notification
protocol. Reflect still returns the target on success; refusals and native failures
propagate unchanged. An observer failure occurs after installation and does not
undo it. Temporary compilation roots survive collecting hooks.

The uncompiled legacy compiler orchestration and its orphaned helpers/tests have
been deleted. Git retains the historical implementation; it is not current
validation or a compatibility layer. Remaining behavior is inventoried
[below](#remaining-migration-reference). The obsolete Runtime optimizer integration
has also been removed; reusable analyses remain in the pure compiler planners.

## Source and generated-code contract

`compile:`, `eval:` and `emit:` take source String Objects. Parsing uses current
Lang `parse:`; CST projection uses `ruleName`, `start`, `end`, `childCount` and
`childAt:`. The pure compiler retains its portable encoding, logical lowering,
slot coloring and canonical descriptor construction. The production native backend
emits the Id calling convention directly; it does not wrap exported legacy
all-Oop runtime functions. Baseline call sites publish the established Machine action and perform current
lookup when driven. The narrow discarded-clone consumer below can omit an admitted
callee body through the existing inline-call path.

Generated module initialization constructs one registered Method per implementation
using canonical descriptor and selection Bytes. Blocks copy captures through
`Block fromMethod:captures:` and Vector messages. Named adapters additionally use
`Method fromStep:descriptor:selection:owner:implementation:` so their implementation
Block and code Unit have distinct, explicit ownership. Top-level bindings publish
through Store-backed `Globals at:put:`; nested bindings remain lexical. Generated
source reads send `Globals at:` first, then use Namespace only when the answer is
nil, preserving the prior source lookup rule (including a bound nil). Missing
names answer nil. Compiler machinery still resolves its dependency objects through
Namespace. A rejected global write returns the setter's exact result and stops
that evaluation; prior successful ordinary writes remain published. Native send
failures propagate unchanged. Values and errors use their current owners, with
temporary generated values retained across callbacks.

## Source Loader protocol

`moduleMetadataOf:` now returns the existing five-element Vector: tag, module
Symbol, required Symbols, original-source SHA-256 and revision digest. It uses
the same pure recognition as `sourceRevisionOf:` and returns nil for unindexable
source. `embeddedFilesOf:` returns unique literal `System embedFile:` paths in
first-occurrence order, including nested Blocks. This preserves the syntactic
inventory: it does not resolve shadowing, open assets, evaluate source, or
register modules. Malformed/non-String input produces an empty asset Vector.
All parsing and value construction use current owner messages and scoped roots.
Host's project-source observations consume these selectors. Installed project
checks cover resolution and the check/build/run commands.

The existing `sourceRevisionOf:`, `recordModuleSource:named:`, `moduleSourceFor:`,
`sealInitialGeneration`, `lastError`, `evalPersisting:` and `evalPersisting:from:`
selectors now use the same current owners. Revision analysis remains private pure
compiler code; its seven-element Vector retains the tag, name, requirements,
original source and three historical digests and admission rules.

Codegen owns an append-only source/name registry and the existing closed-registration
flag. Sources remain original Objects and are visited through `rootsDo:` / `visit:`.
Compilation attaches a `#compiledSourceMetadata` fact to each source Object in
Store. Its ordinary Vector contains the version tag, source digest, module name,
requirements, revision inventory, documentation source and embedded-file paths.
The revision inventory contains method operations and their canonical programs,
projected Globals, implementation keys and the existing revision digests.
These are imageable relational values; there is no separate native metadata cache.
Generation projection/adoption and documentation inventory read these facts and
check source digests and live bindings without parsing. Missing or malformed
metadata fails admission; rebuild the compiled artifact or image. Explicit source
compilation and source-inspection operations may analyze new source.
Registry comparisons use current Symbol/String messages, without a state borrow
across sends; reentrant additions are observed before publication. Identical source
bytes return the incoming source while retaining the first registered Object.
Different bytes, mismatched names, malformed revisions and closed registration
answer nil with the existing diagnostic. `moduleSourceFor:` asks Generation for this registry.
Image now restores it and its closed-registration flag through a staged owner
exchange; candidate source-generation replacement uses the lifecycle below.

Loader-facing evaluation clears `lastError`, preserves nil/diagnostic versus returned
Error behavior, and uses owned Method/Block/Machine sends. It never calls a generated
entry address. Top-level Globals writes and Store MOP publication retain their
existing behavior. The supplied filename reaches generated descriptors, including
nested Blocks. No GC policy is changed; temporary values remain explicitly rooted.

[Source Loader](../birnam-source-loader/README.md) is wired after Codegen and tested
with the complete authored Module manager. Compile-time embedding now uses the
message path [below](#embedding). [Image activation and compensation](../birnam-image/README.md)
use the same source objects and current Store/Execution owners.

## Generated execution

Each implementation has one ordinary resumable entry with a PC switch. Baseline
owner operations remain live Id sends. Automatic per-PC candidates, their Method/Vector
construction and their size-limit policy have been removed: they retained the same
work and added construction and guard costs without an established benefit.
There is no replacement background compiler or candidate service.

Explicit candidates and guarded recovery remain available in Execution for actual
optimizations. Broader region fusion, numeric reuse and allocation-elision need
concrete producers and evidence under [Design 90's reassessment](../../docs/archive/design/90-optimizer-reassessment.md).

The production tests cover literals, sends, threaded messages, conditional and
nested capturing Blocks, named installation/replacement, failed compilation and
initialization, collecting factories, captured Methods and suspended frames.
Compiled protected-boundary exits deliver once into their callers; abandoned
waiting frames release their mappings. A separately authored C candidate
compiled by TCC recovers into a Codegen baseline, then removes its own binding and
collects before returning. This proves the compiled recovery boundary and mapping
lifetime; generated allocation-elision/deoptimization emitters remain separate.
Repeated compiled execution preserves effects of replaced Integer factories,
current numeric lookup and collecting Integer/Machine operations. Compilation,
larger functions and captured Blocks work even when speculative native Method
construction is rejected. Actual Unit counts and relocated bytes return to zero
after the final reference and checkpoint. Explicit candidate withdrawal/lifetime
and compiled recovery remain covered by their separate production fixtures.

Separate Artifact, PortableCode, embedding, Image and Host tests cover their
current message paths; compiler tests alone do not establish those workflows.
Current commands and limits are in [validation](../../docs/validation.md).

Store/db and Cells now load through Bootstrap before the compiler. Their separate
[production suite](../birnam-store/README.md#validation) also exercises compiled
source consumers and Store-backed Globals. Generated source reads and writes now
use that same Globals protocol, including Store history and query visibility.
Reflect now publishes through Store's receiver-based MOP hooks and owned Execution Methods. Staged
compiler publication and compensation use the candidate/ledger messages described
in [Generation](../birnam-generation/README.md); the old Store native tables are gone.


## First allocation-elision producer

The native backend recognizes exactly two PCs: entry receiver/argument operations,
a zero-argument send to an entry argument, then an existing-argument return. No
captures, lexical bindings, extra operations or named adapter are admitted. It
adds `Method withDiscardedClone:answer:selector:return:in:` metadata to the ordinary
baseline, retaining the result in the same TCC Unit. No extra native entry or
per-PC candidate is emitted. The runtime checks the actual selected clone and
allocator owners; selector spelling is not a proof. An unrecognized or changed
operation takes the baseline, and bounded execution always does so.

[Production evidence](../../docs/design/90-discarded-clone.md) covers actual heap
inventory, replacement effects, alias recognition, escaping/bound values, paused
Frames, collection during guards and release of every compiled Unit. This is a
single narrow consumer, not restoration of the retained legacy elision planners.


## Combined arithmetic predicate and unboxed intermediate

The native backend recognizes a three-PC uncaptured callee: one binary send using
entry arguments, another binary send consuming that result and an entry argument,
then an explicit return (optionally through its final binding). It attaches
`Method withNumericPredicate:then:left:right:limit:return:in:` metadata, retaining
the ordinary baseline and same TCC Unit. Runtime Integer ownership determines
whether the sends are arithmetic and comparison; aliases are eligible. Captures,
extra operations, intermediate bindings and bare tail-call forms are not emitted
as candidates. Bounded calls and unsupported operations remain baseline execution.

The admitted predicate returns an existing Boolean while avoiding its intermediate
Integer and callee index boxes. No new C kernel, deferred identities or loop cache
is emitted. [The production receipt](../../docs/design/90-numeric-predicate.md)
records current semantics, allocation savings and the local debug comparison.

## Remaining migration reference

Historical source is pinned at commit `3c4765089a016a01245e259e246b90472e76eaa5`.
These references preserve outstanding behavior, not obsolete native APIs or a
requirement to restore the old architecture:

- **Source Loader (migrated):** [revision metadata and source registration](https://github.com/chazu/birnam/blob/3c4765089a016a01245e259e246b90472e76eaa5/rust/birnam-codegen-runtime/src/deferred.rs#L2236-L2631),
  [revision values](https://github.com/chazu/birnam/blob/3c4765089a016a01245e259e246b90472e76eaa5/rust/birnam-codegen-runtime/src/deferred.rs#L4865-L4935),
  [persisting evaluation](https://github.com/chazu/birnam/blob/3c4765089a016a01245e259e246b90472e76eaa5/rust/birnam-codegen-runtime/src/deferred.rs#L6658-L6742)
  and `lastError` (line 10662). Preserve the loader's registration-before-evaluation,
  exact Error/nil handling and optional Namespace Module notification described in
  the [historical loader contract](https://github.com/chazu/birnam/blob/cffba722/abi/source-loader-v2.md). Current generated
  evaluation already publishes top-level bindings through Globals; nested bindings
  remain lexical. Filename-aware descriptors and loader-facing errors now use
  the current compiler, including the embedding path below.
- **Publication and Image:** the historical compiler
  contains revision/generation projections, staged activation/rollback,
  compensation and materialization algorithms. Portable materialization is now
  reconnected below, and Image now stages owned Methods and source registration
  with Store-state compensation. Candidate activation, rollback, drift detection
  and retirement now run through the message-based lifecycle in [Generation](../birnam-generation/README.md).
  Its [acceptance cases](https://github.com/chazu/birnam/blob/3c4765089a016a01245e259e246b90472e76eaa5/rust/birnam-codegen-runtime/src/deferred.rs#L11675-L12555)
  and sibling `code_bindings.rs`, `materialization_cache.rs`, `state_contract.rs`
  and `finalizer_tests.rs` remain in the same Git tree. Reconnect only the behavior
  required by actual consumers and verify it with current owner messages.

Do not restore direct generated-entry calls, service tables, native mark callbacks
or GC suppression as substitutes for roots. No lock or borrowed foreign bytes may
span an outgoing send. Publication preserves Store's MOP; executable references
retain their Method/Unit owners. The deleted synchronous-evaluator and compiler
state runners were not executing these tests in the current build. Production
compiler/lifetime coverage remains in `birnam-siphon/tests/compiler.rs`; Image
acceptance is in `birnam-siphon/tests/image.rs` and its linked contract above.

## Embedding

The literal `[System embedFile: "relative/path"]` form now sends to current
System at compile time, then snapshots its Bytes result through `bytes`. Generated
code stores the payload using the existing binary-safe literal codec and sends
`asBytes` to construct a Bytes object at execution. Source-relative and cwd-relative
paths, current method/global lookup, failures and remaining Image/project work are
specified in [Design 56](../../docs/design/56-embedded-files.md). Filesystem access
stays with File IO; all owner operations are Id sends. The `embedding` production
test includes an actual system-compiled dylib loaded after its asset is deleted.

## Portable materialization

`materializeSynchronous:captures:` and `materializeResumable:captures:` accept an
admitted CodeObject and an exact Vector with the required capture count. Canonical
Bytes come from PortableCode through a send; construction uses Execution's
`Block fromMethod:codeObject:captures:`. Source-generated Blocks also carry one
CodeObject per body, shared across instances; captures remain per Block.

Synchronous emission uses the current native Method ABI and preserves the native
control boundary. Resumable emission uses the existing PC/Machine path and can
suspend. Both emit owner operations as Id sends, including synchronous capture
reads. The historical capture-read order is preserved: before compilation for
resumable Blocks, after compilation for synchronous Blocks.

The cache compares complete canonical bytes and disposition, retaining at most
64 Methods, 64 MiB of programs and 128 MiB of generated C, with least-recently-used
eviction. `materializationCount`, `evictMaterializations` and nil/`lastError`
validation failures retain their existing contracts. Each returned Block retains
its supplied CodeObject even when its Method is shared. Cache roots use visitor
sends; eviction leaves live Blocks and suspended frames holding their code owners.
Failed construction publishes no cache entry. Production `materialization` tests
cover these behaviors, collecting replacements and final Unit reclamation. Image
uses the same canonical programs and code owners; optimizer work remains separate.

## Durable module artifacts

`compile:output:`, `compile:output:from:` and `compile:output:root:` write a
TCC relocatable native object through System File IO without evaluating source.
The filename/root variants preserve source-relative embedding. The object stores
the existing inert Artifact descriptor, original source, embedded payloads and
current generated Methods, plus compiler-produced source metadata. It advertises
`id-object-v2`. The historical `.dylib`
suffix remains for Module resolution; this payload is an ELF object for TCC,
not an OS-loadable Mach-O library. Native kernel dylibs keep their separate ABI.

`load:` reads one snapshot and sends `Artifact inspectBytes:from:` before passing
those same bytes to `TCC loadObject:`. It checks kind, capability and already-loaded
requirements, initializes an owned Method/source/metadata/adapter record, validates source identity
and digests, and runs the Method on a Machine. Successfully activated, revision-indexable modules enter
Generation's source registry. Artifact dependency discovery preserves the broader
historical syntactic declaration scan, independently of revision admission. Conflicting sources and sealed registration fail before
user initialization. Errors and native failures propagate; nil means unsuccessful
activation. Ordinary user initialization is not a rollback transaction.

Activation decodes the compiler-produced metadata, checks its source digest,
identity and requirements against the descriptor, and persists ordinary metadata
facts through Store. It neither parses source nor compiles adapters.
Methods installed during activation use the compiler's bundled adapter objects;
explicit source compilation retains the source adapter path. Blocks and
Methods retain the TCC Unit across collection; the mapping
is reclaimed after the final executable reference and an idle checkpoint.
The `artifact_activation` production test disables both TCC compilation methods,
removes source/assets, forces collecting inspection/relocation, preserves typed
failures, and checks actual Unit reclamation. Installed artifact tests exercise
Module's complete dependency preflight, registry rules and CLI diagnostics.
Project publication policy is exercised by installed Host checks; candidate
generation replacement and its independent tests are described below.

Module filename construction uses restored `Reflect nameOf:`: Symbols return
their Id-owned name bytes as a String; non-Symbols answer `notASymbol`.
`allMethodsOf:` resolves descriptor targets through native Namespace exports,
then Store-backed source Globals, using live owner messages. Stdlib composes
`factsOf:` from Store's rooted `liveTriplesDo:` snapshot and ordinary Blocks;
rows retain entity, attribute and value without changing collection settings.

## Initial generation projection

Ordinary `--compile-module SOURCE OUTPUT` artifacts already use resumable entries.
The former `--initial-generation-resumable` bootstrap switch is retired: source
registration comes from ordinary artifact admission, and Host seals the complete
initial projection before running. Candidate-specific selections remain explicit.

`initialGenerationProjection` constructs the retained four-element projection:
`#[#generationProjection sourceMap baseDigest generationSelection]`. Source-map
rows are sorted by module name and contain the source revision and nil component.
The complete implementation inventory records the current source backend's
resumable disposition; named-adapter descriptor hashes come from that backend.
Building a projection neither installs methods nor seals the source registry.
Sources remain rooted across callbacks, and a registry change during construction
rejects the mixed snapshot. The native `generation` test covers deterministic
bytes, inert construction, collecting factories and reentrant registration.

Publication, candidate mapping, ledgers, rollback and retirement are documented
with their owner in [Generation](../birnam-generation/README.md).
