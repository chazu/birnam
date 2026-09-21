# 83 — Production Id construction and lifetimes

> **Status:** Production migration in progress, 2026-09-21. Exact-entry adapters
> and managed-turn checkpoints are implemented in the existing production
> runtime. Core Id allocation, Symbols, dictionaries and typed dispatch no longer
> require upper services. Siphon now loads the real Id through the common native
> protocol before upper preparation. Runtime now borrows the native Id carrier
> and original entries through the temporary [v15 constructor](../../abi/runtime-construction-v15.md),
> without taking Id's Rust owner or referencing its primitive functions for
> lookup/Integer admission. Id is independently packaged with only the ABI crate;
> Runtime/compiler code is in a separate upper library loaded through Id. Native
> Block and native value/control operations now precede compiler modules.
> Compiler admission is now a later explicit extension. Language reflection now
> returns owned Methods, including anonymous synchronous/resumable Units;
> native binding references live in Lifetime above Id. Unsafe native consumers and the full entry-level
> construction manifest remain open.
> The loaded Bootstrap now owns the production construction sequence through
> Runtime; host prefix requests delegate to the same implementation.
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

Relevant migration sites are [`ExecutionState`](../../rust/birnam-runtime/src/execution.rs)
(`classify_resumable`, binding removal, `end_dispatch_read`),
[`Runtime`](../../rust/birnam-runtime/src/runtime.rs) (retirement and Machine entry),
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

The native C layout is frozen in
[`id-protocol-v1/id_protocol.h`](../../abi/fixtures/id-protocol-v1/id_protocol.h)
and mirrored by [`id_protocol.rs`](../../rust/birnam-abi/src/id_protocol.rs).
The current production Id dictionaries and core dispatch use these typed
operands. The common descriptor loader and production Siphon handoff are wired.
Upper modules still use the aggregate construction path after Id has loaded;
do not preserve that twelve-argument initializer as the final compatibility layer.

On the supported 64-bit target, an operand is a 32-bit kind, 32 reserved zero
bits and a 16-byte union (24 bytes total). A native entry is a function and native
context (16 bytes); its function takes context, receiver, operand pointer and
count. The context is untraced native data with a caller-retained owner. An
outcome contains status/reserved words, scalar detail and an operand (40 bytes).
The carrier contains version/size, message function, context, receiver, intern
selector and load selector (48 bytes). The fixed descriptor symbol is
`birnam_native_module_v1`: version/size and an initializer function (16 bytes).
The initializer receives the current carrier and explicit operands. Loading
uses a Bytes path as its first argument and passes the remaining operands
unchanged to that initializer. Interning takes one Bytes argument.

Success is status zero. Failure codes distinguish invalid request, mapping,
missing entry, incompatible ABI, initializer failure and unbound dispatch.
Reserved fields are zero. Bytes results may borrow caller-owned inputs or
storage whose owner the caller retains; they must never refer to a callee's
stack. A Protocol result points to a complete carrier with a retained owner.

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
| Native entry | A correctly typed native implementation and its native context; their owners remain live. This is not a language-level arbitrary pointer. |
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

### Initial Id object operations

The real Id descriptor installs these operations through its own message sends,
before any upper module exists. They use the existing typed operand ABI and
add no carrier fields. `Id` below means the carrier's receiver; a behavior is an
Id-registered behavior object.

| Selector | Receiver and arguments | Native result |
|---|---|---|
| `objectPrototype` | Id, no arguments | The initial Object prototype |
| `behaviorOf:` | Id, Object candidate | Its behavior, or Unit for an unknown candidate |
| `delegated` | Parent behavior, no arguments | A new empty behavior delegating to that parent |
| `allocate` | Behavior, Word byte count | A zeroed object with that behavior |
| `allocationAt:` | Id, Word address | Exact live allocation Object with its original pointer provenance, or Unit; never dereferences an unknown address |
| `allocationCount` | Id, no arguments | Current live allocation count as Word |
| `allocationSizeOf:` | Id, Object candidate | Requested payload bytes as Word, excluding header/padding; Unit if unknown |
| `identityOf:` | Id, Object candidate | Stable allocation identity as Word; Unit if unknown |
| `symbolBytesOf:` | Id, Object candidate | Borrowed name bytes without the trailing NUL; Unit for a non-Symbol |
| `reassociateBehaviorOf:with:` | Id, Object receiver and allocated Object behavior | Updates the header and dictionary mutation epoch; returns receiver |
| `behaviorCount` | Id, no arguments | Registered behavior count as Word |
| `behaviorAt:` | Id, Word index | Behavior Object, or Unit beyond the registry |
| `selectorAt:in:` | Id, Word index and Object registered behavior | Direct selector Object, or Unit beyond that dictionary |
| `parentOf:` | Id, Object candidate | Parent Object, or Unit for absent/unknown parent |
| `parentOf:put:` | Id, Object registered behavior and Object parent (or Unit/null to clear) | Updates delegation and mutation epoch; returns behavior |

Object payloads are aligned to a native word and their backing storage is rounded
up to a native-word boundary, including zero-initialized tail padding. The public
extent query still reports the requested bytes. A collector can scan those padded
words using this allocation contract; it does not read an Id allocation record or
infer a header layout. Representation writers stay within their requested extent.

Wrong operand kinds/counts and invalid allocation sizes return InvalidRequest.
The fact queries check exact registry membership; candidate queries may return
Unit without dereferencing an unregistered candidate. The caller retains a
Symbol throughout use of its borrowed name bytes. These native operations need
no String, Integer, Nil/Boolean, Namespace, GC or Execution representation.

After upper adoption, Runtime replaces `allocate` and `delegated` with completed
GC-aware implementations under the same selectors, just as it replaces interning
and mutation. Those entries account for new allocations and behavior metadata
and protect the receiver through their allocation/checkpoint extent. Id's fact
queries continue to read the same registries. Native callers retain their own
objects across calls and supply upper payload descriptions and roots before GC.
The old all-object native convention is still adapted separately during migration.

### Id storage traversal

These additional selectors use the same carrier and operand ABI:

| Selector | Arguments to Id | Visitor arguments and result |
|---|---|---|
| `rootsDo:` | Native entry visitor | One Object per actual Id-held reference; returns Unit |
| `allocationsDo:` | Native entry visitor | Object, Word requested payload bytes, Word allocation identity; returns Unit |
| `referencesOf:do:` | Object candidate, Native entry visitor | One Object per Id reference; returns Word representation kind, or Unit for an unknown candidate |
| `retireIndexesFor:` | Bytes containing native-endian allocation addresses | Retires Id behavior/Symbol indexes and weak keys naming those addresses; returns Unit |
| `detachAllocationAt:` | Word allocation address | Removes the allocation from Id and returns a one-use native release entry; Unit if unknown |

Representation kinds are 0 for an ordinary allocation, 1 for a registered
behavior, and 2 for a Symbol. Reference visitation reports the header behavior
first, including a null header if unset. A behavior then reports its parent
(including null) and strong method keys in dictionary order. Weak keys, native
entry contexts, and upper owner/payload references are excluded. Keeping the
parent position lets an upper collector insert its owner edge before the keys.
Symbol bytes contain no Id object references. A visitor to roots/allocations
receives Id as its receiver; a reference visitor receives the queried object.

Each operation copies its facts before invoking the visitor. No Id registry
borrow crosses the callback. A nested query or allocation is permitted; an
allocation made by a visitor is absent from the already captured allocation
snapshot. The caller keeps snapshot objects live and coordinates collection or
reclamation. This is borrowed enumeration, not a new owning snapshot service.
Allocation/root enumeration makes no ordering promise. A failed visitor stops
visitation immediately and returns its complete native outcome unchanged;
successful visitor values are ignored. Wrong operand kinds/counts or a visitor
without a method produce InvalidRequest.

GC acquires these five native callables through Id's `lookup` messages during
its construction, before application execution. It retains those exact entries
and their owners, then invokes them during collection without another lookup.
Running arbitrary application lookup merely to inspect an allocation would
introduce allocation and mutation into marking. This is ordinary retention of
selected callables: the methods remain replaceable, and a later message or new
consumer observes the replacement. It does not seal selectors or change fresh
lookup for application sends. Initial Id code stays pinned, and the collector
retains Id's state; another implementation must provide equally live owners.
No future GC callback or upper root record is stored in Id.

Retirement is a trusted native storage operation. The caller first establishes
that allocations are unreachable and retires upper metadata and binding owners.
`retireIndexesFor:` borrows an array of native `usize` addresses encoded as Bytes;
its length must be a whole number of addresses. Duplicate/unknown addresses are
harmless. It does not free payloads or invoke callbacks. `detachAllocationAt:`
then removes one exact allocation while preserving its payload. The returned
entry owns that detached allocation and releases it when called with no arguments.
Invoke it exactly once, after the finalizer and every other payload access have
finished. There is no GC/finalizer callback in Id and no borrowed dead Oop stored
for later cleanup. Unknown addresses return Unit without detachment.

These operands are native Bytes/Word, not ordinary language Objects. Passing
Object operands fails before mutation. Safe language wrappers do not expose raw
retirement/release, and native callers must coordinate the complete retirement
sequence with Lifetime/GC. The collector removes upper per-object metadata before
Id detachment, retains the returned release entry across the finalizer, then
invokes it through the existing collection coordinator's reclaim operation.

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

**Production migration is in progress. Id is now independently packaged.**
The production Id crate depends only on `birnam-abi`. Its dylib exports the native
module descriptor and identity; Runtime and compiler/artifact code live in the
separate `birnam-runtime` crate. Both are installed and the host initializes
Runtime through Id's loader after its predecessors. Compiler admission is an
explicit later extension; base Execution has its value dependencies at construction.
Language reflection now returns owned Methods through the checked language-send
entry used by TCC and Portable Code. Unsafe native consumers still use the frozen
raw Method convention. Registered generated implementations retain their
generation; native functions retain their image. Anonymous synchronous and
resumable TCC functions retain their owning Unit. Bindings retain that same Unit
through Lifetime v2 native references, without permanently rooting unreachable
cycles. Native consumer migration and the complete construction graph remain
unfinished. See [the language boundary](../../abi/language-send-v1.md).

The Method production check is `scripts/method-values-smoke.sh PREFIX`, also
included as `method-values` in routine/exhaustive validation. It passes source,
source-free compiled modules, mapped/rebuilt images and baked execution, with
replacement, removal, aliases, custom lookup, malformed lookup results and GC.
Both ordinary and privately customized Method instances refuse image saving.
The native construction-prefix fixture also retains distinct native contexts,
invokes after replacement/removal, and collects during invocation before the
compiler extension exists. The focused Runtime/TCC/Portable Code tests pass
324 tests (17 ignored); the installed Portable Code suite passes 114 assertions.

The synchronous portable-code eviction assertion formerly required unmapping
inside the enclosing turn; it also failed on the preceding committed build.
It now asserts deferral like the resumable cases. The existing
`scripts/portable-resumable-lifecycle-smoke.sh` checks both synchronous and
resumable materializations across separate real REPL turns, proving eviction
survival and actual subsequent native-unit release. Debug installation and the
workspace diagnostic build pass; release validation remains waived.

The host's [`Siphon`](../../rust/birnam-host/src/siphon.rs) supplies the small ad
hoc loading protocol and invokes `birnam_native_module_v1`. The real Id descriptor
constructs `InitialObjects` using only Id allocation, dictionaries and Symbols;
its own bind/invoke path then installs native loading, construction and storage
operations. Core Id dispatch has no Runtime, GC, Execution or future-provider
slot. Native descriptor admission closes rejected images before entry and pins
entered images, including failed initializers with partial publication.

Upper startup uses the same objects and carrier. Id keeps its registries; each
registry has one owner and a stable address. No graph is copied and no carrier
slot is retargeted. Six complete supporting factories now load through Id from Method Observation,
Storage and Dispatch Cache. Runtime receives copies explicitly; the host no longer
prepares dependency globals or maps Runtime ahead of native initialization. That
ordering improvement does not implement the construction graph in section 4.

The first upper native entry is now
`libbirnam_bootstrap.dylib:birnam_native_module_v1`. Its only predecessor is Id.
Its [native owner](../../abi/fixtures/bootstrap-v2/README.md) calls Id's current
loading message for every subsequent native construction. It retains Object and
Protocol initialization results, including nested loads, before releasing
argument roots. Before Lifetime exists those references form a working early
root set. Explicit adoption installs actual records in the completed root registry
before discarding that set. Later loads use the registry directly; explicit
result release permits reclamation. Transfer rejects reentry and retargeting;
ordinary native load failures propagate unchanged. Bootstrap's native owner is
retained by the host through construction/application lifetime and released at
teardown, without requiring a GC object or a forward Runtime callback.

`libbirnam_root_registry.dylib:birnam_native_module_v1` follows through Bootstrap.
The registry itself requires only Id. Initial Object arguments are retained before it publishes a Word pointing to
its complete [root owner descriptor](../../abi/fixtures/root-registry-owner-v1/README.md).
Native registration, retain/release and root enumeration work without Runtime or
GC, including their failure paths. Admission checks the native owner's thread;
there is no future Runtime callback. After Runtime is complete, it replaces that
working mutation policy with its storage-observation admission. Canonical root
queries still work during collection/observation. Existing root-provider ABI v1
remains intact.

The host's `begin_id` initializes Id and Bootstrap; `initialize_roots` constructs
the root registry and adopts Bootstrap's early set. `begin_floor` combines those
steps. `InitialFloor::finish` constructs the remaining Runtime with the same owners. Ordinary `load_floor` and
`start` use this path, while `start_with_floor` continues an explicitly constructed
prefix. Runtime's native entry in `libbirnam_runtime.dylib` receives nine
completed predecessor pointers as Word operands and invokes the temporary
[v15 constructor](../../abi/runtime-construction-v15.md). Earlier Runtime bridges
and normal `id_init` exports are removed. Explicit Runtime diagnostic builds
retain the old local initialization path for fixtures that have not yet migrated;
that path is absent from normal Id and Runtime libraries. The host's obsolete
fallback preparations for completed native predecessors are removed.

Retained root records transfer with the registry into Runtime and are included
in actual GC and diagnostic roots. Releasing a record removes its reference;
tokens are local to the registry and never reused. Released records are removed,
so native storage tracks live roots rather than cumulative registrations. The returned root view is
borrowed until registry mutation and must be copied before invoking callbacks.
The remaining Lifetime records now initialize in `libbirnam_lifetime.dylib`
through Id, before Runtime or any value/collector library is available. Its
predecessors are Id and the complete native factories returned by
`libbirnam_payload_layout`, `libbirnam_trace_hooks` and `libbirnam_finalizers`.
Those factories construct their public capabilities with Lifetime's own working
admission, membership and record operations. They do not point forward to Runtime.

[`LifetimeOwnedV2`](../../abi/fixtures/lifetime-v2/README.md) owns the exact payload
byte descriptions, trace-hook attachments and finalizer records. Runtime adopts
that owner through `birnam_runtime_init_v11`; its behavior/heap tables retain no
copies of these records. Collection, diagnostics, Image and recovery query the
same records through the native boundary. Public APIs retain their existing ABI
and code ownership. Runtime replaces Lifetime's initial Id-based admission/facts
policy with its completed observation/permanence policy. Canonical queries do not
invoke that policy or arbitrary lookup during collection.

`InitialFloor::initialize_lifetime` exposes the completed prefix; ordinary
`finish` invokes the same step. Weak layout/hook retirement is separate from
finalizer detachment, so a finalizer attached to a behavior still runs after its
behavior metadata retires. Native Lifetime destruction releases all description
bytes before destroying its capabilities and never invokes object finalizers.
Legacy `id_init` construction and unit harnesses retain local service records
until their consumers migrate; current production uses only the native owner.
Bootstrap now owns common-protocol initialization results. Routing the remaining
legacy constructors through it, completing ownership handoff as module exports
are published remain required. Collector activation now follows the completed
value/Trace/collection prefix as described below. This prefix does
not move Runtime/compiler code out of Id.

`InitialFloor::initialize_base_values` then loads the existing Boolean and Nil
libraries through Bootstrap. Boolean's native entry takes completed roots and
Lifetime, creates the actual Nil/Boolean identities through Id messages, and
installs identity/basic Boolean operations. Nil's native entry takes those
completed identities and installs the Object/Nil predicates. Their
[base-value descriptor](../../abi/fixtures/base-values-v1/README.md) contains only
already constructed objects. Basic methods use that completed native context;
they do not read the later String/Block/Runtime API. Printing and Block control
are absent until the later extension entries install them.

Runtime adopts these same objects and layouts, retaining the existing permanence
policy for the five canonical objects/behaviors without promoting unrelated early
allocations. Later Boolean/Nil entries preserve the basic method bindings. The
legacy `id_init` path still constructs local constants until its consumers move.
The remaining upper construction graph is still unimplemented.

`InitialFloor::initialize_namespace` then loads Namespace's native entry with
completed roots, Lifetime and base values. Its
[native owner](../../abi/fixtures/namespace-owner-v1/README.md) owns the actual
ordered binding table and a receiver exposing Symbol-based publication/import,
Word-valued count/index access and reverse aliases. Initial names refer only to
completed objects. String, Integer, Error and Block are neither loaded nor
required by this entry. Missing names, malformed operands and bounds have native
outcomes. Its registered root provider visits current binding names/values and
the receiver, copying references before marking. Rebinding does not retain old
values in a second root table.

Runtime adopts the same Namespace storage and adds its existing C-name adapters.
It preserves early published names and initializes the legacy weak dictionary
view from them. The native publication method gains completed Runtime observation
and mirror policy; subsequent native and upper publication still use one table.
The former Namespace preparation is skipped in production. Native destruction
clears the root provider's selection and releases its owned storage; the provider
then reports no roots. The remaining value/construction stages still
need migration, and the legacy weak dictionary mirror remains to be retired
with its consumers.

`InitialFloor::initialize_object_model` loads Object Model with completed roots,
Namespace and base values. Its
[native owner](../../abi/fixtures/object-model-owner-v1/README.md) contains the
actual private-behavior associations and complete Enumeration v2 interface.
Private behavior construction sends existing Id protocols, roots the new behavior
until attachment, and works before String, numbers, GC or Runtime. Native
malformed operands, non-behavior ownership targets and enumeration bounds have
complete early outcomes. Enumeration calls Id facts acquired through lookup;
it retains no Runtime view callback or copied dictionary registry.

Runtime shares that owner between behavior tracing and enumeration. Its old
per-behavior owner field and empty metadata records are removed. A live behavior
contributes its actual owner edge; dead-behavior retirement removes the native
association before Id detaches the allocation. The native private-construction
method survives adoption and later Object Model mounting. Canonical associations
are established early and are not overwritten during adoption. Object Model's
production enumeration-factory preparation is removed.

Runtime installs completed preparation, reassignment and owner/parent adapters
with explicit contexts. This avoids the legacy active-runtime requirement when
an early native method invokes them through Id after adoption. The owner/parent
query adapters temporarily preserve the old all-Oop null sentinel; converting
those remaining native consumers to the typed Unit contract is still required.
Legacy local owner records and enumeration factories remain only for unmigrated
`id_init` consumers and local harnesses. These changes complete the current
Namespace/Object Model construction prefix, not the remaining value and GC
stages or the Runtime/compiler package separation.

`InitialFloor::initialize_string` loads the existing String library through
Bootstrap with completed roots, Lifetime and Namespace. Object Model is already
installed. Its [native entry](../../abi/fixtures/string-v1/README.md) constructs
the real String behavior/prototype, installs its raw layout and finalizer-backed
byte ownership, and publishes the names. `fromBytes:` accepts native Bytes;
`bytes` returns borrowed Bytes; `length` returns a Word; `clone`/`new` copy storage.
No Integer, Error, Float, Block or Runtime dependency is present in these methods.

String sends current Id allocation messages. It validates the returned allocation
against selected inert Id facts before writing, rejects existing ownership or
finalizers, roots clone inputs across dispatch, and attaches cleanup through the
already initialized Lifetime records. No storage-map borrow spans a send.
The completed String table is adopted by the upper consumers; production skips
String's legacy preparation. Its separate text extension installs methods once
the number dependencies exist. Late String mounting preserves early names,
storage and native bindings. This completes step 5's byte-storage construction.
The aggregate Runtime remains in Id.

`InitialFloor::initialize_error` now loads the actual installed Error library
(`libbirnam_id_error.dylib`) with completed roots, Lifetime, base values,
Namespace and String. Its [native entry](../../abi/error-value-v3.md#production-error-construction)
returns the Error factory as an Object, publishes the existing three Error names
and registers its own roots. Bootstrap releases its temporary result root once
that publication is complete. No descriptor or new library is added.

The existing factory selectors, kind/message access, recognition, Error
propagation and Object missing-message construction are all usable before
numbers, Block or Runtime. Kind/message references are rooted across allocation;
payload writes validate the returned allocation and readers validate stored
references. Lifetime owns the existing two-Oop layout. Runtime preserves an
already installed Object DNU, and late Error mounting preserves the native
factory, layout and method/context identities, including early replacements.
The old stateless Error Value v3 adapter remains for legacy IdApi consumers; it
owns no representation and now sees a completed factory before numbers mount.
This completes step 6's native Error construction.

`InitialFloor::initialize_integer` constructs the actual Integer base through
Bootstrap with completed roots, Lifetime, base values, Namespace, String and the
Error factory. The [native entry](../../abi/integer-v1.md#production-construction)
returns the existing Integer operations table. It owns the same signed-64-bit
payload and codec, constructs canonical identities, installs the raw layout and
publishes the names. Integer construction, arithmetic, comparison, printing and
Error propagation work before Float, Word, Block or Runtime. Allocation sends
the current Id method and checks actual size/behavior before writing. Equality
and ordered comparison retain their existing semantics, including double-precision
rounding in ordered comparisons.

This completes step 7's Integer base. Later entries reuse its actual operations
table and identities; mixed-number methods replace only original base entries,
preserving early replacements. Float conversion and Block iteration are absent
until those dependencies exist.
The old numeric compiler declaration assumes direct allocation and still returns
zero for this message-based constructor. The [v5 native declaration](../../abi/native-numeric-semantics-v5.md)
now supplies the successful exact-Integer contract for actual native entries.
Compiler construction admission checks current lookup and allocation; arithmetic
also checks Namespace and Symbol lookup. Numeric predictions preserve complete
function/context pairs, and closure admission checks the current native
owner/parent entries. Replacements withdraw admission without running their
callbacks. These upper compiler checks must move with Execution/compiler during
the remaining package split.

String, Error and Integer root their cached selectors and predecessor references.
Removing a method binding must not collect a selector still held by a native
context; the same selector can be used to restore the method after collection.

`InitialFloor::initialize_float` constructs Float after Integer through Bootstrap.
Its [native entry](../../abi/float-v1.md#production-construction) takes the completed
predecessors and Integer table, owns the existing IEEE bits representation, and
publishes the canonical identities and seventeen existing methods. `fromBits:`
and `bits` use native Words and preserve the full payload, including signed zero
and NaN payloads. Construction sends the current Id allocator and checks actual
allocation size/behavior before writing. Conversion errors, arithmetic, printing
and live Error/ErrorVt lookup work without Word, Block or Runtime.

Once Float is complete, it sends Integer `installFloat:` with its table as a
native Word. This is an explicit extension entry on the existing Integer object.
It creates a separate complete context, installs mixed arithmetic/comparisons and
`asFloat`, and preserves original-base replacements. Copied Integer methods keep
their existing mixed receiver behavior. Integer's base has no future Float slot.
Late mounting preserves these entries and adds only the remaining Block iteration.
Production skips Float's legacy preparation and adopts the identical operations
table into upper consumers; legacy preparation now returns that table if called
on the same native Id. The legacy-only constructor remains for native clients.

`InitialFloor::initialize_word` constructs Word through Bootstrap with completed
roots, Lifetime, base values, Namespace, String, Error and Integer. It takes no
Float input. Its [native entry](../../abi/word-v1.md#production-construction)
constructs the existing unsigned representation, raw layout and canonical names.
`fromBits:`/`bits` use native Words; equality, hexadecimal printing and checked
Integer conversion work before Block/Runtime, including overflow errors through
current Error factory messages. Cached selectors and canonical identities are
rooted. Production skips legacy Word preparation; upper consumers adopt the same
table and late mounting preserves native methods, layouts and replacements.
Legacy preparation returns that table if called on the same native Id.

`InitialFloor::initialize_text` now sends String `installText:` with completed
base-value, Integer and Float descriptors, then sends nil and true
`installPrinting:` with completed roots and String. These explicit native
extensions install all fourteen existing String text/numeric methods and
Nil/Boolean printing before Block/Runtime. Each extension owns a separate
complete context; no future provider is stored in the base owners. Existing
bindings are preserved and late legacy mounting keeps the native methods.

String roots arguments and temporary printable results across callbacks and
uses the live numeric/String operation tables. Indexing remains byte-oriented;
invalid indices and numeric parses keep their existing nil results. Symbol
conversion preserves its first-NUL boundary. Native and legacy text paths share
trimming, replacement and bracket-scanning algorithms. Nil/Boolean printing sends
the current String construction message; cached selectors and predecessor
references remain rooted.

This completes the value/text entries in step 8, with the real sixteen-library
prefix exercising both ordinary results and advertised failure paths. No new
library, representation, descriptor version or Runtime bridge is added.
Execution/Block and the remaining upper/package separation
are still open. Native Integer/compiler admission is described above.

The live graph portion of step 9 now constructs before Runtime as well.
`InitialFloor::initialize_trace` loads Native Mark through Id, then constructs
Trace with completed root-registry, Lifetime, Object Model and Native Mark
inputs. Its [64-byte descriptor](../../abi/trace-graph-v2.md) owns root discovery,
Id/owner edges, payload scanning and inherited-hook selection without a Runtime,
Machine or compiler input. Production Runtime borrows that same completed graph;
its mark admission, Execution roots and retirement policy remain upper inputs.
Native roots and hooks now receive Trace's context-free mark callback, backed by
Native Mark's owned scope; the callback no longer points into Id. Normal startup
skips the old Native Mark factory preparation in Id; legacy initialization
retains that path.
The eighteen-library prefix exercises this construction with the remaining
libraries unavailable. Validation examines the actual early allocation graph and
rejects missing payload descriptions without marking or reclaiming anything.
Native Stack now supplies register capture through its complete v4 factory.
Graph construction alone does not activate collection. The subsequent explicit
activation validates layout or native-hook coverage, establishes the lasting
native boundary and installs collecting constructors. Native modules remain
responsible for declaring their owned references; the graph validator cannot
infer references hidden in arbitrary native storage.

`ObjectMemory` owns allocations and identities. Collect now constructs the
[collection-state owner](../../abi/collection-state-v2.md) before Runtime, from
complete Native Stack, GC Settings and Allocation Budget factories, Trace graph,
Lifetime and Object Model plus its own phase implementation. Its native entry enumerates the existing Id allocations
and creates the actual weak mark/permanence records and initial budget. Runtime
adopts the same records, phase, anchor and settings; it preserves early state and
adds only allocations made since their initial snapshot. Public mutations start
with working thread admission, then receive Runtime's complete observation policy.
The four former production preparations inside Id are skipped. The completed
coordinator is adopted with this owner instead of awaiting a later mount. Marking and dead
record removal now run in Collect, including deferred allocations when realized.
The legacy local mark map remains only for legacy initialization/unit consumers.
The [collector capability](../../abi/collector-v3.md) also owns live tracing,
Lifetime/Object Model retirement, Id index retirement, finalizer detachment and
allocation release. Once Execution exists, Runtime installs a completed extension
that supplies scoped upper roots and deferred/code retirement on every call,
including calls through previously saved collector entries. Without Execution
there are no such records.
The state owner advertises no operation needing a future callback. Explicit
activation and GC-aware constructor installation now run through Collector v3
before Runtime construction. Execution and the remaining package split are open.

`Heap` retains the deferred-allocation journals and storage-consumer adapter.
Allocation records use three native words. The collection owner's weak records
hold the exact payload pointer and mark/permanence flags; they do not root the
allocation or contain finalizer policy. Lifetime owns the separate finalizer
entries. This metadata overhead is outside payload-byte accounting.
`Vtables` and `Symbols` require only Id allocation and their own
indexes. `Behaviors` accesses Object Model owner records and Lifetime descriptions. The obsolete allocation-identity, dictionary-index and
mutation-counter factories and their host preparations are removed.

The current behavior payload is five words (40 bytes): capacity, tally, keys,
values and parent. Each dictionary value is a typed 24-byte Operand. Image
runtime ABI is **7 (`f57d502f7c1c0437`)**, image format is 8, and upper IdApi
remains version 20. [Dispatch Cache ABI v2](../../abi/dispatch-cache-v2.md) stores
complete 16-byte native entries in 40-byte slots; its descriptor is 56 bytes.
These versions reject older incompatible native representations.

Runtime installs completed GC-accounted construction/interning, managed native
loading and observed mutation as ordinary method replacements. The same native
carrier works before and after adoption. Id provides allocation/behavior
construction, its prototype, allocation extent/identity, behavior and Symbol
bytes before language value modules exist. Native caller roots and upper
payload descriptions remain explicit obligations.

Collect selects its exact allocation enumeration, index retirement and detachment
entries through Id lookup during construction. The completed native Trace graph
selects its own Id fact entries and supplies primordial references, marked-object
edges, behavior owners, payload layouts/hooks and native roots. Runtime no longer
selects redundant Id storage callables on this production path. The mark and
collection-source implementations now live in Collect. Runtime supplies its
remaining upper roots, compatibility roots, deferred materialization and
executable-owner retirement callbacks. No Id registry borrow crosses a visitor.
The diagnostic storage view,
legacy Symbol compatibility mode, adoption and Execution's dead-binding accounting
still access internal Rust storage. Production index retirement, allocation
detachment and release now use the native storage callables. Upper behavior
metadata and per-allocation finalizer records retire separately; the coordinator
keeps detached payloads readable until their finalizers return. Lifetime's root and representation owners now initialize before Runtime;
the full Id allocation/registry adoption cutover
remain required.

Ordinary binding, inherited caching and Machine synchronous targets carry the
complete native function/context pair. Context-only replacement is observable.
Effectful lookup can replace a binding while the current send invokes its exact
selected entry once; the next lookup sees the replacement. DNU performs a fresh
handler lookup without repeating the original lookup. Generated adapters enter
their exact registered implementation rather than resending the selector.

Execution owns managed outer turns across sends, Machine work and nested native
callbacks. The pinned outer return roots its result before draining retirements;
a bounded Machine return under a native caller does not drain. Finalizers of TCC
Units and mapped owners transfer native resources into detached queue records
before their Oop payloads are freed. Candidate and CodeBinding disposal use the
same checkpoint mechanism. Alien's callback registry retains its Block and Unit
before publishing the callback address. These ownership implementations still
reside in Runtime inside `birnam-id` and must move together to upper Execution.

At the upper language boundary, native Object results remain objects, Unit
becomes Nil, invalid arguments become `nativeInvalidRequest`, and other failures
become `nativeFailure`. Successful native scalars/bytes/entries/protocols require
a language wrapper and currently produce `nativeResultRequiresWrapper`. Error
construction is an ordinary upper send; bare Id has no Error dependency.
`legacy_method.rs` still adapts existing all-Oop consumers, preserving their raw
argument-buffer convention. Legacy reflective lookup/removal cannot represent
arbitrary native function/context pairs and remain a required migration.

Current debug evidence:

- The Id suite passes **308 tests**, with 15 existing ignored tests. It covers
  early object/behavior creation, adoption, identity/field preservation through
  first GC, post-adoption accounting and reclamation after dropping roots. The
  collector acquisition test installs an Id root callable before construction,
  replaces the method afterward, and proves the existing collector retains the
  selected callable while a new consumer observes the replacement.
- A fresh debug install publishes **55 native libraries and 28 authored modules**.
  `scripts/id-native-loading-smoke.py PREFIX` loads that actual installed Id with
  upper libraries absent, constructs objects and installs methods from independent
  C, and then starts the normal host using the same carrier and graph. The C
  visitor reenters Id, allocates during allocation enumeration, verifies snapshot
  membership, and checks complete failure propagation. It also retires independent
  behavior/Symbol indexes and detaches/releases allocations before Runtime exists,
  verifies immediate membership removal and live detached bytes, and rejects
  ordinary Object arguments to the native destruction operations. After upper adoption it
  supplies completed roots/collector explicitly and verifies actual collection
  preserves the rooted object's identity and payload. Ordinary typed sends and
  native-to-language Error outcomes also pass. With only Id, Bootstrap and
  root-registry libraries available, a second C module retains a new object
  before Runtime
  exists. The same retained record survives adoption and precise collection;
  release then permits reclamation. Repeated retain/release removes records
  without reusing tokens. Invalid initializer operands fail before publication.
  With only Id, Bootstrap, roots, the three primitive description libraries and
  Lifetime available, another independent C initializer declares payload layouts, a native
  trace edge and three finalizers, including one on a behavior. The same records
  survive normal adoption and precise collection; releasing the root permits
  reclamation, runs every finalizer once and removes the weak descriptions.
- The Bootstrap prefix runs with only Id and Bootstrap available. Independent C
  makes a nested native load return an otherwise unrooted Object, exercises
  Protocol retention using the real Id, and propagates a missing-library failure.
  The object survives root transfer, Runtime adoption and precise GC; explicit
  result release permits reclamation. The same check passes after Runtime exists.
  Bootstrap's installed exports match its manifest and its only native link is
  the platform library. The ABI/host suites pass 24/86 debug tests. Normal host
  startup, compiled modules, images and baked execution pass the root-registry
  workload with this loader in the production path.
- With only the nine Id/Bootstrap/Lifetime/base libraries available, the
  independent C base-value fixture exercises Nil/Boolean operations and native
  failures while printing and Block control are absent. It replaces a Nil
  predicate, reloads both completed base entries, then verifies the same values,
  layouts and exact basic function/context pairs after normal startup and
  precise GC. The Boolean and Nil image workloads pass source, compiled, native
  image and baked execution, including live method replacement. The debug
  ABI/host/Id suites pass 24/86/308 tests (plus four host binary tests).
  Installed Boolean/Nil exports match their manifests and link only the platform
  library; Id exports only the current v5 Runtime bridge.
- With only ten predecessor libraries available, Namespace's independent C
  consumer verifies publication/import, ordered native enumeration, first reverse
  alias, malformed operands and bounds with String/Integer/Block/Error absent.
  The actual table and aliases survive Runtime adoption and precise GC. Removing
  one alias preserves the object; removing the last permits reclamation, and
  later C-name imports observe the cleared binding. Both Namespace and Namespace
  Enumeration workloads pass source, compiled, native image and baked execution.
  Debug units pass for ABI (24), host (86 plus four binary tests), Id (308) and
  Namespace (20). The installed Namespace exports match its manifest and it
  links only the platform library. The v4 Runtime bridge replaces v3; production
  no longer prepares Namespace's factory inside Id.
- The eleven-library prefix includes Object Model's actual installed native
  entry. Independent C constructs a private behavior before successor modules,
  exercises native enumeration and argument/target failures, and roots only that
  behavior. The same owner edge retains the object after Runtime adoption and
  precise collection. Releasing the root permits both allocations to be reclaimed
  and removes the weak record. The same construction/release check passes after
  Runtime exists, with the original native ensure entry still selected. The
  fixture uses an inherited probe method so Store backfill does not add a second
  root to the private behavior under test.
- The twelve-library prefix constructs String bytes with Error/Integer/Float/
  Block absent. The independent C fixture verifies binary and empty bytes, copy
  ownership despite raw payload mutation, malformed operands, live allocation
  replacement (including nested sends), rejection of small/already owned results,
  and unchanged clone bindings. Early Strings survive upper adoption and GC;
  releasing their roots removes both their allocations and byte/finalizer records.
  The fixture repeats construction and reclamation after Runtime adoption and
  confirms the upper consumer has adopted the identical String table/identities.
  String's source/compiled/saved-image/baked-image workload passes. The legacy C
  method suite passes 410 byte/index/slice cases and 18 numeric boundaries;
  String storage units (2), Id units (308) and host units (86) pass. The installed
  String library exports exactly its seven manifest symbols and links only the
  platform library. The existing String ABI and private Runtime v5 bridge remain
  unchanged; no new factory preparation or production library was introduced.
- The thirteen-library prefix constructs Error before Integer/Float/Block.
  Independent C verifies the three existing factory forms, predicates, kind and
  message access, propagation, missing-message construction, malformed operands,
  and reader/allocation safety. An Error retained by one root keeps its kind and
  String message alive after Runtime adoption. Releasing that root reclaims all
  three and releases the String buffer. Changed payload-layout records, native
  factory replacements and Object DNU entries survive adoption and late Error
  mounting. The same fixture repeats construction/reclamation with Runtime
  active. Error's source, compiled, saved-image and baked-image workloads pass,
  including fresh factory replacement and exact recognition. Id's 308 and the
  host's 86 debug units pass; the installed Error exports match its five-symbol manifest and
  link only the platform library. The private Runtime v5 bridge is unchanged.
- The fourteen-library prefix constructs Integer with Float/Word/Block absent.
  Independent C checks 605 signed arithmetic cases and 605 comparisons, boundary
  printing, Error propagation, malformed operands and replacement allocation
  bounds. The fixture repeats after Runtime adoption with precise GC, preserving
  canonical storage, early method replacements and unchanged base entries.
  Retained Integers survive collection and released ones are reclaimed; cached
  selectors survive method removal and collection. Integer's source, compiled,
  saved-image and baked-image workload passes. Integer (1), Id (308) and host (86)
  debug units pass. The installed Integer exports match its nine-symbol manifest
  and link only the platform library. The private Runtime v5 bridge is unchanged.
- The fifteen-library prefix constructs Float and installs Integer's mixed-number
  extension with Word/Block/Runtime absent. Independent C checks 1,164 arithmetic
  cases, IEEE comparisons, signed zero/NaN payloads, conversion limits, printing,
  live ErrorVt replacement, copied Integer methods and allocation bounds. The
  fixture repeats after adoption with precise GC; native method/context
  replacements and retained payload bits survive, and released objects are
  reclaimed. Float (1), Integer (1), host (86) and Id (308) debug units pass.
  The legacy Float client passes 676 arithmetic cases plus conversion, publication,
  replacement and GC checks. Installed Float has its eight manifest exports and
  links only the platform library. Float's source, source-free compiled, saved-image
  and baked-image workloads pass, including live arithmetic replacement and Error
  factory rebinding. Integer's error-value image workload also passes with the
  native mixed extension installed. The Runtime v5 bridge is unchanged.
- The sixteen-library prefix constructs Word before Block/Runtime. Independent C
  checks 262 unsigned payloads against hexadecimal formatting and conversion
  bounds, exact equality, fresh identity, malformed operands and allocation size.
  The fixture repeats with precise GC after adoption: method/context replacements
  and retained bits survive, and released Words are reclaimed. Word (1), host (86)
  and ABI (24) debug units pass. The legacy Word client passes 81 equality cases
  plus representation, conversion, replacement and collection checks. Both Word
  image workloads pass source, source-free compiled, saved-image and baked runs,
  including overflow and live Error factory replacement. Installed Word matches
  its eight-symbol manifest and links only the platform library.
- The completed sixteen-library value/text prefix runs String's fourteen methods
  and Nil/Boolean printing before Block/Runtime. Independent C covers all 256
  byte indices, binary slices/equality, trimming, replacement, bracket scanning,
  first-NUL symbol conversion, numeric parsing boundaries and malformed native
  operands. It repeats after adoption with precise GC, including collection
  during printing and subsequent byte callbacks. Early method/context replacements
  survive; releasing the retained String reclaims its identity and byte buffer.
  The legacy String client passes 410 binary/index/slice cases and 18 numeric
  boundaries; host debug units pass (86). String, Nil and Boolean source,
  source-free compiled, saved-image and baked workloads pass with native text
  and printing. Their existing 7/4/4-symbol manifests match the installed dylibs,
  which link only the platform library. No Runtime bridge version changes.
- The native Trace graph is constructed in the production eighteen-library
  prefix before Runtime. An independent C client verifies exact Id fact selection
  survives method replacement, conservative/precise scanning, core-only queries,
  inherited and replaced hooks, Object Model owner edges, sink failures and live
  layout replacement. A separate clean-prefix process validates all initial
  representations, rejects a newly missing description, then succeeds after its
  layout is installed. The same graph works after Runtime adoption and actual
  precise GC. Debug ABI/host/Id/Native Mark/Trace units pass (429 total, 16 ignored).
  Trace and Native Mark expose exactly 6 and 4 symbols and depend only on the
  platform library; installed Id matches its bare manifest plus the two existing
  module wrappers, with only the v6 Runtime bridge. Payload Layout and Trace Hooks
  pass source, source-free compiled, saved-image and baked execution. Native Mark
  also passes mapped/rebuilt images with the legacy preparation skipped; its C
  fixture verifies the callback now belongs to Trace and retains the same identity
  across providers/hooks, null/duplicate marks and nested-collection rejection.
- The twenty-two-library prefix constructs Collect's mark records, budget, phase,
  stack anchor and settings before Runtime. Independent C checks exact initial
  inventory/accounting, mark and permanence rules, control operations, rejected
  coordinator input, preservation through adoption, and accounting for subsequent
  allocations. Actual GC removes records before finalizers, preserves an early
  retained root and reclaims it after release. ABI, host, Id, Collect, Native Stack,
  GC Settings and Allocation Budget units pass (437 total, 16 ignored). The four
  control image workloads pass source, source-free compiled, saved and baked
  execution; Native Stack, Budget and Phase also exercise mapped/rebuilt images.
  The final native harness and Phase image run verify immediate coordinator
  adoption. Installed Collect exports exactly six symbols; each of the three
  primitive owners exports four, matching its manifest. All link only the platform
  library. Installed Id matches its manifest plus the two module wrappers and
  replaces the intermediate bridge with `birnam_runtime_init_v7`. This verifies
  state adoption; the subsequent activation implementation is described below.
- Native Stack v4 retains Darwin arm64 register capture and interval validation,
  and adds the thread-lifetime boundary used by activation.
  The independent C scanner fixture passes at default optimization and `-O2`,
  observing an x19 value in the real save area, callback frame ordering, rejected
  intervals and nested capture. The native loading harness invokes that operation
  before Runtime; later collection uses the same factory. ABI/host/Id/Native Stack
  units pass (425 total, 16 ignored), including source-probe root coverage. The
  legacy native client preserves automatic collection, native/provider roots,
  callback/finalizer anchor resets, nested GC and owner/observation admission.
  Native Stack's source, source-free compiled, mapped/rebuilt image and baked
  workloads pass. The installed-host anchor-selection test also passes; its
  harness uses consistent unwind flags to avoid Cargo's abort/unwind artifact
  collision. Installed Native Stack and Id match their 4/68 export manifests;
  the GC-specific Darwin `_setjmp` dependency now belongs to Native Stack. Id's
  separate compiler-linked `setjmp` dependency remains with the pending compiler
  package extraction. The subsequent explicit activation transition is described below.
- The same twenty-two-library prefix now constructs the collector itself from
  completed native owners. Independent C verifies root-only survival, later
  release, retirement of unreferenced Symbols/behaviors, removal of mark/finalizer
  metadata before finalization, nested collection rejection, and accounting reset.
  A separate process verifies a rejected root callback leaves all allocations and
  finalizers intact and the result count untouched. The full native loading and
  adoption harness passes with the same collector. ABI/Collect/host/Id units pass
  (423 total, 16 ignored). Collection and Trace Hooks pass source, source-free
  compiled, saved-image and baked execution. Finalizers also pass mapped/rebuilt
  images, replacement, detachment and native cleanup. The portable CodeBinding lifecycle
  workload preserves publication, replacement, self-removal and retirement.
  Collect depends only on `birnam-abi`, exports its existing six declared symbols,
  and links only the platform library. Installed Id matches its 68 exports with
  the new temporary v8 bridge. These checks establish native collection ownership;
  they predate the explicit activation/construction transition described below.
- Serial debug unit suites pass for Object Model (4), Id (308), host (86) and ABI
  (24). The combined multi-package command encountered Cargo's existing
  abort/unwind artifact filename collision; separate package runs passed.
  The final installed Enumeration and Allocation Identity workloads pass source,
  compiled, native image and baked execution, including independent host proof
  of mapped/rebuilt images, parent/behavior replacement, method lookup, active
  Frame access, identity and finalization. Object Model's installed exports match
  its manifest and its only dynamic dependency is the platform library.
- The root-registry image workload passes for source, source-free compiled,
  native Code images and baked execution, including duplicate provider order,
  callback-time append, nested collection, roots, aliases and reload. Debug unit
  checks pass for ABI (24), host (86), Id (308) and root registry (5). The installed
  root-registry exports match its manifest; Id has the declared exports plus the
  installer's existing `module_init` and `birnam_symbol_borrow_v1` wrappers.
  The Lifetime change additionally passes 431 debug unit tests across ABI, host,
  Id, payload layout, trace hooks and finalizers. Installed Lifetime and all three
  primitive description libraries match their export manifests. Lifetime's only
  Cargo dependency is `birnam-abi`; its dylib links only the platform library.
- The installed payload-layout, trace-hooks and finalizers image workloads pass
  with native Lifetime records across source, source-free compiled, native Code
  image and baked execution. They preserve borrowed-prefix replacement, arbitrary
  layout kinds and image rejection/restoration, hook-marked children, finalizer
  replacement/detachment, native cleanup, roots, aliases and reload.
- `scripts/allocation-identity-image-smoke.sh PREFIX` exercises source, source-free
  compiled, mapped/rebuilt native images and baked execution, with identity,
  dictionary replacement, finalization, active Frames, aliases, GC and reload.
  Existing dispatch and lifecycle coverage additionally exercises exact adapter
  entry, context-only replacement, nested callbacks, outward-result rooting,
  detached-owner release, portable resumable eviction and CodeBinding self-removal.
  The installed `portable-resumable-lifecycle-smoke.sh` and
  `portable-code-binding-lifecycle-smoke.sh` checks also pass with native Id
  detachment/release, including eviction survival, replacement and self-removal.

The deferred-identity fixture exposed a legacy custom-lookup failure that also
reproduced on `e8045b44`: converting a selected native allocator to a legacy
Method address and back discarded its context and typed Word argument contract.
The upper legacy adapter now retains the original native entry for the duration
of that lookup. Nested lookups have separate temporary records; later binding
mutation does not change the entry already selected. Different native contexts
represented by the same legacy address in one lookup are rejected as ambiguous.
This supports existing callable legacy adapters only; arbitrary native entries
still require migration of reflective consumers, not invented Method addresses.

The unchanged `scripts/deferred-identity-image-smoke.sh PREFIX` now passes in
full: source, compiled module, saved mapped/rebuilt image and baked execution,
including the actual host's image-mode checks and forced recovery. Its independent
Storage observer records the same closure/numeric deferred writes in all three
image forms: `(386, 150)` normally and `(386, 75)` with forced recovery. The
existing assertions for identity, finalization, Frame reads/writes, custom lookup,
dictionary mutation, aliases, GC and reload remain intact.

Native compiler admission uses the current allocator and lookup, full native
function/context pairs, and the declared Namespace read path. Closure admission
also recognizes the native owner/parent protocol entries. The independent
`scripts/native-integer-admission-smoke.sh PREFIX` compares counted effects with
optimization disabled/enabled for eight replacements: allocation, Namespace
binding lookup, interning, Symbol validation, lookup, owner/parent queries and
arithmetic. All counts match; restoring every method restores closure admission.
The production native-loading harness also passes with C checks for the new
declarations before and after Runtime adoption. Debug export inventories match
the Integer and Namespace manifests (11 and 5 symbols respectively).

The legacy adapter tests exercise the actual native allocator with a Word size
after nested lookup and binding replacement, and distinct live Runtime contexts
sharing one legacy address. The current ABI/Id/Integer/Namespace unit suites pass
(356 passed, 16 ignored), including native relation-context rejection. Legacy
Integer error/value and numeric-context fixtures pass their 800 oracle cases and
live callback/restoration checks. These tests do not close arbitrary native
reflection migration or the package split.

Remaining architectural acceptance requires all of the following:

1. Move Runtime, compiler/artifact dependencies and upper ownership out of Id;
   remove temporary forwarding exports and migrate native/reflective consumers.
2. Route the remaining legacy constructors through Bootstrap and complete their
   result-ownership handoff when module exports become available. Common native
   initialization results are now retained and transferred to the initialized
   root owner. Migrate remaining legacy local-service paths with their consumers.
3. Finish storage-consumer migration in adoption, diagnostics, compatibility and
   Execution accounting. Collection now uses Id's native retirement/release path;
   the remaining Rust allocation/registry handoff must not become a permanent
   coupling between separately packaged modules.
4. Replace the aggregate preparations/initialization with real base and extension
   entries. Publish the entry-level predecessor manifest, map every SCRIPT_MODULES
   entry or removal, reject cycles/missing predecessors, and update the diagram.
5. Prove real construction prefixes, source/compiled/image/worker behavior and
   the source/Cargo/private-export independence audits specified above. Passing
   the current tests does not close these unimplemented requirements.

### Mandatory completed collector extension

The public collector now has an explicit completed-owner extension
([Collector v2](../../abi/collector-v2.md)). The base remains independently usable
before Runtime. Once Runtime and its Execution objects exist, Runtime installs
its full roots and retirement operations. Saved public descriptors select that
extension on every call; they cannot accidentally keep collecting the earlier
heap without Execution's obligations. The scoped base passed into the extension
performs the original native collection sequence without redispatch.

Normal Runtime collection now passes only the captured native interval, while
trace probes pass only their additional roots. Runtime's extension owns the full
upper retirement sequence and rejects a competing per-call retirement table.
The outer collection owner keeps its 128-byte layout and requires collector v2;
the temporary upper constructor becomes `birnam_runtime_init_v11`.

This extension closed the saved-entry obligation needed for section 5's
construction wrappers. Activation was separate work; its production
implementation follows below. The module/package split remains open.

Validation passes for the ABI/Id/Collect unit suites (341 passed, 15 ignored),
the installed native-loading harness and the collection, finalizer and
deferred-identity image scripts. The saved-entry test traces an otherwise
unbound Symbol with no caller roots and proves Runtime's compatibility root
survives; the same early descriptor later collects with Runtime retirement.
The independent C extension test checks added roots, release, all retirement
callbacks, repeat installation and rejection of replacement/nested collection.
The image tests cover source, compiled, mapped, rebuilt and baked execution.
Actual deferred writes agree across image forms: `(386, 150)` normally and
`(386, 75)` under forced recovery. Installed Id/Collect exports match their
manifests, including Id's then-linked artifact exports. No release validation was run.

### Production collector activation and construction

[`Collector v3`](../../abi/collector-v3.md) now activates the production heap
before Runtime/Execution. `InitialFloor::finish` first calls
`activate_collection`: Bootstrap v2 pauses publication, the live Trace graph
validates Id edges and payload layout/native-hook coverage, Collect reconciles
records and installs Native Stack v4's lasting thread boundary, then replaces
construction methods through Id messages. It publishes Active only after all
steps succeed. A failure retains the existing owners and leaves collection
disabled without marking, finalization or reclamation. Bootstrap's early roots
have already been transferred into ordinary Root Registry records; activation
neither copies the heap nor permanently marks every bootstrap allocation.

`allocate`, `delegated`, `clone` and `new` collect before construction, with their
input explicitly rooted. This prevents a native hook from seeing an uninitialized
new instance. `intern:` accounts a fully initialized Symbol and roots it if a
collection is due; re-interning an existing Symbol never causes collection.
The Id primitives remain non-collecting. New allocations update Collect's own
weak records and budget. Saved early wrappers invoke the public collector, so
they gain the complete later Runtime roots and retirement extension.

Aggregate Runtime construction temporarily retains its actual canonical objects
and compatibility Symbols while it creates Execution objects, then releases those
roots after installing its complete collector extension. This transitional
constructor still installs Runtime's existing upper methods and deferred-accounting
adapter. Those duplicates and the remaining Rust Symbol/dictionary handoff must
disappear with the native Execution/package migration; activation does not claim that cut
is complete. The matching temporary upper entry is `birnam_runtime_init_v11`.

The C activation fixture forces automatic collection through all four construction
paths with only twenty-two predecessor libraries present. It checks result/input
liveness, existing-Symbol behavior, initialized payloads at trace hooks, publication
pause and failure without sweep for an undescribed payload. It also forces GC
during native Integer construction and overflow-error creation. Separate tests reuse
saved constructors after Runtime adoption and force collection while Runtime
itself is being built. The older Id-only storage probes now explicitly destroy
their temporary raw payloads before entering validated startup. Native-hook-only
payloads remain valid coverage; the missing-description negative test still fails.

Validation: the final ABI, Bootstrap, Collect, Id, Native Stack and Trace unit
run passes 356 tests with 15 ignored. The installed native-loading harness and
collection, finalizer and deferred-identity image workloads pass. Deferred writes
remain `(386, 150)` normally and `(386, 75)` under forced recovery in mapped,
rebuilt and baked images. The independent ARM64 stack capture/scanner checks
pass, and installed Id, Collect, Bootstrap and Native Stack exports match their
manifests (including Id's then-linked artifact exports).

The worker-service smoke passes its Cell image round trips but fails the
unchanged ten-second ready deadline, including an isolated retry. A child sample
shows normal authored-module startup. The existing independent worker wire test
then passes source-free image boot, twelve exact replies and orderly shutdown;
its observed ready time was 53.87 seconds while the unit suite also ran, not a
performance baseline. An earlier debug implementation records the same deadline
failure in [Design 81BE](81be-production-namespace-import.md). This establishes
working worker image recovery and transfer, not acceptance of the nineteen-case
worker-service suite. That suite and the full architectural acceptance above
remain open. No production timeout was changed or release validation run.

### Diagnostic storage inventory through completed Trace

[Trace graph v2](../../abi/trace-graph-v2.md) now produces the allocation,
representation and core-edge portion of the production diagnostic storage view.
It uses the same selected Id operations as live tracing, with completed Lifetime
and Object Model records. It reports Id roots and Symbols without exposing Id's
registries. Inspection reads no ordinary payload words and invokes no root
provider, trace hook or finalizer. Its callback receives copied scratch arrays;
no Id or representation-record borrow survives into the callback.

Runtime obtains that snapshot outside its state borrow, then adds Collect's
permanence flags, its current upper roots, optional compatibility Symbol roots,
provider declarations and deferred-journal counts/bytes. Permanent-object roots
are derived from the reported core edges. It no longer re-enumerates Id storage
just to subtract physical bytes from total accounting. Observation admission,
ordinary lookup of `withStorageView:`, epochs, coverage gaps and failure cleanup
retain their existing contracts. The local/legacy constructor still has a
fallback inventory; it is not a second production storage authority.

This removes direct allocation/header/classification/dictionary reads from the
production diagnostic inventory. Runtime's allocation construction, Symbol
compatibility management, behavior mutation and aggregate ownership handoff are
still coupled to Id's Rust types and remain part of the package separation.

Validation passes 350 ABI/Trace/Collect/Id unit tests (15 ignored). The shared
root-building helper preserves the existing collector root order. The
installed native-loading harness passes the eighteen-library graph prefix,
activated construction and post-adoption cases. Independent inspection checks
cover sorted identities, extents, core flags, owner edges, inherited hook and
finalizer declarations, no provider/hook/finalizer invocation, visitor failure
propagation and replacement of Id fact methods after graph construction.

The protected storage-observation script passes source, source-free compiled,
mapped, rebuilt and baked execution. Final-prefix host checks also pass compiled,
mapped, rebuilt and baked observations with epochs, nested rejection, failure
cleanup, aliases and post-GC allocation identity. Id, Trace and Collect exports
match their manifests. No release validation was run. This closes the production
diagnostic inventory migration, not the remaining Runtime/package separation.


### Production allocation ownership stays in Id

Id now retains its allocation registry from the moment the initial graph is
created. The aggregate Runtime constructor no longer takes that registry away.
Its Heap calls completed native Id entries for allocation, interning, membership,
size/identity, enumeration, behavior association and detached release. Temporary
views copy coordinates and facts; they neither own an allocation nor borrow its
private record. Id is destroyed after upper Runtime state at harness teardown.
The local allocation backend remains only for legacy isolated harnesses.

Bootstrap selects the working, non-collecting `allocate` and `intern:` entries
before activation replaces their bindings. The temporary upper constructor is
`birnam_runtime_init_v11`, taking the completed
[primitive allocation descriptor](../../abi/primitive-allocation-v1.md) alongside
its existing predecessors. This is a concrete bootstrap seam: deferred identity
materialization may occur during collection and must not recursively collect or
charge its already-accounted journal entries again. Ordinary upper construction
still performs its existing admission, rooting, collection and accounting before
using these primitives. No future callback is installed in Id.

`allocationAt:` supplies the original pointer for an exact live payload address;
`allocationCount` supplies the current registry length without a copied index or
full enumeration. New objects are allocated with their final behavior, so their
construction does not spuriously advance the method mutation epoch. Explicit
reassociation advances that epoch once, in Id.

This removes production Heap's allocation-registry ownership and direct access.
It does not finish Symbol/dictionary ownership, their private upper interfaces,
native Execution construction or the final Id package separation.


Validation: 337 ABI/Id unit tests pass, with 15 intentionally ignored. A fresh
debug installation passes the independent native-loading harness, including exact
allocation address/count queries, detached storage, non-collecting primitive
retention across activation/adoption and collecting upper constructors. Installed
Id exports match all 68 expected symbols, including its two statically linked artifact exports.
Collection-value, finalizer and protected storage-observation workloads pass
source, source-free compiled, image and baked execution. The deferred-identity
suite also passes mapped/rebuilt/baked recovery with actual write counts
`(386, 150)` normally and `(386, 75)` under forced recovery in every mode.
No release validation was run. These checks establish the allocation migration;
the broader construction and package-separation acceptance remains open.


### Removing upper access to Id registries

Production now keeps Symbol names and ordered method dictionaries in Id
alongside allocations. Runtime retains native operations, not Rust registry
owners or pointers. Existing `symbolBytesOf:`, `behaviorCount`, `behaviorAt:`,
`parentOf:`, `parentOf:put:`, `referencesOf:do:` and retirement operations remain
the sources of those facts. The following native-only operations cover actual
remaining Runtime consumers; they use the existing typed operand carrier.

| Selector | Arguments to Id | Result and purpose |
|---|---|---|
| `symbolNamed:` | Bytes name | Existing Symbol or Unit, without interning; optimizer admission and compatibility checks |
| `symbolCount` | None | Word live Symbol count |
| `symbolAt:` | Word ordered index | Symbol or Unit |
| `symbolNameBytes` | None | Word bytes owned by names, including NUL terminators; compatibility limits |
| `behaviorEpoch` | None | Saturating Word dictionary mutation epoch; existing dispatch/optimizer guards |
| `isBehavior:` | Object candidate | Word zero/one exact membership |
| `behaviorView` | None | Borrowed Bytes encoding the ordered Object array |
| `selectorsIn:` | Object behavior | Borrowed Bytes encoding its ordered key array |
| `bindingOf:in:` | Object key, Object behavior | Direct typed value or Unit if absent; an opaque Object value still hides the parent binding |
| `lookup:in:` | Object selector, Object behavior | Canonical inherited NativeEntry or Unit; no application lookup |
| `bindingsIn:do:` | Object behavior, NativeEntry visitor | Visits copied key/value/weak-flag triples in dictionary order; used for executable retirement |
| `addMethod:to:as:` | Object key, Word behavior address, NativeEntry | Previous entry or Unit; primitive mutation below Store and Execution policy |
| `removeMethod:from:` | Object key, Word behavior address | Removed entry or Unit |
| `addWeakBinding:to:as:` | Object key, Word behavior address, Object opaque value | Unit; existing compatibility dictionaries |
| `newBehaviorWithParent:` | Word parent address, zero for none | New registered behavior, without collection; upper constructors account it |
| `bind:to:` | Object selector, Object receiver | Exact selected NativeEntry or Unit, or unchanged native failure; performs effectful metacircular lookup once |

Policy-free registry mutations require native Word addresses. Ordinary all-Object
language sends are rejected before mutation or allocation; they continue to use
the existing upper methods with collection and executable-lifetime bookkeeping.
Addresses resolve through Id membership to their original pointers.

Borrowed Object arrays are public native views, not Rust dictionary records.
They remain valid until the corresponding registry changes; callers copy them
before callbacks. Binding visitors receive copied entries with no registry
borrow held across invocation. Id neither owns executable generations nor changes
turn policy. Runtime's existing notification, rooting and lifetime bookkeeping
still surrounds primitive dictionary mutation. It snapshots dead selected entries
before Id retires their dictionaries.

`bind:to:` establishes Id's own dispatch context. Runtime no longer establishes
that context with a raw dictionary pointer. Legacy lookup/add/remove adapters
use the completed primitive registry operations instead of relying on an ambient
Id scope. The isolated local unit backend can retain its local scope until its
legacy harness is removed. Application sends still use fresh effectful lookup;
canonical dictionary queries are used only where the existing guards or trusted
bootstrap/collector contracts already permit them.


The initial Id graph contains no upper registry clients. Runtime constructs those
clients from the completed protocol, and retains Id until its own state is gone.
Only isolated local harnesses own local registry backends. Production executable
retirement enumerates copied bindings before Id removes their dictionaries;
Store notification and managed-turn ownership remain upper responsibilities.

This removes production Runtime's Symbol/dictionary ownership, direct registry
access and raw-pointer entry into Id dispatch. The legacy all-Oop Method adapter
still recognizes core primitive function identities in the shared crate. Native
Method reflection, removing that adapter and the local fallback, native Execution
construction and the final package split remain required; this change does not
establish independent Id packaging.


Validation: all 311 Id unit tests pass (15 ignored). A fresh debug installation
passes the independent native-loading harness, including registry queries and
mutations before and after Runtime, rejection of all-Object raw mutations,
ordered views, weak opaque shadowing, exact selected contexts, and visitor
mutation/failure propagation. Its 68 installed Id exports match the existing
manifest plus statically linked artifact exports; the native carrier and Runtime v11 bridge are
unchanged.

Dispatch-cache and protected storage-observation scripts pass source, source-free
compiled, mapped/rebuilt image and baked execution, including host observations,
custom lookup effects, mutation, shadow removal, GC, aliases and failure cleanup.
The deferred-identity image suite passes with actual writes `(386, 150)` normally
and `(386, 75)` under forced recovery in all three image modes. No release
validation was run. The full construction/package acceptance remains open.


### Saved Id entries and explicit dispatch context

Id's `lookup`, `addMethod` and `removeMethod:` primitives now carry their own
Id dictionary reference in the existing `NativeEntry.context` field. A native
consumer can select one through a lookup message and invoke it after that message
returns, while retaining the Id owner. Previously these entries had null contexts
and consulted a hidden thread-local dispatch scope, so an otherwise valid saved
primitive could abort outside `message` or use the wrong active Id.

The fixed bridge now passes its four existing Id facts explicitly through binding,
nested lookup and DNU dispatch. There is no Id thread-local context, enter/leave
guard, or Runtime-owned dispatch scope. The first two bootstrap bindings carry
the already-created dictionary reference; subsequent bindings still use Id's
message protocol. No public carrier, selector inventory or native export changes.

The upper all-Oop bridge distinguishes an actual legacy function adapter from a
native function merely recognized by legacy reflection. It invokes native entries
with their selected context, adapting the old allocation/mutation operands and
representable method results at that boundary. Recognizing a native equivalent
no longer authorizes invoking a different function against the active Runtime.
Canonical lookup guards compare the complete primitive retained from completed
Id. Restoring a legacy primitive address recovers that original native entry,
including its context. The guard also rejects lookup replaced before Runtime
construction; capturing a replacement does not make it canonical. Installing a
legacy lookup wrapper in the primitive's place would
prevent Id queries from returning native entries without legacy equivalents.

This removes a hidden dispatch prerequisite and fixes invocation context. It does
not finish language Method ownership: the legacy representable method-result
convention still exists, arbitrary native entries still require a checked wrapper,
and the upper adapter still recognizes primitive function identities. Owned
callables, native Execution construction and the package split remain open.


Validation: 316 Id unit tests pass (15 ignored), including direct saved primitive
calls across distinct Id registries, invocation against the selected native
context while another Runtime is active, allocation without an ambient Runtime,
restoration of the complete primitive, and effectful lookup installed before
Runtime construction. The fresh debug installation passes the independent native
loading harness and the dispatch-cache source/compiled/mapped/rebuilt/baked suite,
including the host checks. All 68 installed Id exports match the existing manifest
and statically linked artifact exports. Runtime v11 and the native carrier ABI are unchanged.

The independent native Integer admission test also passes all eight replacement
paths with matching optimization-disabled/enabled effects. This covers allocator,
lookup, Namespace, interning, Symbol validation, owner/parent and arithmetic
replacements after the lookup guard changes.

The deferred-identity suite passes source, compiled, mapped/rebuilt image and baked
execution, including host observations and forced recovery. Actual writes remain
`(386, 150)` normally and `(386, 75)` under forced recovery in all image modes.
No release validation was run. The remaining Rust `InitialObjects` handoff,
legacy Method adaptation, native Execution construction and independent Id
packaging are still architectural work, not acceptance established by these tests.


### Runtime receives Id through its native carrier

Production Id now keeps its state for process life, like its pinned primitive
native image. There is no `PENDING` owner slot or `take_initial_objects` transfer.
The [Runtime v12 constructor](../../abi/runtime-construction-v12.md) takes the
completed native Id carrier explicitly, followed by the existing completed lower
owners. Runtime stores a copied `Protocol`; it neither owns nor inspects a boxed
Id `State`. The host passes the same carrier it received from Siphon.

The upper constructor obtains Object/Symbol prototypes, behaviors and lookup/DNU
selectors through Id messages. It roots each query result before another native
call, preserves the current collecting interner and keeps those temporary roots
through construction. This also removes a stale private Symbol-prototype pointer:
if the early unbound Symbol has been retired, Runtime uses the current interned
identity. Compatibility storage inspection obtains Id roots through the native
root operation without retaining a RuntimeState borrow across the call.

Tests use the same constructor through an opaque forwarding carrier whose context
has a different Rust layout. Their separately created Id instances are kept alive
by a test-only erased lifetime guard, dropped after upper state. Production has
no such guard and no private Id owner handoff. The legacy `id_init` entry still
obtains a carrier from the process slot before calling this constructor; it is a
compatibility path, not the production constructor's implicit dependency.

The actual Cargo/package split, native Execution/Block construction, owned Method
reflection and remaining temporary interfaces are still outstanding. In
particular, native primitive identities in the legacy adapter/compiler admission
and local test registry layouts still need to be separated; this change does not
claim that moving a Rust pointer alone completes Id independence.

Validation: 345 ABI/Id unit tests pass (15 ignored), including the opaque-carrier
constructor and early Symbol retirement cases. The fresh debug installation
builds 55 native libraries and 28 stage-1 modules. The independent native-loading
harness passes, including collection during Runtime construction, invocation of
saved primitives afterward, and C rejection of malformed v12 inputs before and
after initialization without consuming owners. All 68 installed Id exports match
the manifest and statically linked artifact exports. Release validation remains waived.

The same v12 installation also passes the deferred-identity image suite through
source, compiled, mapped/rebuilt and baked execution. Its independently compiled
host uses the matching v12 constructor. Actual writes remain `(386, 150)` normally
and `(386, 75)` under forced recovery in all image modes.

### Original Id entries cross the boundary as a completed dependency

Bootstrap retains the original allocation, interning, lookup, add/remove,
allocation-size, behavior and Symbol-byte entries immediately after Siphon loads
Id, before loading Bootstrap or any other upper module. The temporary
[Runtime v13 constructor](../../abi/runtime-construction-v13.md) receives this
[complete record](../../abi/id-primitives-v1.md). It replaces the allocation-only
record without changing the Id carrier, native selectors or Id initialization.

Runtime's lookup and Integer admission compare exact retained entries, including
contexts. The legacy bridge recognizes operand conventions from those entries
and invokes the selected function/context. It no longer refers to private Id
primitive functions. Capturing current lookup after upper preparation is not a
substitute: an earlier effectful replacement must not become the default merely
because Runtime sees it first. Test construction retains the initial entries
before allowing early mutations, matching the production order.

This removes private primitive-symbol dependencies from those consumers. It does
not remove the remaining local registry/layout variants, complete owned Method
reflection or separate the Runtime package. Those remain acceptance work.

Validation: 347 ABI/Id unit tests pass (15 ignored). A production-constructor test
uses a provider whose lookup/add/remove implementations share one forwarding
function with distinct opaque contexts; another confirms an earlier effectful
lookup replacement is not admitted as the original primitive. The fresh debug
installation builds 55 native libraries and 28 stage-1 modules. The independent
native-loading suite passes, including automatic collection during construction,
saved-context invocation and malformed-constructor rejection. Integer admission
preserves optimization-disabled/enabled effects for all eight replacement paths.
All 68 installed exports match the v13 manifest and statically linked artifact exports.

The v13 dispatch-cache image suite also passes source-free compiled, mapped,
rebuilt and baked execution, including the separately compiled host observations.
It preserves lookup effects, mutation, shadow removal, collection, aliases and
reload. No release validation was run.

### Private Id implementation and production-only native clients

The Id graph, allocation registry, Symbol registry, dictionaries and native
dispatch now live behind the private modules in
[`id/mod.rs`](../../rust/birnam-id/src/id/mod.rs). Ordinary upper code cannot name their
Rust implementation types. The sole Rust facade is the existing legacy `id_init`
carrier getter; production construction continues to receive its carrier and
original entries explicitly. Tests and explicit `ffi-test-hooks` diagnostics
retain only the internal access required by their local harnesses.

Runtime's local Heap/Symbol/dictionary variants and constructors no longer compile
into the ordinary library. Native mutation results and Symbol-validation errors
have upper-owned Rust representations. The scanner's borrowed payload coordinates
belong to Heap. The existing preceding behavior word, allocation alignment and
rounded extent calculations live in
[`object_layout`](../../rust/birnam-abi/src/object_layout.rs); no object layout,
native descriptor or exported symbol changes. These ABI facts do not expose Id's
allocation records or dictionary layout. Source links and executable measurement
inventories follow the moved production files.

This is an enforced source boundary, not independent Cargo packaging. Runtime,
compiler/artifact dependencies and the legacy exports are still built by the
same crate. Separating those libraries, finishing native Execution construction
and removing the compatibility initialization path remain required.

Validation: 347 ABI/Id unit tests pass (15 ignored), and the normal and explicit
diagnostic/guarded-slot configurations compile. The fresh debug installation
builds all 55 native libraries and 28 stage-1 modules. The independent native
loading suite passes before and after Runtime construction, including allocation,
collection and saved contexts. Storage observation passes source, compiled,
mapped/rebuilt and baked execution, including nested rejection, failure cleanup,
roots, aliases and reload. All 68 installed exports match the unchanged manifest
and wrappers. No release validation was run.


### Separate production Id and Runtime libraries

The production package boundary now follows the existing native carrier boundary.
`birnam-id` contains Id allocation, Symbols, dictionaries, fixed native dispatch and
native loading, with only `birnam-abi` as a Cargo dependency. Its complete normal
export inventory is `birnam_native_module_v1` and `birnam_id_floor_identity`.
`birnam-runtime` contains the upper Runtime, Machine, collector integration and
compiler/artifact dependencies. Ordinary Runtime has no Rust dependency on Id;
only explicit diagnostic builds and unit tests compile the local Id test backend.

The installer publishes both dylibs. The host no longer looks for `id_init` or an
upper Runtime constructor in Id. After completed native predecessors exist, it
maps Runtime for its six remaining legacy factory preparations, then requests
Runtime's actual native initialization through Bootstrap and Id. Mapping that
upper image is still temporary construction machinery: it does not establish the
Execution base/compiler extension required by section 4. Eleven obsolete factory
preparations and their retained-handle fields are removed from the host.

The installed native-loading fixture audits both complete symbol inventories and
Id's system-only dynamic dependencies. Its isolated directory contains only the
actual Id dylib before upper construction. Independent C checks that Id has no
Runtime/legacy constructor exports; a separate C consumer checks Runtime's direct
and native-entry malformed constructor rejection before adoption. Existing early
allocation, dispatch, rooting, collector and saved-entry checks continue against
the same installed production libraries.

Validation: 352 ABI/Id/Runtime unit tests pass (15 ignored), 90 host/installer
unit tests pass (one ignored), and normal plus explicit diagnostic/guarded-slot
configurations compile. A debug installation builds 56 native libraries and 28
stage-1 modules; the complete independent native-loading suite passes. The final
installation also passes storage observation in source, source-free compiled,
mapped/rebuilt image and baked execution, including the real host's nested
rejection, failure cleanup, rooting and alias checks. Release validation remains
waived.

Remaining work includes the Execution base/compiler extension, owned Method
consumers, the six upper preparations, predecessor-manifest/diagram reconciliation
and worker acceptance. Older C/diagnostic drivers that still initialize the former
aggregate dylib through `id_init` or use `BIRNAM_ID_DYLIB` to replace Runtime need
migration to the new boundary; their previous results do not validate this split.
The older `compatibility-authority-test.py` currently fails its reviewed-image
comparison. Running the committed HEAD checker against committed HEAD image source
reproduces that failure; the split does not change image encoding or broaden the
checker to accept it. That comparison remains an open validation issue.

Earlier evidence sections above record intermediate states and their then-current
export counts. Their two additional artifact exports came from statically linked
`birnam-artifact`, not installer-generated wrappers.


### Runtime receives completed supporting factories

The final six host preparations have been removed. Method Observation, Storage
and Dispatch Cache now publish their existing complete factories through Id's
native loading entry, using the explicit [factory requests](../../abi/runtime-support-v1.md).
`FactoriesV1` carries the copied operations into Runtime v14; Heap, observation,
cache and guard consumers receive their actual operations during construction.
Their ordinary constructors no longer consult global factory slots. Normal Runtime
no longer exports the six dependency setters or the v13 constructor. Explicit
legacy diagnostic construction remains separate from the production path.

The host opens Runtime only through the real Id loader after all construction
arguments exist, then acquires its lookup handle after initialization. The three
empty legacy module entries and their `SCRIPT_MODULES` rows are removed. Their
replacement entries are the six named native requests in the factory document;
no language methods or objects were published by those removed rows.

This completes the supporting-factory migration, not the Execution/compiler
separation. Runtime still includes artifact/optimization policy, and the full
entry/predecessor manifest, owned Method migration, older diagnostic drivers and
worker acceptance remain required.


Validation: 363 ABI/Runtime/support-library unit tests pass (15 ignored), and
normal and explicit diagnostic/guarded-slot builds compile. The fresh debug
installation builds 56 native libraries and 28 authored stage-1 modules. The
independent native suite validates the exact installed export inventories,
exercises all six factories with Runtime unavailable, rejects malformed native
requests and eight malformed factory tables with otherwise valid predecessors,
then constructs Runtime from those same predecessors. Existing early-object,
collector, saved-entry and post-adoption checks also pass. Release validation
remains waived.

The final host/installer suite passes 90 tests (one ignored). The installed
storage-observation suite passes dynamic source, source-free compiled,
mapped/rebuilt native Code images and baked execution, including independent
host observations, nested rejection, failure cleanup, roots, aliases and reload.


### Native values and Block precede parser/compiler modules

Runtime now constructs `Block` and `BlockVt`, with Oop capture layout and the
ordinary behavior owner, alongside its execution objects. A native module can
create a concrete delegated Block behavior, install its native `value` method
and allocate captures without Prelude or a source compiler. Block Loop's own
`module_init` installs its complete native while methods using the existing
Block and Error Value; Prelude no longer constructs Block or attaches that
installer. Printing also installs its own Object/Symbol defaults before
collections; StringBuilder conversion and collection printing do not depend on
Prelude. Boolean's Block-taking controls, Nil/String/StringBuilder, collection
value operations, Vector/Range/Map/String collection composition, Error and Store
now load before Artifact, PEG, Lang, TCC and Codegen. These later modules receive
actual published values instead of relying on future initialization.

The installed native-loading suite now has a prefix containing the real shipped
Runtime and value modules, with Artifact, PEG, Lang, TCC, Codegen, Prelude,
Portable Code and Image libraries absent. The actual host startup must reach the
missing Artifact entry, after which an independent C consumer exercises native
Block capture/calling and precise collection, Boolean control, Vector callbacks,
Map, String splitting, Object/Symbol/collection/StringBuilder printing, loop error
propagation, Globals and basic Machine behavior.
The same check verifies Block identity survives Block Loop reload. Separate
copies of the shipped Block Loop image reenter first initialization during
selector interning and registration with another API table; execution retains
the original table selected before either callback.

This establishes the native value/control prefix. Runtime still physically links
compiler/artifact implementation and exposes its old admission machinery;
separating that advertised compiler extension remains required. The legacy value
entry/provider construction and full checked predecessor manifest also remain
open. The new prefix fixture does not establish completion of those boundaries.


Validation: 319 Runtime and 90 host/installer unit tests pass (16 ignored), plus
Prelude's identity test. The final debug installation builds all 56 native
libraries and 28 stage-1 modules. Its full native-loading suite passes, including
the prefix's printing/conversion cases, precise GC and both first-initialization
reentry cases. The printing source/compiled/native-Code-image/baked suite passes
on that installation. Block Loop source/compiled/native-Code-image/baked tests
pass after its installation and reentry changes; the broader storage-observation
source/compiled/mapped/rebuilt/baked suite also passes after the Block and value
ordering changes. Release validation remains waived.


### Execution base and explicit compiler extension

Runtime v15 receives the already constructed Integer, Float, Word and String
APIs before it publishes Machine/Block or installs collector extensions. These
are mandatory constructor inputs; legacy value entries reuse them. Machine's
Integer-valued queries work immediately after construction, before those legacy
entries run.

Artifact admission/catalogs and speculative native/numeric registries now belong
to an optional compiler extension. Production constructs it through Id with
`Bytes("compiler"), Word(Runtime API)` after Store, Artifact, PEG, Lang and TCC,
immediately before Codegen. Its provider returns null beforehand. Codegen
requires the completed extension and resolves compiler operations through its
[descriptor](../../abi/runtime-compiler-v1.md), without raw-export fallback.
Native Execution, Block and collecting value/control operations work while the
extension is absent. Both entries share a library, as section 4 permits.

This change does not finish owned Method reflection, the entry-level manifest,
legacy driver migration or the outstanding full image/worker acceptance. Those
remain part of Designs 82/83, not follow-up scope outside this implementation.


Validation for this change uses `/tmp/birnam-83-compiler-extension`, a fresh debug
installation with 56 native libraries and 28 compiled stage-1 modules. Runtime's
319 unit tests, host/installer's 90 tests and Codegen's 22 tests pass (20 tests
remain explicitly ignored across those suites). The installed native-loading
suite passes, including direct constructor rejection of incomplete value inputs,
immediate Machine queries, compiler-provider absence, foreign-carrier rejection,
complete extension resolution and repeated construction. The printing source,
source-free compiled, native-Code image and baked-executable checks also pass.
The unit suites use a separate unwind build directory from native harness builds;
release validation remains waived.

Block Loop's source/compiled/native-Code-image/baked checks pass on the same
installation, preserving captured blocks/cells, aliases, errors, GC and live
method/factory replacement. The storage-observation suite also passes across
source, compiled, mapped, rebuilt and baked execution, including host-side BUSY
checks, nested rejection, visitor failure cleanup, roots and allocation identity.
The diagnostic `ffi-test-hooks` Runtime configuration still type-checks.

### Owned Methods and anonymous compiled Units

Language reflection now returns an owned Method, preserving the exact native
function/context pair. `Object methodFor:`, `Vtable methodFor:`,
`Method invokeOn:`, `invokeOn:arguments:` and `Vtable addMethod:as:` use this
representation. TCC and Portable Code select the completed checked language-send
entry during construction. Invalid language lookup results become errors rather
than executable addresses. The unsafe legacy `IdApi.send` convention remains a
migration boundary. See [the language contract](../../abi/language-send-v1.md).

A Method retains its TCC Unit in an ordinary traced slot when its function comes
from anonymous compilation. Registered resumable functions additionally retain
their generation lease; other native functions retain their mapped image.
Native authors still own arbitrary native context memory and its object roots.
Method invocation roots the Method, receiver and arguments across callbacks.
Image saving rejects process-local Methods, including those with private behavior,
as `methodNotImageable`.

TCC and Codegen select completed register/forget operations from Runtime before
publishing their interfaces. TCC registers each generated address and owning Unit
before publishing bindings, and forgets weak address records before finalization.
Portable synchronous nested functions and resumable thunks use the same path.
Retaining the Unit is necessary because generated nested Block constructors read
that object as well as execute its native code.

Ordinary source-written Blocks use this path too. TCC fills a generated Unit
reference before publishing relocated code, and generated Block installation
registers the function with that Unit. Source evaluation no longer permanently
retains every Unit containing a Block. It roots active evaluation and outward
results, uses a Unit-owned cache, and registers normal finalization after
evaluation. Surviving Blocks, Methods and bindings determine the Unit's lifetime.
The separate-turn lifetime check covers this source path as well.

[Lifetime v2](../../abi/fixtures/lifetime-v2/README.md) adds native references
indexed by source object and a native key. Runtime uses behavior/selector keys
to retain a bound function's Unit. Trace visits these references only from live
source objects, allowing unreachable Unit/behavior cycles to collect. Removing
or replacing a binding takes effect immediately and retains the previous Unit
until the outer turn returns. This covers lookup that sends more messages or
triggers collection before the original dispatch finishes. Id has no new fields,
dependencies or executable-lifetime policy.

Synchronous portable Block entries validate arity, payload extent and traced
capture slots before reading captures. A String with enough bytes is not a valid
capture vector. No native pointer validation or sandboxing is introduced.

Acceptance uses the normal production installer and independent native-loading
fixtures. `method-values-smoke.sh` covers source, source-free compiled modules,
mapped/rebuilt images and baked executables, including live replacement/removal,
aliases, callback GC, malformed lookup results, capture checks and image rejection.
`method-unit-lifetime-smoke.sh` checks synchronous/resumable reflection and alias
survival after eviction/GC, followed by eventual Unit release across outer turns.
The native collector fixture additionally checks retention and collection of a
native-reference cycle before Runtime is present. These are production paths,
not a separate bootstrap experiment.

Remaining Designs 82/83 work includes unsafe native consumers and legacy
constructors, the complete entry/predecessor manifest, migration of diagnostic
drivers, worker/image acceptance and the final requirement-by-requirement audit.
The ownership work does not establish completion of those requirements.

The expanded ownership checks use the debug installation
`/tmp/birnam-83-method-ownership`. Separate REPL turns prove eventual native Unit
release for source, synchronous and resumable Blocks. Method source/compiled/
mapped/rebuilt/baked checks pass, including source-written Block reflection and
wrong-arity invocation. ABI, Runtime, Codegen, Codegen Runtime, TCC and Trace
unit checks pass; the normal installed Portable Code runner reports 114 assertions.
The native prefix suite covers Lifetime v2 before Runtime and its later adoption.
Storage observation also passes source, compiled, mapped, rebuilt and baked
execution, including host observation, nested BUSY rejection, visitor cleanup,
root preservation and allocation identity after GC. Release validation is waived.

The forced-source Portable Code runner exposed a generated-reference lifetime
bug: compiled `Test` answered nil after the nested framework load, while a fresh
`[Globals at: #Test]` found the object and executed all 114 assertions. The emitter
had embedded an unrooted Symbol address in each global reference, then the lookup
shim converted that Symbol back to a name. Collection during nested loading could
retire those Symbols before their generated references were used.

Generated global reads now call the existing name-based lookup directly. JIT
Symbol binding, per-artifact Symbol storage/initialization and the emitter's
baked-global lists are removed. The artifact initializer symbol remains because
candidate admission requires it. Existing `birnam_global_b` exports remain for
old artifact ABI consumers; newly emitted code does not use them. This removes
an ownership obligation rather than adding another native root table.

`scripts/source-global-lookup-smoke.sh` runs the Portable Code suite through
installed, forced-source and compiled entry paths and checks both the reported
assertion total and individual pass lines. Its forced-source check rejects the
previous `d9f61666` installation's silent zero exit. Broader worker and
image-authority acceptance remain outstanding.

The debug installation `/tmp/birnam-83-live-globals` passes all three modes
(114 assertions each), Method source/compiled/mapped/rebuilt/baked checks and
the separate source/synchronous/resumable Unit-reclamation checks. Codegen passes
97 unit tests; Codegen Runtime passes 22 with four existing ignored tests;
validation tooling passes eight. Its Id library is byte-identical to the
previous ownership installation used for the independent native-prefix checks.

The worker service check still fails its ten-second readiness deadline. Profiling
the real child shows repeated selection-table decoding during authored-module
admission; a later sample shows the host recompiling image code after loading
the image. Generation authority now indexes admitted named-method rows once per
calculation, preserving the existing independent digest fixture and validation.
This removes quadratic decoding, but does not by itself establish worker
acceptance or remove the host's additional image-recovery compilation.

On `/tmp/birnam-83-generation-index`, Codegen Runtime's 22 unit tests pass
(four remain ignored), including the frozen dispatch-base digest. The independent
image-worker reader verifies twelve replies and orderly shutdown on both the
previous and updated installations, with byte-identical transcripts. One serial
comparison observes readiness at 43.63 and 42.21 seconds respectively; this is
diagnostic timing, not a performance acceptance claim. The production service
suite remains blocked by its unchanged ten-second readiness limit.

### Bootstrap owns construction through Runtime

The production sequence from roots through Runtime now lives in
[`birnam-bootstrap/src/construction.rs`](../../rust/birnam-bootstrap/src/construction.rs).
Bootstrap captures Id's original entries, loads the completed predecessor modules
through the actual Id carrier, adopts roots, activates collection and supplies
Runtime's constructor inputs. The host's `InitialFloor` methods now delegate to
this constructor rather than reimplementing its steps.

Construction is selected through Bootstrap's existing common native descriptor
and returns a native entry retained by the Bootstrap owner. Its requests and
result kinds are specified in the [Bootstrap contract](../../abi/fixtures/bootstrap-v2/README.md).
The 64-byte owner ABI is unchanged. A constructor request removes its mutable
construction state from the shared owner until it returns; nested construction
fails, while ordinary nested module loads retain their existing behavior. Prefix
diagnostics stop this same production constructor before successors are available.

The host still loads later legacy modules and discovers Runtime's aggregate
capabilities. Their migration, the full predecessor manifest, worker/image
acceptance and the final architecture audit remain required. Moving this sequence
does not establish that those later modules satisfy the target construction rule.

Validation on `/tmp/birnam-83-bootstrap-construction` passes the independent native
prefix suite, all three global-lookup modes, and Method source/compiled/mapped/
rebuilt/baked checks. Host unit tests pass 86 cases with one existing ignored
case. The installed constructor and host sources match the tested worktree.
The native fixture directly obtains Bootstrap's constructor, checks missing
successors and rejects graph advancement from inside an initializer.

### Remove repeated lower initialization

The upper host sequence no longer invokes `module_init` on completed Namespace,
Float, Word, Object Model, Root Registry, GC Settings, Payload Layout, Trace Hooks,
Finalizers, Native Stack, Native Mark, Allocation Budget, Nil or String modules.
Their entries and values are already available from Bootstrap's construction.
`LoadedFloor` carries the completed value descriptors; `LoadedStack` uses them
immediately. The host no longer has a second Float/Word/String preparation path
or prepared-library handles.

Integer and Boolean control extensions remain in the upper sequence because they
install behavior requiring Runtime's Block representation. Trace/Collect legacy
provider attachment and other upper module entries also remain pending migration.
The compiler-free prefix fixture removes the fourteen completed library paths
after Runtime construction, then exercises the real remaining startup sequence.
It also checks the number of legacy initializations, since the platform loader
can reuse a mapped library even after its file is moved. The previous host
executes thirty initializations before Artifact; the updated host executes
sixteen. The same fixture rejects the previous host at that check and passes
with the updated host, including its execution, collection and Store assertions.

Validation on `/tmp/birnam-83-no-repeat` passes installed/forced-source/compiled
global lookup and Method source/compiled/mapped/rebuilt/baked execution with image
rejection. Host unit tests pass 86 cases with one existing ignored case. The
native prefix suite passes with the rebuilt host and the preceding installation's
native libraries, all 56 of which are byte-identical to the new installation.
The installed host source matches the worktree used for these checks.
