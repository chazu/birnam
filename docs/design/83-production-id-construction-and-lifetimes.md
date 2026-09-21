# 83 — Production Id construction and lifetimes

> **Status:** Production migration in progress, 2026-09-21. Exact-entry adapters
> and managed-turn checkpoints are implemented in the existing production
> runtime. Core Id allocation, Symbols, dictionaries and typed dispatch no longer
> require upper services. Siphon now loads the real Id through the common native
> protocol before upper preparation. Library ownership, upper construction and
> consumer migration remain open.
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
| `allocationSizeOf:` | Id, Object candidate | Requested payload bytes as Word, excluding header/padding; Unit if unknown |
| `identityOf:` | Id, Object candidate | Stable allocation identity as Word; Unit if unknown |
| `symbolBytesOf:` | Id, Object candidate | Borrowed name bytes without the trailing NUL; Unit for a non-Symbol |
| `reassociateBehaviorOf:with:` | Id, Object receiver and allocated Object behavior | Updates the header and dictionary mutation epoch; returns receiver |
| `behaviorCount` | Id, no arguments | Registered behavior count as Word |
| `behaviorAt:` | Id, Word index | Behavior Object, or Unit beyond the registry |
| `selectorAt:in:` | Id, Word index and Object registered behavior | Direct selector Object, or Unit beyond that dictionary |
| `parentOf:` | Id, Object candidate | Parent Object, or Unit for absent/unknown parent |
| `parentOf:put:` | Id, Object registered behavior and Object parent (or Unit/null to clear) | Updates delegation and mutation epoch; returns behavior |

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

**Production migration is in progress. Id is not yet independently packaged.**
The same production Id now initializes before upper libraries are present, but
its crate still contains Runtime and statically linked compiler/artifact code.
Independent first entry is evidence for the loading path, not architectural
acceptance of the complete design.

The host's [`Siphon`](../../rust/birnam-host/src/siphon.rs) supplies the small ad
hoc loading protocol and invokes `birnam_native_module_v1`. The real Id descriptor
constructs `InitialObjects` using only Id allocation, dictionaries and Symbols;
its own bind/invoke path then installs native loading, construction and storage
operations. Core Id dispatch has no Runtime, GC, Execution or future-provider
slot. Native descriptor admission closes rejected images before entry and pins
entered images, including failed initializers with partial publication.

Upper startup adopts the exact initial registries, objects and carrier. Each
registry has one owner and a stable address; no graph is copied and no carrier
slot is retargeted. Eleven upper factory preparations still run in the host
after native Id/Bootstrap/root-registry/Lifetime/base-value/Namespace/Object-Model/String/Error initialization and feed the aggregate Runtime constructor. That
ordering improvement does not implement the construction graph in section 4.

The first upper native entry is now
`libbirnam_bootstrap.dylib:birnam_native_module_v1`. Its only predecessor is Id.
Its [native owner](../../abi/fixtures/bootstrap-v1/README.md) calls Id's current
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
prefix. `birnam_runtime_init_v5` is an upper construction entry taking the completed
root and representation owners plus completed base values, Namespace and Object Model, temporarily packaged in the Id library along with
Runtime. The intermediate v1/v2/v3/v4 Runtime bridges are removed. The old `id_init` and factory preparation path remain
for legacy consumers and fixtures; this legacy construction
is not used by the current production host with the current Id. Remove it with
those consumers during the ownership cutover. The production path no longer
prepares a root-registry factory inside Id or constructs a second registry.

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

[`LifetimeOwnedV1`](../../abi/fixtures/lifetime-v1/README.md) owns the exact payload
byte descriptions, trace-hook attachments and finalizer records. Runtime adopts
that owner through `birnam_runtime_init_v5`; its behavior/heap tables retain no
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
are published, and staged collector activation remain required. This prefix does
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
The remaining value stages and full construction graph remain unimplemented.

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
The old numeric compiler declaration assumes direct allocation and therefore
returns zero for this message-based constructor. Native-aware compiler admission
must still be implemented; this fallback is not the completed compiler design.

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
Collector activation, Execution/Block, native-aware compiler admission and the
remaining upper/package separation are still open.

`ObjectMemory` owns allocations and identities. `Heap` adds mark/permanence
records, budget accounting and deferred allocations. Allocation records use
three native words and GC records one, in separate maps; Lifetime owns the
separate finalizer entries. This metadata overhead is outside payload-byte
accounting. `Vtables` and `Symbols` require only Id allocation and their own
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

The production collector acquires the five Id storage callables described
above through Id lookup during construction. It uses them for primordial roots,
permanent allocation references and each marked object's Id references. Upper
roots, behavior owners, payload layouts/hooks and mark metadata stay above the
interface. No Id registry borrow crosses a visitor. The diagnostic storage view,
legacy Symbol compatibility mode, adoption and Execution's dead-binding accounting
still access internal Rust storage. Production index retirement, allocation
detachment and release now use the native storage callables. Upper behavior
metadata and per-allocation finalizer records retire separately; the coordinator
keeps detached payloads readable until their finalizers return. Lifetime's root and representation owners now initialize before Runtime;
separate GC activation and the full Id allocation/registry adoption cutover
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
4. Implement disabled → adopting → active collector activation, complete root and
   representation checks, and failed activation without partial collection.
5. Replace the aggregate preparations/initialization with real base and extension
   entries. Publish the entry-level predecessor manifest, map every SCRIPT_MODULES
   entry or removal, reject cycles/missing predecessors, and update the diagram.
6. Prove real construction prefixes, source/compiled/image/worker behavior and
   the source/Cargo/private-export independence audits specified above. Passing
   the current tests does not close these unimplemented requirements.
