# 83 — Production Id construction and lifetimes

> **Superseded integration architecture:** [Design 84](84-message-only-native-modules.md)
> requires deleting cross-module function tables and using opaque objects and
> Id messages throughout production. Existing lifetime/safety requirements remain.

> **Status:** Production migration in progress, 2026-09-21. Id is independently
> packaged with only the ABI crate, and Bootstrap constructs the production
> module graph through Id. Legacy fixture migration, worker acceptance and the
> final requirements audit remain open. See [implementation status](#implementation-status)
> for current evidence and [migration history](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/83-implementation-history.md)
> for earlier records. Design 82 remains the failure record and dependency
> constraints; sections 1–6 below specify the required end state.
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
[60E section 8](../2026-10-runtime-refactors/60e-resumable-registration-and-entry.md#8-keeping-the-microcode-late-bound)
between storage safety and semantic correctness after replacement, and
[60C section 13](../2026-10-runtime-refactors/60c-stepmethod-calling-abi-options.md#13-deferred-and-rejected-alternatives)
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
[`resumable_c::emit_entry_thunk`](../../../rust/birnam-codegen/src/resumable_c.rs)
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

Relevant migration sites are [`ExecutionState`](../../../rust/birnam-runtime/src/execution.rs)
(`classify_resumable`, binding removal, `end_dispatch_read`),
[`Runtime`](../../../rust/birnam-runtime/src/runtime.rs) (retirement and Machine entry),
and [Codegen runtime](../../../rust/birnam-codegen-runtime/src/lib.rs) (mapped-owner
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
[`id-protocol-v1/id_protocol.h`](../../../rust/birnam-abi/tests/fixtures/id-protocol-v1/id_protocol.h)
and mirrored by [`id_protocol.rs`](../../../rust/birnam-abi/src/id_protocol.rs).
The current production Id dictionaries and core dispatch use these typed
operands. The common descriptor loader and production Siphon handoff are wired.
Upper modules now construct through this protocol with explicit predecessors.
The twelve-argument initializer and dependency-setter exports are removed.
The Runtime compiler-provider fallback and obsolete preparer/factory/provider
exports are removed. Old direct fixtures still require migration.

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
borrow crosses the callback. Outside collection, a nested query or allocation
is permitted; an allocation made by a visitor is absent from the already captured
allocation snapshot. During collection, the upper collector's phase restrictions
apply, including the prohibition on allocation. The caller keeps snapshot objects
live and coordinates collection or reclamation. This is borrowed enumeration, not a new owning snapshot service.
Allocation/root enumeration makes no ordering promise. A failed visitor stops
visitation immediately and returns its complete native outcome unchanged;
successful visitor values are ignored. Wrong operand kinds/counts or a visitor
without a method produce InvalidRequest.

GC sends these operations through the real Id message protocol. It may retain
receiver and selector identities, but must not capture their implementations at
construction and bypass lookup on subsequent calls. The next send observes the
current method. The former cached-callable exception was an error, removed under
[Design 90](../2026-10-runtime-refactors/90-functional-modules.md); GC has no exemption from message dispatch.

Fresh lookup and synchronous callbacks remain subject to the established upper
collection-phase rules: no allocation during collection, no recursive collection,
and no registry borrow across a callback. Trace or lookup failure during tracing
must propagate before sweep; partial marks never authorize reclamation. Preserve
the supported hook and finalizer effects rather than replacing those rules with
frozen service entries. Keeping an implementation alive for an already selected
active call is distinct from caching it for later sends. Initial Id code stays
pinned, and its state remains live. No future GC callback or upper root record is
stored in Id. The one-use detached-allocation release entry below owns a specific
resource; it is not a cached dependency implementation.

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
| 12 | Artifact codec, PEG, Lang, Portable Code and native compiler services; steps 2–11 | Existing parsing, portable representation and native compilation with their real Error, String, Vector, Block and Store dependencies. Lang follows PEG. Completed Runtime admission precedes TCC; Portable Code precedes Codegen. No hidden compiler remains in Id. |
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
waived. Worker readiness acceptance uses ordinary compiled-image startup. The
user also waived forced-source bootstrap timing as a readiness gate; source
execution and explicit image rebuilding remain separate functional checks.

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

## Implementation status

**Migration remains in progress.** Id and the production construction graph are
implemented; legacy test migration and complete acceptance are unfinished.
The [construction manifest](../../../abi/production-construction-v1.json) records the
executed native graph. The [migration history](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/83-implementation-history.md)
retains detailed implementation decisions, earlier tests and intermediate states.
It is not a substitute for the acceptance requirements in section 6.

The final compiled-image regression passes on the debug installation at
`/tmp/birnam-83-compiled-image-final`. The broader image and worker results below
used `/tmp/birnam-83-compiled-image-v3`. Earlier native, service and deferred-storage
acceptance below used `/tmp/birnam-83-retirement-index`; those results must not be
mistaken for a complete rerun on the newer installation. Current evidence:

| Area | Established evidence | Remaining closure |
|---|---|---|
| Minimal Id and common loading | Installed native suite passes independent Id startup, native operand/result handling and failure admission; binary and Cargo audits confirm Id depends only on ABI and Runtime is separate. | Include this evidence in the final requirement-by-requirement audit. |
| Explicit construction | Bootstrap constructs the production graph through applications with complete predecessor operands. Installed prefixes exercise real entries with successors absent, including early value errors and later extensions. | Audit every manifest entry against section 4, not just its presence in the file. |
| Same-heap GC adoption | Installed tests cover early native roots, activation failure, tracing, collection, finalization and weak-index retirement before and after Runtime. | Complete the section 5/6 audit, including the exact-address-reuse requirement. |
| Managed turns and executable lifetime | Owned Method dispatch, exact-entry adapters and detached retirement are implemented. Four Runtime turn tests cover outward roots, nested callbacks, finalized-owner detachment and bounded Machine returns beneath a synchronous caller. Earlier installed Method/image/lifecycle evidence is in the history. | Audit every release path and external callback entry; unit evidence alone is not proof for every generated-code path. |
| Source, compiled and image carriers | New saves include the compiled source-generation unit as well as portable code. Mapped/baked application startup passes with source compilation/evaluation disabled; explicit and missing-code recovery and malformed bundle rejection pass. Earlier service/deferred-storage matrices cover source-free, mapped/rebuilt and baked cases. | Complete the remaining consumer matrices and final same-installation acceptance audit. |
| Worker | Compiled-image readiness is 7.48 seconds with all twelve exact wire exchanges and shutdown passing. The service runner passes mapped/rebuilt Cell checks and all nineteen AOT service cases. | Include this evidence in final same-installation acceptance. The user waived forced-source readiness timing as a gate; its diagnostic still reports `handshakeTimeout`. |
| Test construction cleanup | Support admission, Runtime services, deferred storage and the namespace/reflection boundary oracles now use production Id/Bootstrap. The installed native suite includes all 43 service cases and 76 support admission cases. | Migrate the remaining `namespace-support.h` consumers and other obsolete test initializers. Older consumer/image runners are not claimed green. |
| Documentation | Current ABI guides describe explicit construction; detailed progress records are separated into the history. | Finalize the startup diagram and close this table from authoritative acceptance evidence. |

### Native reflection and Runtime admission

Object Model's enumeration capability is complete before Runtime. Its count,
index and selector operations call pinned Id entries selected during construction;
they do not depend on a future Runtime thread/observation admission hook. These
read-only native operations work during root collection and storage observation.
Native callers retain the owner-thread and valid-behavior obligations from
section 1. The old tests expecting Runtime diagnostics for foreign-thread,
stale-pointer or arbitrary-pointer calls to this native capability are retired.
Typed Id requests still reject a live non-behavior object before checking bounds,
and reject a non-Word index. Bounds misses on valid requests return Unit.

Runtime-facing Namespace presence/import/publication retain their null-name,
owner-thread and observation refusal checks, including precedence, and remain
callable from root callbacks. The new independent C matrix checks those exact
outcomes alongside ordered enumeration, weak membership and survivor order,
mutation, GC and typed request failures. It uses explicit completed services and
runs its storage observers after the native loading turn returns. No production
guard or policy was changed to make these tests pass.

On `/tmp/birnam-83-retirement-index`, the installed native suite passes all 43
service cases, 76 support admission cases and early construction/collector/value
checks. Its five service crates pass 40 unit tests with one existing ignored test. Reproduce with:

```sh
bash scripts/runtime-services-test.sh PREFIX reflection
python3 scripts/id-native-loading-smoke.py PREFIX
```

Evidence for this migration is `.cargo-target/83-reflection-evidence.json`.
Earlier current-prefix carrier evidence is in
`.cargo-target/83-retirement-index-evidence.json` and
`.cargo-target/83-deferred-storage-evidence.json`. Release validation remains
waived. None of these partial results declares the full Designs 82/83 goal done.
