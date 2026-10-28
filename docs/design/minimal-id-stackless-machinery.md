# Necessary machinery for Id, stackless execution and concurrency

2026-10-28. Source audit at `80b8a47c`, with new test-only probes. No runtime
implementation is changed by this document. The user explicitly permits changing
method-definition syntax and restricting what may be installed as a Method.
Stackless execution, concurrency and the simplicity/malleability of Id are the
requirements; preserving the present adapters and internal APIs is not.

This supersedes the proposed universal Method-metadata record as the starting
point for redesign. First identify which data and operations the chosen language
actually needs. A historical test of an implementation detail is not a reason to
rebuild that detail in another layer.

## Optimizer/deoptimization retained (user decision, 2026-10-28)

The user explicitly retains optimizer and deoptimization support, with near-native
performance as a goal while preserving liveness. This supersedes any suggestion
below that those capabilities might be dropped as unwanted features. Their current
implementation is still subject to simplification. Optimized-code metadata belongs
to the optimized code/version that needs it; it is not a reason for every Method
to acquire a general metadata record or for Id to learn optimization policy.

## Implementation checkpoint: frame-owned answers (2026-10-28)

The continuation-forwarding repair is implemented independently of the pending
method-definition redesign. The original counterexample below remains historical
evidence; current qualification is recorded in
[the forwarding receipt](../measurements/continuation-forwarding-oop-20261028.json).

Contract: a continuation may forward its answer without registering its identity
with Execution. Suspension, budget stops, effects and structured exits retain their
meaning. An actual synchronous native caller still forbids escaping its boundary.
Id continues to perform current lookup followed by direct Method entry.

Before:

```text
recognized answer -> direct frame call
unrecognized answer -> assume native reentry -> install an entry barrier
frame return -> pop a separate per-Machine answer stack
```

After:

```text
owned pending-call answer -> optional direct frame/optimizer path
other answer -> transfer through Id's existing pause operation
  accepted -> owned driver continues the same computation
  native barrier -> retain the actual synchronous entry boundary
frame return -> answer its retained continuation
capture / restore / protected exit -> move or discard answer with its frame
```

The actual transfer is necessary because an answer's representation does not tell
Execution whether a native caller is waiting. A boolean in the owned driver marks
an entry handoff that it should immediately continue; genuine lookup pauses and
budget stops retain their existing scheduling behavior. No Id operation, ABI,
foreign payload read, or cross-owner private representation was added.

The frame owns its answer Oop and whether returning completes a native/initial
drive. These replace the separate answer stack. It also retains the identity of
its current pending return; this replaces a saved arena handle in `ReturnSite`.
Capture changes arena handle generations, but preserves the logical pending call.
Return consumption and boundary completion clear that identity. Live and detached
frames trace both references through Execution's existing GC ownership.

`alias_return` is removed. The remaining Method-address index recognizes only
Execution's own pending answers for direct calls, tail replacement and optional
inlining/recovery. Unrecognized wrappers execute through the general path and
are actually called at return; they are not skipped as aliases.

Qualification covers direct/one/two independent wrappers; exactly-once wrapper
execution; budget stops and GC; suspension/resumption; protected exits both inside
and outside a wrapper; capture/restore with GC and consumed-continuation rejection;
and rejection at a real native callback boundary. The broader core and focused
checks retain custom lookup, native boundaries, task control, optimizer recovery
and lifetime coverage. This checkpoint does not complete direct Method definition,
construction purity, code-owner admission or full language/release qualification.

## Implementation checkpoint: direct stackless Method construction (2026-10-28)

`Execution methodFromBlock:` now constructs an executable Method from a Block
whose code is already owned by Execution. This is the first direct-construction
slice, not a switch of `Reflect` or the language's default definition path.
[Qualification](../measurements/direct-method-construction-oop-20261028.json).

The constructor checks the owned Block representation, rejects unfinished image
shells, verifies the executable against Execution's existing code registrations,
and checks the receiver formal and capture layout. It rejects arbitrary native
bodies, unrelated callable objects, missing/extra captures and null capture values.
It does not query optional `codeObject` metadata or attach unrelated code after
construction.

Before, defining a Method constructs a forwarder that later sends `value:` to a
body Block. The new constructor instead binds that Block as the lexical environment
of the already-compiled body. On entry, the actual message receiver supplies the
first formal parameter and the remaining message arguments supply the remaining
formals. Captures come from the bound environment, independently of the actual
receiver's representation. The original compiled body and descriptor are shared;
there is no second registration or recompilation per definition.

The one added owner operation is construction, not an application protocol.
The one additional instance reference is the lexical environment in Execution's
existing owned entry record. Default construction allocates a context and Method
without adding dictionary entries or reassociating behavior. Execution traces the
environment and retains the existing code owner. It reads only its own Block
format; executable identity queries and all cross-owner work use Id.

The existing body layout still uses its former Block-receiver slot for the lexical
environment. Keeping that layout preserves the input contract of existing inline
candidates and recovery descriptions while separating the actual Method receiver
at entry. An entered candidate or a declined candidate both produce the correct
receiver, captures and arguments in the new path. PC candidates remain attached
to the shared code registration. This does not introduce optimization metadata on
arbitrary native Methods.

The acceptance test poisons the source Block's `value:` and capture-access methods,
and separately poisons optional CodeObject inspection: direct entry still works.
It also checks an unchanged binding epoch across repeated construction, default
Method behavior, live captured Cells, replacement versus captured invocation,
optimizer entry/fallback, two independently suspended computations, and native
mapping retention/release after source roots disappear and the binding is removed.

Remaining integration is explicit: connect ordinary definitions and bootstrap,
preserve ordinary arity and DNU behavior, handle explicit native conversion at its
real boundary, replace optional reflection/image consumers, and remove the old
forwarding/templates/per-instance metadata behavior. Synchronous provider binding
and generic admission cleanup are not completed by this constructor. Construction
currently finds its existing code registration through Id identity queries; this
is construction work, not a new lookup on every Method send. No performance claim
is made for complete programs by this checkpoint.

## Implementation checkpoint: direct native conversion Methods (2026-10-28)

`Execution method:withNativeSignature:` constructs a Method for an explicit native
conversion contract. It binds the actual message receiver and lexical environment
without a method-definition forwarder. This is a real native boundary: argument
conversion, return conversion and failure observation remain required work.
[Qualification](../measurements/direct-native-method-oop-20261028.json).

Before, native signature state was duplicated under both a Method address and its
context in the general admission table. A separate Sequence held the executable,
and ordinary definitions added a forwarder and private metadata vtable around the
resulting Block. The new native constructor returns the actual native-conversion
Method, whose context retains the captured executable and lexical environment.

Parsed signature state now lives once per native context. The general admission
table holds only portable-code arities. Both the existing explicit typed Block and
the new Method use the same native conversion implementation. Native arity checks
stay at that boundary, including observer delivery on a rejected argument list.
`nativeSignatureOf:` works for Blocks and captured native Methods through the
owner's existing signature contexts; it does not add metadata to other Methods.
The context retains its declared Method for this explicit reflection operation,
replacing the second per-Method signature index. Construction/reflection can scan
those contexts; invocation reads its own context directly.

All context references are ordinary Sequence fields, read and written through Id.
Their existing owner supplies GC tracing. The signature table contains no Oops;
retirement removes dead context keys before address reuse. No Id operation,
payload catalog, shared private struct or allocation exemption was introduced.
Default direct construction neither edits bindings nor asks for optional portable
CodeObject metadata. The legacy typed Block's image metadata remains on its old
path pending the wider admission/reflection migration.

Tests cover pure construction, actual receiver/captures, poisoning the source
Block's `value:` lookup, immutable signature reflection, captured Method identity,
Word/Bytes conversion, exact native failure identity, observer receiver and detail,
rejection before native effects, a deferred branch that suspends and resumes across
GC, deferred failure observation, synchronous generated bodies, unfinished-image
rejection, and code mapping lifetime after source roots and the last binding are
released. Existing typed-Block image round trips, Block arity, numeric inputs and
publication failure handling continue to pass. Both native constructors now reject
unfinished image shells instead of making their unfinished bodies executable.

This makes the required native conversion constructor available. Default Reflect
migration, direct-Method image recipes and removal of the old definition and
admission machinery remain pending; this checkpoint does not qualify full startup
or the entire language migration.

## Implementation checkpoint: install supplied Methods directly (2026-10-28)

`Reflect defineMethod:on:as:` now installs an already executable Method without
constructing a body forwarder. The selector spelling and installation operation
are unchanged. Id's existing executable-identity operation distinguishes that
case; Reflect neither reads foreign Method payloads nor requests metadata from
the supplied Method. The existing Block/body construction path remains pending
migration, including its purity violation.

Before, every supplied object passed through `methodForArity:implementation:`.
After, an executable Method goes directly to the target's dictionary installation;
a body still follows the existing construction policy. No new registry, template,
context, descriptor, application protocol or Id operation was added by this step.
The operation retains the supplied Method's identity and that Method's own argument
contract. This permits both new Execution constructors to use the existing source
form without another wrapper.

The regression test makes the old constructor and the supplied Method's `isError`
query fail. Installation still succeeds, invokes neither, preserves Method identity
and advances the binding epoch only once for the requested binding on an already
private vtable. Direct native conversion and deferred suspension work through this
same Reflect operation. Legacy Block construction/binding tests and the core suite
continue to pass. [Evidence](../measurements/direct-method-installation-oop-20261028.json).

This is executable installation, not completion of default body construction or
optional reflection/image migration. The remaining synchronous code-provider
binding and owner descriptions must be completed before deleting the old path;
no generic forwarding fallback was added for those missing pieces.

## Implementation checkpoint: code-provider context binding (2026-10-28)

TCC Units now answer `declaredMethodAt:context:`. A successfully initialized Unit
selects one of its declared executables and constructs a Method with a supplied
Oop context. This supplies the owner operation needed by generated method bodies;
the compiler has not yet adopted it for ordinary synchronous definitions.
[Qualification](../measurements/tcc-context-binding-oop-20261028.json).

Before, TCC's private entry record retained `{unit, code}` and always supplied the
Unit as the generated function's context. Replacing the Method's context directly
would lose that private entry record. After, the record retains `{unit, code,
context}`; TCC still selects and retains the executable, while supplying the bound
context to that executable through the existing native calling convention.
Ordinary declared Methods retain the previous Unit context by default. Receivers
and arguments pass through unchanged. No forwarding send or second entry protocol
was introduced.

The added state is one context Oop per binding and the Unit's validated code and
calling-convention declarations for indexed construction. TCC alone interprets
that state. Its existing GC callback traces Unit and context, and entered calls
root both. Bound instances are not appended to the Unit's permanent reference
list: retaining compiled code must not retain every environment ever bound to it.
Construction changes no dictionary or binding epoch. Calls before successful
initialization, after a failed initialization, or with an invalid declaration
index are rejected before a binding is allocated. Earlier escaped Methods remain
valid after a successful initialization retry.

An independently declared C module verifies synchronous and continuation-aware
entries, default and bound contexts, unchanged receiver/arguments, collection
during entry, immediate/null contexts, and pure construction. Allocation identity
checks with a live positive control prove that an unreferenced bound environment
can die while the Unit stays live. A separately installed Method alone retains its
Unit and environment; removing its final binding returns native mapping count to
baseline. The core suite and focused direct-construction, continuation and optimizer
recovery checks pass.

This closes the code-provider interface gap. Generated body lowering, coherent
code/shape construction, default definition migration, and optional reflection/image
consumers still need integration; this checkpoint does not claim those completed.

## Implementation checkpoint: pure Method capture (2026-10-28)

The existing bootstrap Method `captured` implementation now allocates the captured
Method with the original behavior. Execution no longer reassociates its behavior
after allocation. This preserves the existing language-level capture behavior,
including custom Method annotations, while eliminating the binding-epoch change.
No selector, calling convention, payload format or Id contract was added.

Before: allocate the captured executable with default Method behavior, query the
original behavior, then reassociate the new Method. After: the Method owner copies
its executable/context/code-owner data and uses the original behavior as the
allocation argument. The generic allocator still receives an opaque size and
vtable; it does not interpret Method data or suppress any invalidation.

The regression checks an unchanged epoch for default and customized captures,
retained annotation behavior, distinct identity and unchanged invocation. Core,
Block arity/capture, direct Method/native Method and Machine checks pass.
[Evidence](../measurements/method-capture-purity-oop-20261028.json).
Default definition metadata and generic code admission remain separate open work.

## Finding

Basic Id invocation needs an executable Method, its opaque context and code
lifetime, plus the receiver and arguments of the current call. Stackless execution
adds an explicit answer destination and saved computation state. Concurrency adds
scheduling, waiting/resumption ownership and cleanup policy.

None of these properties requires the present MethodInvocation cache,
per-definition forwarding Method, per-Method metadata vtable, post-hoc portable
code attachment to arbitrary native Methods. Stacklessness alone does not require
speculative optimization or reconstruction; the user's performance requirement
retains them as a separate capability with the obligations described below.

Some implementation must provide each required responsibility below. This is not
proof that the proposed replacement is already implemented or that every current
structure can be deleted independently.

## Evidence, including a new counterexample

An isolated native probe loaded only Id, Roots, Collect and independent test
modules. Execution, Codegen, NativeBackend, Bootstrap, MethodInvocation, Store and
Generation were absent. It passed:

- normal answers and continuation objects returned as ordinary data;
- suspension/root handoff, interleaved work and collection;
- custom/metacircular lookup, suspension during lookup, replacement after selection,
  and entry without looking up the selected Method again;
- native barriers, late/consumed answers and captured-context lifetime;
- 10,001 tail entries through custom lookup, 60,007 work transitions, measured
  entry-stack span zero, and live allocations returning from 640 to 640 after GC.

This establishes that the continuation transfer mechanism does not intrinsically
need the language's Method-definition or Machine-registration machinery. It does
not qualify the full language scheduler, structured cancellation or every native
boundary. See the [source-bound receipt](../measurements/stackless-necessity-audit-20261028.json).

A second probe compares a direct answer with one and two independently declared
forwarding continuations. The wrappers only forward the same answer through the
public Id continuation operation; they do not register Execution aliases. All
three return 42 normally. The direct case parks and resumes, with effects occurring
once before and once after suspension. Both wrapped cases fail with
`invalidRequest` after the first effect instead of suspending.

This is a demonstrated continuation-transparency failure in the current Execution
integration. Its entry path recognizes answer Method identities in `return_methods`;
a recognized answer joins a pending caller frame, while an unrecognized answer
enters a different drive path. `alias_return` supplies recognition for selected
built-in wrappers. The observed control difference is not merely an efficiency
choice. Do not fix it by registering each additional wrapper.

The complete failure/cancellation/cleanup/frame-inspection matrix remains to be
run against a replacement. This counterexample establishes a problem, not its
finished repair or a new defect in the underlying Id transfer mechanism.

## Required responsibilities

| Responsibility | Why it is necessary | Minimum state or operation; owner |
| --- | --- | --- |
| Current message lookup and direct entry | Replacement and delegation must affect subsequent sends; a selected Method must not be selected again | Id owns dictionaries and lookup. A send supplies receiver, selector, arguments and, for continuing work, an answer destination. |
| Executable meaning and context | An Oop reference alone does not specify which native code to enter or what state that code uses | The executable owner supplies a valid entry and opaque Oop context. Existing code-owner retention must remain. No universal language-metadata record is implied. |
| Lexical environment | Closures and captured Methods must retain their captured references and shared Cells | The compiled-code/closure owner holds the actual captures. The call receiver is separate and is supplied at invocation. |
| Explicit continuation transfer and bounded native stack | A suspended computation cannot depend on an expired C/Rust frame; tail calls must not accumulate native frames | Keep the Id transfer/trampoline capability. Answer and failure delivery must be distinguishable from returning a continuation object as data. |
| Saved activation state | Non-tail calls and waits must retain unfinished work | A resume point (PC or executable continuation), live values, receiver/environment where needed, and answer destination. Owned by Execution or the compiled activation owner, opaque to Id. |
| Root/liveness and code-lifetime discipline | Collection and unloading must not destroy values or code still needed after a callback or suspension | Owner tracing and entered/suspended references. Dead references must be cleared or excluded by liveness information; all-Oop storage alone does not solve dead-value retention. |
| Runnable/waiting/completed state and scheduling | More than one computation must make progress and resume the right waiter | Task-owned state, a scheduler's runnable work, and one-shot/stale-resume checks. Budget checkpoints must be sufficient for cooperative fairness. |
| Native boundary rules | An unaware native caller cannot be resumed after its native stack has gone away | Synchronous barriers and explicit asynchronous completion with retained state/resources. These belong at actual native boundaries. |
| Cancellation, failure and cleanup | Existing tasks own resources and child work; dropping references is not equivalent to executing required cleanup | Structured task/handler state with once-only effects and explicit outcomes. Keep the behavior, not necessarily the current delimiter/arena representation. |
| Parallel isolation, when parallel workers are retained | Cooperative interleaving is not parallel execution; concurrent access to a single-owner Id heap is not made safe by scheduling | Preserve an explicit domain/worker ownership boundary. Current Isolate/worker transport can remain separate from invocation. |

Receiver and arguments are call state. Lexical captures are callable-instance
state. A scheduler queue is scheduler state. None belongs in a global Method
metadata database merely because execution is stackless.

## Current machinery: disposition and reason

| Current machinery | Disposition | Reason / replacement boundary |
| --- | --- | --- |
| Id `prepareSend`, `prepareEntry`, `answerTo:with:`, driver, pause and native barriers | Keep the capabilities | Independently exercised without the higher runtime. Do not replace them with implicit `apply`/`invoke` lookup on selected Methods. Exact work-object allocation strategy is not essential. |
| `Reflect` body-object → Method forwarding | Remove from the default definition contract | Needed only to promise a later `value:` send to an arbitrary supplied body object. The user permits retiring that implicit conversion. Explicit forwarding can be written as an ordinary method body. |
| `MethodInvocation` arity cache and forwarding templates | Remove once direct method construction is usable | They select/cache the preceding forwarding operation; the current native entry already handles every supported arity. Sharing compiled body code remains useful and is a different responsibility. |
| Per-Method implementation/synchronous/binder/DNU getter vtables | Remove | They turn construction into dictionary mutation and violate purity. Do not replace their full contents with a universal weak table by default. |
| Outer-adapter arity/prefix/native-policy flags | Remove as universal Method state | Binding formal arguments is necessary, but it belongs to the actual executable or explicit boundary. DNU remains a real lookup-failure protocol, with handler-specific argument handling. |
| Generic `admitCodeObject:` mutation of existing Methods | Replace with construction by the code owner | The owner creating compiled executable code can establish the code association and shape together. Arbitrary native code need not acquire a portable CodeObject, and unrelated code must not be attached as if its correspondence had been verified. |
| Per-code arity, environment shape and saved-state layout | Keep where the compiled execution format needs them | These are facts about code, not a second application protocol. Validate external code/captures before use. Share immutable descriptions rather than reconstructing them per binding. |
| Capture values and mutable captured Cells | Keep | They are program state. Moving captures from the call receiver to an explicit lexical context preserves the distinction between receiver and environment. |
| `return_methods` identity recognition / `alias_return` | General forwarding repaired; `alias_return` removed | The original forwarding probe failed. Opaque answer forwarding must not decide whether task control works. A possible direction is to retain and invoke the actual answer continuation at frame return rather than decode its identity into a special return path. Execution context must come from the owned drive, not a guess about the answer's internals. |
| Pending call/result destination | Keep the responsibility | A caller must receive its result in the right suspended computation, once. A caller-owned continuation can contain this state; a reverse Method-address index is not inherently required. |
| Machine/frame arenas, capability slots and generation counters | Representation choices to simplify after control equivalence is established | They currently supply saved state, stale-handle checks and ownership. Those properties are required where exposed; this exact collection of indexed tables and generations is not. Do not delete checks before replacing their guarantees. |
| Machine action objects plus Id work objects | Consolidation candidate | Both encode control transitions. Continue/call/return/wait are necessary operations, but two complete encodings/drivers are not a consequence of stacklessness. Prove one coherent execution path before removal. |
| Per-PC root maps and edge clearing | Keep a sufficient liveness mechanism | May be replaced by compiler-managed clearing/owner-specific layouts, but cannot simply be removed while stale references remain in frames. |
| Oop call-preparation plans | Optional implementation optimization | Argument order, aliases and current cross-owner operations are necessary. The compact plan format and admission protocol are not fundamental to sending or concurrency. |
| Optimizer, guarded specialized code and deoptimization | Retain as an explicitly requested capability; simplify the mechanisms | Optimized execution needs justified assumptions, safe points, baseline recovery state and code lifetime. Associate these with optimized code, not every Method. Current inline/PC candidates and reconstruction algorithms must earn their complexity against this contract; retaining optimization does not require restoring every historical producer or API. |
| First-class frame capture/restore | Optional control feature | Basic wait/resume needs a valid one-shot suspension, not arbitrary stack snapshots or replay. Retained structured cleanup/exit behavior must be implemented independently of speculative deoptimization. |
| Synchronous versus resumable language-body modes | Prefer one continuation-capable language path | A second language execution/error convention is not required. Real synchronous native calls still need a boundary; their ABI declarations are not universal language metadata. |
| Explicit native signature conversion and byte snapshots | Keep at actual native/foreign boundaries | They validate real argument/result contracts and keep borrowed data safe. They are not required by every pure Birnam Method. |
| Ambient failure-preservation policy | Remove implicit use; audit remaining explicit scopes | Typed invocation's implicit use already broke callback handling. Necessary failure/outcome delivery does not imply a process-wide policy flag. Any retained explicit scope must restore on every exit and never leak across suspended tasks. |
| CodeObject/source maps, image recipes and artifact selection | Keep with compiler/persistence/debug owners for retained features | These support validation, rebuilding, inspection and loading. They must not become prerequisites for every native Method or ordinary Id send. |
| Generation coordination, Store descriptions, rollback and certificates | Explicit optional services | The object model and scheduler do not require them. Current value-checked rollback and basic image-loading decisions remain applicable. |

## Recommended method-definition contract

1. A dictionary installation receives an executable Method Oop. No automatic
   conversion of arbitrary callable objects into Methods.
2. A method-definition form compiles a method body, with the call receiver bound
   directly as its receiver and lexical captures supplied separately as context.
   Exact surface syntax is still to be designed. This is not a requirement to
   compile native code on every definition: compiled body code is shared, while
   each definition binds its environment.
3. The selected Method is entered directly. Replacing the dictionary binding
   changes subsequent sends; a captured Method retains its selected executable
   meaning and environment.
4. A body that wants dynamic delegation writes that send explicitly. There is no
   automatic second `value:` lookup merely because a method was defined.
5. Valid executable status is a code-owner contract, not an allocation type tag or
   constructor-origin catalog in Id. Native callables remain possible without
   source, CodeObjects, frame descriptors or a general metadata record.
6. Specify one ordinary compiled-method argument rule. Do not accidentally retain
   different missing/extra-argument behaviors solely because three historical
   wrappers performed different checks. This audit does not silently choose a
   new arity rule or alter the existing runtime.

Current versus proposed normal definition/call:

```text
current definition:
  create implementation Block -> choose/cache forwarding template
  -> construct forwarding Method + private metadata vtable -> install
current call:
  lookup installed Method -> enter forwarder -> send Block value:...
  -> enter the compiled body

proposed definition:
  compile/select actual method body -> bind lexical environment -> install Method
proposed call:
  current lookup -> enter that Method with receiver, context, arguments and answer
```

The compiler currently treats the Block receiver as the source of captures and
ignores the native `context` parameter in emitted synchronous bodies. That is a
concrete source of the forwarding requirement, not a law of the object model.
Change lowering to distinguish receiver from lexical environment.

A real interface seam must be solved explicitly: TCC currently uses its Method
context to identify `{unit, code}` and passes the Unit as the generated function's
context. It cannot simply be overwritten with captures. The code provider now offers
`declaredMethodAt:context:` to bind an environment while retaining the code and
Unit. The compiler must still adopt that operation for method bodies. This is code construction, not a universal metadata
service. No foreign payload read or shared private struct is justified.

Similarly, per-code descriptions needed by stackless execution must be created
by that code's owner, with captures validated at binding. Removing generic post-hoc
admission does not authorize unchecked shapes, false code associations, or unsafe
image reconstruction. Tests for those obligations move to the actual constructors.
The existing implementation remains defective until a replacement is verified.

## How optimization fits the smaller runtime

Baseline and optimized execution implement the same language contract. Optimized
code specializes regions whose assumptions can be checked, and returns to a valid
baseline continuation when those assumptions no longer hold. Deoptimization must
not restart an already-effectful computation.

The necessary additional information is specific:

| Optimized-code information | Purpose |
| --- | --- |
| Baseline code identity and resume point | Identify the remaining computation after leaving optimized code. |
| Guarded assumptions and dependencies | Prove that omitted lookups, calls, allocation or conversions preserve current meanings. Recheck after callbacks, mutation and resumption where required. |
| Live-value/materialization description at exit points | Recover correct logical values, aliases, captures and observable frames when optimization removed or changed their physical representation. |
| Root information and owned code references | Preserve values and the precise code version needed by running or suspended work. |
| Safe-point/control information | Permit bounded scheduling progress, cancellation, GC and required inspection without retaining an invalid native stack. |

These descriptions can be shared per compiled region/version. Simple or
unoptimized Methods do not need speculative assumptions or reconstruction recipes.
Some guard/invalidation and recovery coordination is real machinery and must stay;
per-definition forwarding, arity-template caching, private metadata vtables and
recognition of particular answer wrappers are not prerequisites for it.

Specialized regions can keep temporary values in native registers and use native
operations when justified. Oop-only Id messages do not prohibit unboxed internal
temporaries: owner-boundary arguments/results remain Oops, and safepoints must have
a valid root/materialization plan. Long-running optimized loops need bounded
polling/yield opportunities; native speed is not permission to starve other tasks.

Malleability constrains specialization. Inlining or omitting an operation requires
valid proofs about its selected meaning and actual effects; replacements or changed
lookup behavior must force an appropriate current path. Code and live data cannot
be frozen merely to keep an optimization enabled. Failed guards and deoptimization
must not replay lookup effects, calls, allocations with required effects or cleanup.
Ordinary Id sends retain lookup followed by direct entry of the selected Method.

Performance qualification must compare representative complete programs, including
guards, safepoints, compilation, allocation/GC and native boundaries as applicable.
Use matched native implementations with equivalent arithmetic/error semantics as
reference points. Record mutation-heavy and suspension/cancellation cases as well
as stable hot loops. Near-native performance is a goal, not a claim established by
this audit or a promise for inherently dynamic message-heavy code.

## Reflection and persistence are explicit design constraints

Dictionary inspection, Method identity and captured invocation remain straightforward
object-model operations. Access to a compiled method's environment, source or image
recipe is a richer owner protocol. It must not force every native Method into one
metadata schema.

The current bootstrap Method does not expose its opaque context as a getter.
For richer reflection/persistence, decide explicitly between suitable owner
operations and a small bootstrap-Method reflection operation. Do not invent a
weak registry, inspect a foreign payload, use GC enumeration as a context-getter
side channel, or encode a secret metadata request as an ordinary invocation to
avoid acknowledging this interface question. Basic invocation and stackless
transfer themselves do not require that getter.

## Minimum control-state model to prove

- A compiled-code object or equivalent immutable owner description identifies
  executable meaning and any required activation layout.
- A callable instance retains its lexical environment and executable resources.
- An unfinished activation retains its resume point, live values and actual answer
  continuation. A callee need not recognize the answer object's implementation.
- An owned execution/task context supplies scheduling and structured control.
  Entering an unknown answer wrapper is not itself evidence of a new native barrier
  or a different task. Real native boundaries remain explicit.
- A scheduler retains runnable work or a waiting completion capability; completion,
  cancellation and consumption have defined ownership and exactly-once effects.

These are roles, not a mandated count of heap objects. A PC may be encoded by an
executable continuation. Immutable code descriptions may be shared. A frame arena
may remain if it earns its complexity. Avoid introducing a new object for every
noun in this list.

## Source cleanup already identifiable

Compiler dependency files and the crate module declarations show 14 legacy
Execution source files and seven legacy NativeBackend source files outside the
production release input closure. Two additional Execution files are deliberately
`cfg(test)` inputs. The exact list is in the receipt. This is source/test burden,
not proof of runtime cost. Check scripts, test-only includes and retained bootstrap
consumers before deleting files; production absence alone is not a complete
workspace-consumer audit.

## Source map

| Area | Current source |
| --- | --- |
| Direct dispatch and Method entry | [Id dispatch](../../rust/birnam-id/src/id/id_dispatch.rs), [bootstrap Method owner](../../rust/birnam-id/src/id/bootstrap_values.rs) |
| Transfer loop, resumable lookup and native barriers | [Id continuations](../../rust/birnam-id/src/id/continuation.rs) |
| Definition forwarding and per-Method behavior | [Codegen runtime](../../rust/birnam-codegen-runtime/src/lib.rs), [authored MethodInvocation](../../modules/method-invocation.birnam) |
| Generated capture access and steps | [synchronous emission](../../rust/birnam-native-backend/src/oop_emission.rs), [resumable emission](../../rust/birnam-native-backend/src/oop_resumable.rs) |
| Compiled entry/context ownership | [TCC owner](../../rust/birnam-tcc/src/lib.rs) |
| Pending calls and continuation recognition | [resumable runtime](../../rust/birnam-execution/src/resumable_runtime.rs) |
| Frames, registrations and leases | [Execution engine](../../rust/birnam-execution/src/execution.rs) |
| Metadata and admission | [Execution reflection](../../rust/birnam-execution/src/oop_reflection.rs) |
| Native conversions and explicit failure scopes | [native adapters](../../rust/birnam-execution/src/oop_native.rs) |
| Optimizer/recovery consumers | [native tier](../../rust/birnam-execution/src/oop_native_tier.rs), [recovery](../../rust/birnam-execution/src/oop_recovery.rs) |
| Task scheduling, waiting, cancellation and cleanup | [authored stackless control](../../modules/stackless-control.birnam) |
| Reproduced forwarding failure | [test](../../rust/birnam-siphon/tests/continuation_transparency.rs), [independent wrapper](../../tests/fixtures/oop-continuation/transparent-answer.c) |

## Proof and implementation order

1. Record the changed definition contract and directly compile one Method with
   captures. Prove correct receiver/arguments, pure construction, live binding
   replacement, captured meaning, GC/code lifetime and no hidden `value:` send.
2. Repair/prove general continuation forwarding before extending the current
   recognition mechanism. Cover one/nested wrappers, lookup suspension, waits,
   failure identity, native barriers, cancellation/cleanup, and supported inspection.
3. Migrate definition sites and bootstrap, then delete default forwarding/template
   caching/per-instance metadata behavior. Do not ship both paths indefinitely as
   compatibility machinery. Explicitly rebuild incompatible artifacts.
4. Keep code-shape validation at the code owner. Remove general Method metadata
   requirements that no longer serve a retained feature. Do not claim the current
   admission failures repaired merely by removing their old API tests.
5. Retain and reconnect optimizer/deoptimization support against the direct-method
   baseline. Remove obsolete or redundant optimization machinery only after its
   required role is covered; simplify frame/action representations against the
   control/lifetime tests.
6. Qualify one corrected revision: architectural review, full native acceptance,
   clean cold build, relocated archive and end-to-end Host/concurrency checks.

This is the proposed simplification boundary. It establishes the required jobs,
identifies concrete excess and exposes one control failure. It is not a completed
runtime redesign, a claim of a unique mathematically minimal implementation, or
permission to replace missing proof with a passing subset of tests.
