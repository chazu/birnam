# Source compiler over Id

Current Oop migration checkpoint: synchronous and resumable source paths are
connected through explicit `Codegen compileSynchronous:` / `compileResumable:`.
The resumable path uses the retained Machine engine with `runBlock:budget:`,
`continueWithBudget:` and `executionState`. See the
[resumable milestone](../../docs/design/oop-resumable-milestone.md) for acceptance.
Suspension/control/recovery, default compiler and full-runtime integration remain
pending. Historical feature descriptions below are preservation requirements,
not claims that every feature is available. No v1 compatibility path is built.

Codegen consumes Lang CST messages and produces canonical portable programs and
compiler metadata. [Generation](../birnam-generation/README.md) owns publication,
explicit candidate rollback and image-application invalidation. The authored
SourceRegistry owns source registration. Codegen calls the separately loaded [NativeBackend](../birnam-native-backend/README.md)
through Id for native emission, slot coloring and descriptors. `compile:` returns
an owned resumable Method, `eval:` runs it on a real Machine, and `emit:` returns
the generated C String. TCC compiles and owns each Unit; its initializer consumes
Namespace and the Unit owner through the existing native module ABI.

Generated Oop modules initialized by TCC receive Namespace as their sole explicit
argument and use the supplied Unit's `declaredMethodAt:` and `retain:` operations.
For emitted C compiled by a system compiler and loaded directly through Id, pass
Namespace and an explicit retention owner. Initialization obtains native Methods
from the image's existing `newMethodAt:` and sends `retain:` to the supplied owner.
That owner must keep the Methods alive for as long as the generated code can use
them. No additional operation is installed on Id or its native image objects.

Ordinary source Reflect construction now sends the receiving facade
`methodForArity:implementation:`. The native implementation is a cold construction
seed; host bootstrap replaces it with the authored MethodInvocation policy after
control installation. That owner shares neutral forwarding templates and creates
fresh bindings through `withImplementation:`. Supplied resumable artifact templates
and identical image adapter payloads now bind through the current
`methodForTemplate:implementation:` hook. Synchronous artifacts retain their
separate Unit initialization. See [the source owner report](../../docs/design/method-invocation-owner.md)
and [supplied template integration](../../docs/design/supplied-method-templates.md).

The construction seed compiles the retained named-adapter algorithm. Its
Method keeps both the code Unit and the implementation object. Calls perform fresh
`value:` lookup on that implementation; replacement does not rewrite a captured
Method or a suspended frame. No generated Runtime service catalog is imported.

Ordinary Reflect installation sends the current vtable `addMethod:as:` with the
owned Method and selector. Removal checks direct presence and uses that vtable's
`removeMethod:`. Both retain the target-returning facade contract and no longer
resolve Store or Generation. They create no method description facts and are usable
while a program is running. Explicit Store publication keeps its selected policy
and description behavior. Temporary roots survive collecting owner calls.

Reflect `globalNamed:` queries current Namespace exports first, then current
Globals when the export is absent. A bound nil export still wins. `allGlobalNames`
returns the ordered Namespace export names through `count` and `nameAt:`; it does
not merge authored-only Globals. `parentOf:` uses the current Id `parentOf:`
query. These operations inspect no foreign payloads or private owner tables.

Image binding installation uses the same direct owner path. General snapshots
preserve actual live Methods even if an opted-in managed revision has drifted;
that metadata reloads as descriptive state, not a recovered activation capability.
See [ordinary editing and snapshots](../../docs/design/ordinary-edit-and-snapshot.md).

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
through `Globals at:put:`; nested bindings remain lexical. Generated
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
`lastError`, `evalPersisting:` and `evalPersisting:from:`
selectors now use the same current owners. Revision analysis remains private pure
compiler code; its revision Vector retains the tag, name, requirements, original source and
three historical digests, followed by the retained compilation record.

The authored SourceRegistry object in Globals owns the source registry and its
registration policy; all consumers use its current Id messages. Entries retain
ordinary compilation records: `#[#compiledSourceV1 originalSource metadata]`.
Source Strings keep their identity; no String-keyed cache or native metadata table
is added. Metadata retains the existing `compiledSourceMetadataV1` schema and
canonical artifact codec. Core code no longer reads or writes a
`#compiledSourceMetadata` Store fact; explicit facts under that name are independent.

`analyzeSourceMetadata:` returns a compilation record instead of true. Text is
analyzed through current parser sends; a supplied valid compilation record is reused
without parsing. `compiledSource:metadata:` constructs a value, `sourceTextOf:`
reads its text, and `retainedSourceMetadataOf:` exposes its structured metadata.
`compiledSourceFor:` accepts a record or finds a registered record by source
identity. `compiledSourceMetadataFor:` validates/encodes retained metadata and never
parses to repair missing or malformed data. Static module/asset queries analyze
Strings locally or read supplied records; neither creates facts.

`sourceRevisionOf:` retains the original seven fields and appends the owning
compilation at index 7. Index 3 is still the original source String. SourceLoader
passes the record into registration, avoiding a repeated parse. Candidate inventory
returns a fifth field containing its compilation record, so candidate artifact
inspection also retains its previous parse count. Revision snapshots keep their
own metadata when the current registry changes.

`recordModuleSource:named:` accepts text or a prepared record and answers its text.
`moduleSourceFor:` continues returning the original registered String. An identical
source declaration retains the first registration. `replaceCompiledSource:named:`
explicitly selects a new validated record for an existing name without installing
methods or activating code. The registry's content revision invalidates stale
optional certification observations, including ABA; ordinary candidate admission
compares only its selected record and owner. Ordinary sends acquire no observers
or method-identity guards. Duplicate source text uses the current String `=` send.

Image transports the ordinary record graph through the existing value codec.
Source ownership in images is now `#[#sourceRegistryV2 owner]`: the heap and
Globals preserve the actual owner, its state and aliases. Old registry inventories
and source-less images require rebuilding. Early bootstrap installs the authored
owner before named artifacts; no native registry seed is retained. Artifact
metadata encoding is unchanged. See [retained compilation records](../../docs/design/retained-compilation-records.md).

Loader-facing evaluation clears `lastError`, preserves nil/diagnostic versus returned
Error behavior, and uses owned Method/Block/Machine sends. It never calls a generated
entry address. Top-level Globals writes and ordinary dictionary edits retain their
existing behavior. The supplied filename reaches generated descriptors, including
nested Blocks. No GC policy is changed; temporary values remain explicitly rooted.

[Source Loader](../../docs/guide/native-loading.md#loading-source-files) is wired after Codegen and tested
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
source consumers and explicit fact-backed handles. Source Globals now owns its
bindings independently, including exact presence and identity-based keys.
Reflect installs owned Execution Methods directly through dictionary messages. Staged
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
  reconnected below. Image stages owned Methods and source registration, then
  applies them without automatic compensation or recoverable capability backups. Candidate activation, rollback, drift detection
  and retirement now run through the message-based lifecycle in [Generation](../birnam-generation/README.md).
  Its [acceptance cases](https://github.com/chazu/birnam/blob/3c4765089a016a01245e259e246b90472e76eaa5/rust/birnam-codegen-runtime/src/deferred.rs#L11675-L12555)
  and sibling `code_bindings.rs`, `materialization_cache.rs`, `state_contract.rs`
  and `finalizer_tests.rs` remain in the same Git tree. Reconnect only the behavior
  required by actual consumers and verify it with current owner messages.

Do not restore direct generated-entry calls, service tables, native mark callbacks
or GC suppression as substitutes for roots. No lock or borrowed foreign bytes may
span an outgoing send. Explicit Store publication preserves its MOP; executable references
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

## Image body demand

An image stage retains private immutable native bytes (mapped load) or canonical
backend requests (rebuild). `portableImageBlockMethod:synchronous:` loads or emits
only the requested program/disposition on its first use. It resolves the current
TCC/NativeBackend operations through Id, initializes the Unit, and caches only a
successful Method. Repeat requests return that Method; the other disposition
remains independently available. No stage borrow crosses a callback. The existing
image-operation reentry boundary still applies.

Stage creation validates inventory/bundle framing and prepares named adapters;
it does not initialize every body. Unrequested native payloads are not relocated
or checked for linker errors. Required body failures occur during heap restoration,
before normal image application begins. A failed individual request remains
retryable until cancellation. This changes optional loader/backend call counts
and their timing; those operations run at demand, not at stage creation.

Stage roots protect initialized Methods during restoration. Cancelling the stage
releases pending inputs and its Method roots; returned Blocks/Methods and entered
or suspended frames retain their actual Units independently. This introduces no
persistent native cache or inter-module ABI. Saving still includes both variants
in the existing format, and mapped loading needs no compiler. Production
`artifact_templates` tests cover demand, both dispositions, replacement, collecting
callbacks, rejected reentry, failed emission/loading/initialization, retry,
malformed requested objects and final Unit reclamation.

## Durable module artifacts

`compile:output:`, `compile:output:from:` and `compile:output:root:` write a
TCC relocatable native object through System File IO without evaluating source.
The filename/root variants preserve source-relative embedding. The object stores
the existing inert Artifact descriptor, original source, embedded payloads and
current generated Methods, plus compiler-produced source metadata. It advertises
`id-object-v3`. The historical `.dylib`
suffix remains for Module resolution; this payload is an ELF object for TCC,
not an OS-loadable Mach-O library. Native kernel dylibs keep their separate ABI.

`load:` reads one snapshot and sends `Artifact inspectBytes:from:` before passing
those same bytes to `TCC loadObject:`. It checks kind, capability and already-loaded
requirements, initializes an owned Method/source/metadata/adapter record, validates source identity
and digests, and runs the Method on a Machine. Successfully activated, revision-indexable modules enter
the authored source registry. Artifact dependency discovery preserves the broader
historical syntactic declaration scan, independently of revision admission. Conflicting duplicate sources fail before
user initialization. Errors and native failures propagate; nil means unsuccessful
activation. Ordinary user initialization is not a rollback transaction.

Activation decodes the compiler-produced metadata, checks its source digest,
identity and requirements against the descriptor, and persists ordinary metadata
bindings through their dictionary owners. It neither parses source nor compiles adapters.
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
then source-owned Globals, using live owner messages. Optional `storeSupport`
installs `factsOf:` by composing Store's rooted `liveTriplesDo:` snapshot and ordinary Blocks;
rows retain entity, attribute and value without changing collection settings.

## Initial generation projection

Ordinary `--compile-module SOURCE OUTPUT` artifacts already use resumable entries.
The former `--initial-generation-resumable` bootstrap switch is retired: source
registration comes from ordinary artifact admission. Host does not adopt or seal
an initial generation. Candidate-specific selections and adoption remain explicit.

`initialGenerationProjection` constructs the retained four-element projection:
`#[#generationProjection sourceMap baseDigest generationSelection]`. Source-map
rows are sorted by module name and contain the source revision and nil component.
The complete implementation inventory records the current source backend's
resumable disposition; named-adapter descriptor hashes come from that backend.
Building a projection does not install methods or adopt a baseline.
Sources remain rooted across callbacks, and a registry change during construction
rejects the mixed snapshot. The native `generation` test covers deterministic
bytes, inert construction, collecting factories and reentrant registration.

Publication, candidate mapping, ledgers, rollback and retirement are documented
with their owner in [Generation](../birnam-generation/README.md).
