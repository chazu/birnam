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

## Finding

Basic Id invocation needs an executable Method, its opaque context and code
lifetime, plus the receiver and arguments of the current call. Stackless execution
adds an explicit answer destination and saved computation state. Concurrency adds
scheduling, waiting/resumption ownership and cleanup policy.

None of these properties requires the present MethodInvocation cache,
per-definition forwarding Method, per-Method metadata vtable, post-hoc portable
code attachment to arbitrary native Methods, or speculative optimization and
reconstruction framework.

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
| `return_methods` identity recognition / `alias_return` | Replace the semantic dependency | The forwarding probe fails. Opaque answer forwarding must not decide whether task control works. A possible direction is to retain and invoke the actual answer continuation at frame return rather than decode its identity into a special return path. Execution context must come from the owned drive, not a guess about the answer's internals. |
| Pending call/result destination | Keep the responsibility | A caller must receive its result in the right suspended computation, once. A caller-owned continuation can contain this state; a reverse Method-address index is not inherently required. |
| Machine/frame arenas, capability slots and generation counters | Representation choices to simplify after control equivalence is established | They currently supply saved state, stale-handle checks and ownership. Those properties are required where exposed; this exact collection of indexed tables and generations is not. Do not delete checks before replacing their guarantees. |
| Machine action objects plus Id work objects | Consolidation candidate | Both encode control transitions. Continue/call/return/wait are necessary operations, but two complete encodings/drivers are not a consequence of stacklessness. Prove one coherent execution path before removal. |
| Per-PC root maps and edge clearing | Keep a sufficient liveness mechanism | May be replaced by compiler-managed clearing/owner-specific layouts, but cannot simply be removed while stale references remain in frames. |
| Oop call-preparation plans | Optional implementation optimization | Argument order, aliases and current cross-owner operations are necessary. The compact plan format and admission protocol are not fundamental to sending or concurrency. |
| Inline candidates, hot tiers, discarded-clone/numeric producers, recovery graphs and reconstruction recipes | Remove from the minimum runtime; retain only for a separately justified optimization feature | Stacklessness does not require deoptimization. Unoptimized code can expose its actual state. Authored task code has no calls to these optimization APIs in the audited source. Existing explicit optimizer APIs are a separate feature decision, not a reason to burden every call. |
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
context. It cannot simply be overwritten with captures. The code provider must
bind an environment to its executable entry through an owner operation, retaining
both code and environment. This is code construction, not a universal metadata
service. No foreign payload read or shared private struct is justified.

Similarly, per-code descriptions needed by stackless execution must be created
by that code's owner, with captures validated at binding. Removing generic post-hoc
admission does not authorize unchecked shapes, false code associations, or unsafe
image reconstruction. Tests for those obligations move to the actual constructors.
The existing implementation remains defective until a replacement is verified.

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
5. Strip optimizer-only and obsolete source machinery after consumer/feature audit;
   simplify frame/action representations against the control/lifetime tests.
6. Qualify one corrected revision: architectural review, full native acceptance,
   clean cold build, relocated archive and end-to-end Host/concurrency checks.

This is the proposed simplification boundary. It establishes the required jobs,
identifies concrete excess and exposes one control failure. It is not a completed
runtime redesign, a claim of a unique mathematically minimal implementation, or
permission to replace missing proof with a passing subset of tests.
