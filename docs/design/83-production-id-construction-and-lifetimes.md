# 83 — Production Id construction and lifetimes

> **Status:** Production migration in progress, 2026-09-20. Exact-entry adapters
> and managed-turn checkpoints are implemented in the existing production
> runtime; the minimal Id and upper ownership cutover remains open.
> Resolves the four adversarial-review findings against
> [82](82-minimal-production-id.md). Design 82 remains the failure record and
> dependency constraints. This document specifies construction, native loading,
> collector activation and a simpler executable-lifetime policy.
>
> User clarification: Birnam must be safe for serious software, but its runtime
> should not attempt to anticipate every mistake by a module or application
> author. Preserve safety guarantees; justify mechanisms against actual supported
> operations rather than hypothetical misuse.

## 1. Safety boundary

Ordinary Birnam execution must not produce memory corruption through method
replacement, effectful lookup, collection, suspended execution, malformed
language-level driver results or an ordinary type/bounds error. Keep the checks
that make those operations safe. Replacing a protocol incompatibly may still
produce a language error or change the program's meaning; it need not preserve
the author's intended computation.

Loaded native modules and explicit unsafe/FFI implementations are trusted code.
They must obey argument, rooting, owner-thread and borrowed-pointer contracts.
Id does not validate arbitrary native pointers, sandbox native entry points,
repair unregistered native roots or defend against a module calling `dlclose`
behind the runtime's back. Safe language-facing wrappers remain responsible for
validating their language inputs. A native author violating the contract is not
an excuse to add another runtime authority system.

This follows the existing distinction in
[60E section 8](60e-resumable-registration-and-entry.md#8-keeping-the-microcode-late-bound)
between storage safety and semantic correctness after replacement, and
[60C section 13](60c-stepmethod-calling-abi-options.md#13-deferred-and-rejected-alternatives)
on trusted native artifacts. It changes the dispatch protection mechanism below,
not that distinction.

Do not introduce selector sealing, speculative mutation vetoes, a rollback
transaction around every send or per-send security tokens. Existing Store
transactions, Frame validation and live-reference ownership serve supported
features and are not removed merely because they are complex.

## 2. Dispatch: Id binds; Execution owns turns

**Id binds and invokes native methods. Upper Execution owns managed turns.
During a turn, removal changes future lookup immediately but cannot invalidate
an already obtained callable. Reclamation happens after the outer turn returns
to pinned code, subject to surviving owned references.**

### Bare Id

The Id message entry is a fixed native bridge, not a replaceable send method
whose own lookup needs another protection mechanism. Its sequence is:

```text
native message entry
  -> bind through the receiver's current lookup behavior
       -> recursively send lookup as required
       -> terminate at the explicit metacircular root binding
  -> if absent, resolve doesNotUnderstand: and forward the failed send
  -> invoke the exact selected native method once, or report native Unbound
  -> return
```

Keep the existing finite metacircular termination principle. Lookup and the
methods implementing construction remain replaceable. The fixed bridge neither
classifies resumables nor knows that Execution, GC or a compiler will exist.
It contains no execution callback, read epoch, turn counter or retirement queue.
Early Id and primitive native module code remain mapped for process life.

Keep `doesNotUnderstand:` as an ordinary Id protocol. An unbound original send
performs a fresh lookup of that handler and forwards the original selector and
arguments under the existing forwarding contract, including its current
sixteen-argument limit. The default handler returns native `Unbound` before Error
exists; Error later installs its ordinary behavior. If DNU itself is unbound,
return native failure rather than redispatching DNU indefinitely. Machine's
upper resolver applies this same rule and classifies the exact chosen handler;
it must not accidentally omit DNU or resolve the original selector twice.

### Managed execution

Execution supplies the application-entry boundary above Id:

```text
pinned entry wrapper holding the entry callable and argument roots
  -> enter an owner-thread Execution turn (nest if already active)
  -> run ordinary sends / Machine work / FFI reentry
       -> resolve each send once through Id
       -> invoke, or classify that exact answer in Execution
  -> retain the outward result; return through all generated native frames
  -> leave the outer turn in pinned runtime code
  -> process eligible retirements
  -> return the still-rooted result to its owner
```

Removing the last binding during lookup cannot destroy the selected entry,
descriptor, implementation anchor or classification record during that turn.
Deferring only native unmapping is insufficient: the exact answer must remain
logically callable too. Nested sends, callbacks and nested Machine runs share
the turn. They cannot independently drain retirements.

Methods installed for generated code have an ordinary native adapter. That
adapter enters **its exact registered implementation**, not a new lookup of the
original selector. The current emitter in
[`resumable_c::emit_entry_thunk`](../../rust/birnam-codegen/src/resumable_c.rs)
formerly resent the selector; retaining that behavior with bare Id would recurse indefinitely.
Migrate the adapter and its registration owner together.

Machine's existing stackless path stays in upper Execution: ask Id to resolve
once, classify that exact native answer, and acquire the implementation for the
new Frame. It need not call the synchronous adapter for every stackless call.
The borrowed binding remains valid for the active turn; a Frame that outlives
the turn holds an owning implementation reference.

### Entry ownership and trusted native calls

An external callback must already own its callable **before entering its
generated instructions**. A pinned FFI/application wrapper holds that reference
and enters the turn before dispatch. Starting a turn inside an otherwise
unowned generated wrapper is too late. Compiler-generated safe interfaces and
ordinary application entry paths provide this ownership automatically.

A native module invoking potentially reclaimable bindings through raw Id must
enter Execution's turn protocol first. Retaining a raw method address beyond
that turn is an unsafe contract violation; retain an owned callable instead.
This obligation belongs to the native caller and upper Execution, not an empty
future-Execution slot in Id. Before Execution exists, construction publishes
only pinned native implementations and needs no managed turn.

This is owner-thread execution, matching the current runtime. Cross-thread
callbacks must enter the supported owner-thread mechanism; this design does not
invent concurrent code unloading.

### Retirement and checkpoints

A retirement request queues work. It cannot invalidate code during a turn.
Bindings/aliases, admitted units, active or suspended Frames, Continuations,
owned callables and retained code owners still keep implementations alive.
Ownership required across turns is not replaced by the turn counter.

A checkpoint requires return through **every generated/native activation that
could borrow reclaimable code** into the pinned outer entry wrapper. A bounded
Machine run is a checkpoint only when it returns through that complete boundary.
A Machine yielding beneath a synchronous generated caller is not one. Long
synchronous native calls deliberately delay code reclamation; they are not
interrupted to manufacture a quiet point. Long-lived services should use the
existing bounded Machine driving interfaces between turns.

Checkpoint work may collect or run finalizers. Keep the outward result and
pending owners rooted. While draining, callbacks enter a new protected turn;
an in-progress-drain flag prevents recursive draining. Recheck ownership after
callbacks return. Respect the collector's existing prohibition on recursive
collection. No callback runs while an internal registry borrow is held.

All physical release paths use this rule, including mapped-owner finalizers,
failed candidates, retained-generation reaping and artifact disposal. A request
that is queued is a normal outcome, not an abort or permission to `dlclose`.

When GC finalizes a code-owner object, its finalizer **transfers** the native
handle, descriptors and required ownership metadata into a queue-owned record
before the object's payload is freed. Clear ownership in that payload to prevent
double release. The queue must not retain a borrowed Oop to the dead owner or
dereference/resurrect it later. Any other live object references required by the
record are rooted separately until release. This is ownership transfer, not a
promise to keep a finalized object alive until the next checkpoint.

Relevant migration sites are [`ExecutionState`](../../rust/birnam-id/src/execution.rs)
(`classify_resumable`, binding removal, `end_dispatch_read`),
[`Runtime`](../../rust/birnam-id/src/runtime.rs) (retirement and Machine entry),
and [Codegen runtime](../../rust/birnam-codegen-runtime/src/lib.rs) (mapped-owner
finalization and candidate disposal). Remove the old per-send protection only
when all those paths use the upper turn boundary.

This deliberately changes cleanup scheduling from a send's read-scope exit to
an outer-turn checkpoint. It preserves safety, fresh lookup and eventual
eligible reclamation; it does not promise identical diagnostic reader counts
or the same instruction at which a retired mapping is released. GC of ordinary
objects can still occur within a turn. Code owners discovered dead by GC queue
their executable release until the checkpoint.

## 3. The common native loading protocol

These are semantic ABI requirements. Freeze a C layout and versioned independent
oracle during the production implementation; do not preserve the twelve-argument
initializer as a compatibility layer.

The native Id carrier identifies a live message entry, its context/Id receiver,
and the primordial interning and loading selectors. Include version and byte
size for admission. Other capabilities are obtained through messages. There is
no field per future service.

The ad-hoc Siphon carrier implements only the first loading request. Its temporary
receiver/selectors are meaningful only to that carrier. The Id initializer builds
the actual graph and returns a complete real carrier with its own identities;
callers must replace the temporary carrier rather than copy its tokens into Id.

Use the following native operand/result kinds, distinct from language objects:

| Kind | Meaning and lifetime |
|---|---|
| Object | An actual Id object reference. The owning caller keeps it live for the call. |
| Word | An unboxed size, status or native scalar; never a traced Oop. |
| Bytes | Pointer plus length, borrowed for the synchronous call. Retention requires copying. |
| Native entry | A correctly typed native implementation address; its image must remain mapped. This is not a language-level arbitrary pointer. |
| Protocol | A complete admitted Id carrier with a live owner. Bootstrap returns this kind. |
| Unit | No value; does not depend on Nil. |

Message selectors after bootstrap are real interned Symbols. A native caller
uses the carrier's interning selector to obtain them; the loading selector is
available without String or a general namespace. Native method arguments and
results carry their kinds explicitly, including borrowed method answers from
lookup. Language-facing reflection must expose an owned callable or a checked
language representation, not smuggle an unowned function pointer into an Oop.

The loading message receives a byte path and an explicit list of initializer
operands. The operands may name already available dependency objects. Id passes
them unchanged; the initializer defines and checks its required inputs. There
is no universal bag containing every runtime service and no missing-provider
placeholder. Namespace-based discovery is a later convenience.

The result is either:

- `Loaded(value)`: the fixed, versioned native entry completed initialization.
  Id's first entry returns `Protocol(real_id)`; later entries normally return
  an Object representing their exports/owner, or Unit. A returned Protocol is
  adopted explicitly by its caller, not silently installed as a global override.
- `Failed(code, native_detail)`: a native status covering invalid request,
  mapping failure, missing entry, incompatible entry ABI or initializer failure.
  Neither member requires String, Nil or Error. Diagnostic text is formatted by
  the caller from these codes; borrowed stack text cannot escape as a result.

The loader opens the native file, discovers a fixed descriptor symbol, checks
its version/size and calls the admitted entry. Missing/incompatible entries fail
before explicit initialization. OS library constructors are trusted native code;
shipped bootstrap libraries must not run Birnam semantic initialization there.
The loader does not parse source, images or language artifact manifests.

Once an initializer has been entered, the primitive loader pins its native image
for process life, including failure after partial publication. It does not try
to undo arbitrary initialization effects or unload methods that may have escaped.
The result reports failure; startup stops on failure of a required module. A
higher-level transactional Module loader may supply stronger publication rules.
Opening/version failure before entry may close the image normally.

This pinning rule applies to primitive native implementation modules. Reclaimable
generated artifacts are owned by the later artifact/Execution loader, whose
entry ownership and retirement follow section 2. It is not permission to pin all
generated programs forever.

## 4. Construction with no dormant dependencies

**Available means every operation advertised by that initialization result is
callable, including its error paths, using only already available predecessors.**
An allocated container with methods waiting for future provider slots is not an
available module. Not-yet-installed extensions are absent, not stubs or promises.

The table gives the target construction order by responsibility. Rows may share
a production library, or a library may expose explicit base and extension entry
points. No new library or design document is required per row. A later entry
only installs behavior when all its named inputs exist.

| Step | Responsibility and predecessors | Complete behavior available now / later extension |
|---|---|---|
| 0 | Id; platform only | Storage, Symbols, behavior, bind/invoke, primitive native loading. No values, Namespace, GC or Execution. |
| 1 | Bootstrap; Id | Sequential loading, retained initialization results and raw failure reporting. It calls the real Id loader for each subsequent step. |
| 2 | Lifetime; Id, explicit bootstrap-owned roots | Root/owner records, payload descriptions, finalizer declarations and native storage ownership. These are working records above Id; no collector is required to create/query/release an owner. |
| 3 | Base identities; Id + Lifetime | Nil/Boolean objects, identity and basic nil/boolean operations. Printing and Block control methods are not installed yet. |
| 4 | Namespace and Object Model; steps 2–3 | Publication/import and private-behavior/owner operations. Enumeration initially uses native operands; Integer/collection-valued convenience comes later. |
| 5 | String storage; steps 2–4 | Own/copy/read bytes and construct String objects through native operands. No Integer-valued length/index methods, numeric conversions or collection methods. |
| 6 | Error values; steps 3–5 | Error construction, kind/message access, recognition and propagation. String storage is already usable; no Integer or Float dependency. |
| 7 | Integer; steps 2–6 | Representation, checked integer arithmetic/comparison and real error paths, plus printing using String storage. Mixed Float operations and `asFloat` are absent. |
| 8 | Float/Word and numeric/text extensions; steps 2–7 | Construct Float/Word, then install mixed arithmetic and conversions on existing numeric behaviors. Complete String length/index/numeric methods and Nil/Boolean printing after their inputs exist. |
| 9 | Collector activation; Lifetime + Id storage facts, native stack support, tracer/coordinator | Adopt the existing heap and owners as section 5 specifies. Install collection policy only after its complete dependencies exist. No Machine/compiler needed. |
| 10 | Execution and Block representation; completed values, Namespace/Object Model, Lifetime/GC | Turn ownership, implementation/Frame ownership, Machine and native Block calling. No source compiler, speculative native tier or artifact-based admission yet. |
| 11 | Collections, Store and remaining value/control extensions; steps 2–10 | Vector/Map/Range/String collections, Cells/Globals and Store observation, Block loops and Boolean Block-taking control. Compiler/source reflection extensions remain absent. |
| 12 | Artifact codec, PEG, Lang and native compiler services; steps 2–11 | Existing parsing and native compilation with their real Error, String, Vector, Block and Store dependencies. Lang follows PEG. No hidden compiler remains in Id. |
| 13 | Codegen and artifact admission; steps 10–12 | Lowering/emission/loading, exact-entry adapters, admission/ownership and source reflection. Add Store/compiler integration now. Optimization is an upper extension with these services available. |
| 14 | System/module/image/application composition; preceding required owners | System IO can mount as soon as its values are ready; source loading waits for Codegen. Image loading waits for Codegen/Store/portable-code owners. Isolate/relational/Alien and authored modules mount with their declared dependencies, then CLI/application execution. |

Bootstrap knowing which file to load next is ordinary sequential construction.
It is not a callable earlier module advertising behavior that requires a future
provider. Its own loading/failure operations work at step 1. Lifetime does not
live in Siphon or Id and does not become another mandatory Id initialization
argument. Bootstrap's small initial root set is explicitly adopted into Lifetime.

The current Integer/String/Error cycle is resolved by steps 5–8: byte storage
precedes Error; Error precedes integer arithmetic; Float adds mixed operations
after Integer exists. Split representation from methods where necessary. A
native `String length` query returning a Word can exist at step 5; the language
`String size` returning Integer cannot be advertised until step 8. Preserve the
final language selectors and arithmetic/error behavior once their extensions
mount. There is no late-filled Float/String/Error slot in Integer's base owner.

Apply the same rule to other concrete forward references: Nil printing uses
String; PEG result production uses Vector; TCC Block construction uses Block;
Codegen uses Store/Globals; Image uses Codegen and portable code. Splitting their
initialization behavior is required, not merely moving `SCRIPT_MODULES` into a
different file. Module initialization must not compile Birnam source until the
compiler is actually available.

The production bootstrap manifest must name the actual entries implementing
these responsibilities, their predecessors and advertised operations, including
late extensions in step 14. Map every current `SCRIPT_MODULES` entry to that
manifest or its removal. Reject missing predecessors/cycles in the build check.
The table is the target split; it is not a claim that today's entry points
already conform. Complete the entry-level mapping as part of the construction
change, before admitting the full bootstrap path.

## 5. Collector activation is an explicit transition

Use one Id-owned physical heap throughout. Lifetime owns the upper root records,
payload descriptions and native finalizer declarations before a collector exists.
Modules register their owned references **before publishing/populating state that
uses them**, as Integer's existing initialization already does. Lifetime describes
actual current ownership; it does not reserve a collector callback in Id.

Before activation, GC is off. Bootstrap retains initialization results and any
early native roots, including its own state and Lifetime's initial owner. At
activation these are ordinary roots, not permanent marks on all boot objects.
Id exposes its own primordial references, dictionary edges, allocation identities
and storage integrity operations. Upper roots/layouts do not get copied into Id.

Activation proceeds on the owner thread, outside application execution:

1. Enter a non-reentrant activation phase in Bootstrap/GC. Keep all bootstrap
   owners live. Pause new module publication; allocation remains non-collecting.
2. Register/adopt Bootstrap's early roots into Lifetime and enumerate the full
   Id allocation set and Id-owned edges. Verify that each non-Id representation
   has the required description/native trace ownership. Id's scoped storage
   borrow ends before any upper callback runs.
3. Establish the native stack anchor and root providers; load the complete tracer,
   coordinator and finalizer policy. No collection has yet occurred. Missing
   descriptions or providers fail activation without reclaiming anything.
4. Install GC-aware construction behavior through the existing Id protocols.
   Cover instance allocation, cloning, interning and behavior construction,
   including native consumers. Primitive internal allocation is non-collecting;
   ordinary callers cannot bypass the installed construction protocols. Finish
   installation while collection remains disabled and publication paused.
5. Publish the Active collector state only after all those operations succeed.
   Release the temporary bootstrap root set only after Lifetime owns its live
   references. The first collection now uses the same heap and identities.

If activation fails, leave collection disabled and retain the bootstrap owners;
required production startup fails with a native diagnostic. No partial sweep,
automatic retry or elaborate rollback is needed. Reentrant activation fails.
Installed GC wrappers must remain non-collecting until the Active state is set.
Activation itself runs trusted bootstrap code, not application callbacks.

After activation, each new module uses Lifetime to establish roots and payload
descriptions before publication. Collection cannot call language code while it
borrows Id's allocation records. Separate snapshot/preparation, marking and
metadata retirement from foreign callbacks and storage release. Id retires its
own Symbol/behavior indexes before address reuse; upper owners retire their own
metadata. Existing finalizer and deferred-materialization semantics remain
explicit upper responsibilities.

The existing diagnostic `withStorageView:` is not the collector protocol: it
reports native/provider coverage gaps and rejects collection/Execution states.
Retain its external BUSY behavior in the upper diagnostic service. Supply the
collector with narrowly scoped Id storage facts and Lifetime's complete roots,
not a relaxed diagnostic check or an opaque full-Runtime capability.

GC may run at safe allocation points during a managed turn, using generated
roots and the supported native stack/root conventions. It may queue dead code
owners, but section 2 controls their physical executable release. Moving policy
does not require identical collection counts at every internal constructor;
it requires automatic collection, complete roots and preserved object/lifetime
semantics. Any changed timing assumption in an existing test must be identified,
not silently deleted to get the suite passing.

## 6. Review closure and production acceptance

| Review finding | Decision | Required production evidence |
|---|---|---|
| Unspecified protected dispatch | Fixed Id bind/invoke; upper managed turns and outer checkpoints | Effectful lookup removes/replaces its own or the target binding and still invokes the exact selected implementation once. Next lookup sees replacement. Native and Machine paths agree. |
| Undemonstrated dependency order | Explicit base/extension stages and entry-level predecessor manifest | With successors absent, exercise each meaningful construction prefix and its advertised failure paths. In particular: early Integer overflow/type error works without Float; Float later adds mixed arithmetic; String representation works before Integer convenience. |
| Missing GC adoption | Disabled → adopting → active transition, with Lifetime above Id | An early object reachable only from early module native state survives the first collection, then is reclaimed after that root is removed. Exercise failed activation, weak Symbol/behavior retirement and exact-address reuse. |
| Missing native load result/failure contract | Typed operands and Loaded/Failed, common entry descriptor, native image pinning after entry | First load returns the real carrier; it loads real successor modules. Missing/incompatible entry fails without publication. Partial initializer failure cannot leave dangling code addresses. |

Also cover outward-result rooting during checkpoint GC, an external callback's
owned entry, nested FFI reentry, finalizer reentry, suspended Frames/Continuations,
aliases, DNU in native and Machine dispatch, and every code-release path. Finalize
a code owner during a turn and verify its detached queue record releases exactly
once after the owner payload is gone. Verify that a bounded driver return beneath
a synchronous caller does not release code prematurely, and that release occurs
at a later eligible outer checkpoint. Do not replace these with arbitrary
invalid-native-pointer tests.

Use the same installed Id for independent startup and the complete production
source/compiled/image/worker checks. Id must initialize with all upper libraries
absent. Source/Cargo and private-export audits must show no upper runtime hidden
inside it. Prefix checks must exercise real shipped entries; no new seed or
one-off experiment. Debug validation is sufficient; release validation remains
waived.

An independent follow-up review accepted the managed-turn architecture and found
the four original architectural gaps addressed. It identified two additional
details: ownership transfer before a finalized code-owner payload is freed, and
preserving DNU redispatch. Both are specified above and included in acceptance.
This is design review evidence, not a claim that the production implementation
or its tests have passed these new requirements.

Keep the three production work packages in Design 82. Implement these decisions
within them, commit coherent progress and remove superseded factories, adapters
and initialization paths. Update the manifest and startup diagram with the
executed result. The design is a target contract; implementation and tests must
establish it before declaring Id decoupled.

### Implementation evidence and remaining boundary

The production emitter now enters the exact registered adapter rather than
resending its selector. JIT symbol binding and native TCC exports use the same
entry; current image runtime ABI 6 (`5582a42725e647a6`) retains that required
import, first admitted by ABI 5.
Machine's missing-send path now proceeds directly to fresh DNU lookup rather
than repeating the original lookup. A paired native/Machine regression verifies
one original lookup and preservation of the forwarded argument.

Public application sends now establish an Execution turn; nested sends share
it. Machine driving retains the turn across its complete native extent. The
per-send/per-lookup guards and the optimizer's shorter read scope are removed.
The outer pinned return roots its result, ends logical protection, then drains
retirements with a reentrancy guard. Explicit GC also checkpoints when no turn
or collection is active. Bounded driver returns beneath a caller do not drain.

TCC Unit and mapped-image finalizers transfer their native handles into detached
queue records before their Oop payloads are freed. Candidate and CodeBinding
reaping also schedules checkpoint work. A retained CodeBinding can consequently
disappear at the preceding outer return: a subsequent explicit reap can report
zero. The lifecycle smoke now asserts this timing and verifies that self-removal
still retains the executing binding inside its turn.

Debug evidence covers exact adapter entry after binding replacement, checkpoint
GC with native stack roots excluded, nested callback protection and nonrecursive
draining, and release after a finalized owner payload is gone. The installed
source/compiled/source-free checks, mapped/rebuilt/baked image checks, portable
resumable eviction and CodeBinding replacement/self-removal checks pass.
Alien's external callbacks already retain their Block and TCC Unit in the
callback registry before exposing their address. Its explicit unsafe release
contract remains the native caller's responsibility; nested callback sends use
the active turn.

Object allocation now uses `ObjectMemory` inside Id without GC services. It
contains addresses, extents and identities; `Heap` supplies marks, finalizers,
budget accounting and deferred allocation separately. The allocation identity
counter is back inside Id, and its Storage factory, Id dependency export and
host preparation are removed. `Heap::adopt` accepts existing allocations without
copying objects or resetting identities. This is an internal implementation
boundary, not a new module or service.

The split preserves object layout and payload-byte accounting. Native metadata
now uses a three-word allocation record and a separate two-word GC record in
two maps, replacing the combined four-word record in one map. That overhead is
outside payload-byte accounting. The adoption unit test exercises nonempty
allocation handoff. Production now builds `InitialObjects` with Id allocation
first, then adopts those objects into Heap and Behaviors. This does not yet
prove staged collector activation or early module-root adoption.

Allocation validation passes in debug: Host 86, Id 299 and Storage 7 unit tests;
a fresh installation of 53 native libraries and 28 authored modules; the loaded
identity/finalization fixture; source, source-free compiled, mapped/rebuilt/baked
image checks; and deferred-index loaded execution with forced recovery. The
installed Storage exports match its manifest; installed Id matches its bare
manifest plus the two published wrapper exports.

Id's `Vtables` and `Symbols` now use only object allocation and their own
indexes. `Behaviors` composes the dictionaries with GC layouts, trace hooks and
private-owner metadata; Heap adds accounting when Symbol creation allocates.
GC chooses dead objects, and Id retires their weak dictionary/name records before
allocation release. No stored provider or GC callback is needed by these core
implementations. The key index and mutation counter are local again; their
factories and two more host preparations are removed. Seventeen of the original
twenty pre-Id preparations remain until the upper construction cutover.

The behavior payload is now the five-word dictionary view (40 bytes): capacity,
tally, keys, values and parent. Layout, hook and private-owner fields no longer
occupy its payload. Existing language-facing metadata operations still use the
separate upper records. The key-index threshold, ordered mutation transcript and
epoch saturation are preserved. Image runtime ABI 6 (`5582a42725e647a6`) separates
this representation from older native images; image format 8 and IdApi v20's
table shape are unchanged. The aggregate production Runtime still composes these
parts inside the Id crate pending the loading/ownership cutover.

This boundary passes 300 Id, 86 Host, 12 Image and 4 Object Model debug unit
checks, including construction of the actual dictionary/Symbol implementations
using only Id allocation. A fresh 53-native/28-authored installation passes the
loaded layout fixture, dictionary source/compiled/mapped/rebuilt/baked image
workload, and layout image workload. The latter verifies callback-time layout
replacement, nested collection, rejection during encoding and successful image
saving after restoration. Installed Id and Object Model export checks confirm
the removed dictionary factories and dependency entries are absent.

Production `build_bootstrap` now constructs the primordial Object/Symbol/behavior
graph through `InitialObjects` before creating Heap or behavior metadata. Upper
construction adopts those exact addresses and allocation identities, then adds
Namespace, Nil/Boolean and the remaining runtime. The initial `lookup`,
`doesNotUnderstand:` and `Symbol` names are consequently the first three interned
names; the old aggregate constructor interned the latter two later. Method and
behavior enumeration order is unchanged. Initial behaviors and Object retain the
existing permanence policy; initial Symbols remain collectable and use the normal
runtime roots. The host still prepares upper services before entering `id_init`,
and primitive method installation still happens in Runtime: this is the actual
production graph handoff, not completed independent Id initialization.

The graph handoff passes the 301-test Id debug suite, including a check that
adoption and first collection preserve addresses, identities and interned names.
The fresh 53-native/28-authored installation passes the allocation-identity
source, source-free compiled and mapped/rebuilt/baked image workload, including
finalization, active Frames, live behavior replacement, aliases and reload.

These changes establish the lifetime mechanism, **not independent Id**. Its
registration owner, turn state and temporary private retirement queue export
still reside in the existing Runtime inside `birnam-id`. Move these together
to upper Execution during the ownership cutover and migrate their consumers;
do not retain Id forwarding exports as the final architecture. The typed loader,
base/extension construction entries, staged collector activation and removal of the
pre-Id preparation path remain unimplemented. No architectural acceptance is
claimed from the regression results above.
