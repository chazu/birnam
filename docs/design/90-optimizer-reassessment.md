# Design 90 — Optimizer reassessment

2026-09-24. Applies the revised [language-behavior contract](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history)
to the next batch. Existing code is evidence to inspect, not a list of mechanisms
to restore. This review changes production construction and removes an unused API;
it does not port the retained allocation-elision implementations.

## Decisions from the current code

| Area | Finding and decision |
| --- | --- |
| Eager PC candidates | Codegen emitted a Method for every eligible PC plus an installation Vector, despite retaining the same sends, allocations and actions. The candidate mainly selected a body without the ordinary switch. Remove emission, installation and the associated size-limit policy. |
| Generated C structure | The separate dispatcher and per-PC body functions existed to share code with candidates. Remove those extra calls and by-value `Call` transfers too. One resumable entry contains the switch and owns its temporary roots. |
| Generic candidate guards | `Execution::native_tier::select` checks default lookup and seven Machine operations, with behavior-epoch checks around effectful lookup. These checks were added overhead for the removed producer. Keep them for explicit native candidates, which can bypass those operations. This does not establish that all seven are necessary for every future specialization. |
| GC admission API | `canAllocate:` had only test callers. Delete it and its generalized checked preview. The actual allocator only needs to compare accumulated bytes against its threshold; checked addition still protects committed charges. |
| Callback/collection exclusions | Legacy discarded-clone code precomputes the original allocation budget to exclude collection, then journals omitted allocations. Neither mechanism survives the new contract. An effect-free native segment can be short and local; a whole-region reservation protocol is not required by language semantics. |
| Recovery validation | Current recipe reads, allocation and slot writes use replaceable messages. They can collect or mutate execution/storage state. Keep rooting, post-callback validation and atomic publication while those calls exist. Do not remove them merely as defensive code; also do not require a consumer with no recoverable virtual values to use the graph materializer. |
| Hotness, limits and defaults | The 64-visit policy remains private to explicitly installed candidates. Recovery graph limits bound the current implementation. Neither is a language guarantee or a mandatory policy for new producers. The removed emission limits have no remaining purpose. |
| Scalar numeric shortcut | The current `numeric::invoke` path still constructs a result through Integer and adds hotness tracking plus an owner/visitor call. Its exact-entry validation is useful evidence for ownership, but not proof of a speedup or a reason to build numeric chains around it. Its profitability remains to be measured; it is unchanged in this slice. |
| Remaining families | Numeric chains/loops, closures and live aggregates require separate evidence of useful work removed and acceptable cost. Old default settings and timing thresholds do not transfer to the refactored implementation. |

Relevant production paths:
[emitter](../../rust/birnam-codegen/src/native.rs),
[candidate selection](../../rust/birnam-execution/src/native_tier.rs),
[Machine guards](../../rust/birnam-execution/src/machine.rs),
[recovery](../../rust/birnam-execution/src/recovery.rs),
[GC accounting](../../rust/birnam-collect/src/accounting.rs).
No lazy compiler, new guard-description framework or alternate owner ABI replaces
the removed code.

## First allocation-saving consumer: a discarded ordinary clone

Choose a clone whose identity is neither used nor inspectable before it becomes
dead, and whose enclosing computation returns an existing input. Begin with an
ordinary zero-slot object. This isolates allocation removal from field projection,
closure environment substitution, numeric representation and virtual graph recovery.
It is a concrete implementation case, not a claim that discarding a result alone
makes a send pure.

The retained [compiler planner](../../rust/birnam-codegen/src/discarded_clone.rs)
contains useful origin/escape analysis. Its index-box accounting is obsolete.
The [old consumer](../../rust/birnam-runtime/src/runtime/native_tier/discarded_clone.rs)
also requires at least three clones to amortize admission and journal preparation.
That threshold describes the old implementation's costs; do not inherit it.
Payload snapshots, snapshot caching, synthetic byte calculations and journal
capacity checks disappear from the proposed consumer.

The minimum work is:

1. **Prove absence of observable identity.** Use the lowered program and descriptor
   to establish no escape, use, inspectable Frame value or pending result requires
   the clone. Preserve supported stop/capture/control boundaries. Start with one
   eligible site; do not fuse whole frames merely because the old consumer did.
2. **Establish the selected operation's semantics.** The current implementation is
   `clone_object` on Object's behavior, installed by
   [ObjectModel](../../rust/birnam-object-model/src/native.rs), with zero arguments
   and the actual source object as receiver. `clone` and `new` share this entry;
   neither selector by itself authorizes elision.
3. **Account for the nested allocation path.** `clone_object` sends GC
   `prepareAllocation`, determines owned extent/behavior, snapshots the payload,
   and calls `Model::allocate`, which sends the behavior's live `allocate` method.
   A replaced clone, preparation or allocator may have effects. Matching the
   outer clone entry alone is therefore insufficient. ObjectModel must establish
   its own implementation/storage facts; GC owns allocation policy. Execution
   must not inspect their Rust state or capture service functions to prove this.
4. **Skip only the admitted construction.** If admission fails, execute the
   ordinary selected call once. An admitted discarded value needs no Oop, payload
   snapshot, global token or later materialization. Preserve required effects and
   the continuation. Any guard/admission callback requires reconsidering facts it
   could change before the skip is committed.
5. **Validate the actual benefit.** Show fewer physical allocations for this case,
   correct existing-input identity, replaced-method effects exactly once, ordinary
   fallback and supported GC/control behavior. Measure guard/construction cost
   against the saved work before enabling it generally. Do not compare hypothetical
   charges or replay the old threshold/finalizer receipts.

The current owner protocols do not provide an operation that certifies the whole
clone/allocation path for omission. This is the concrete remaining implementation
question, not a reason to restore the retired allocation-budget or identity API.
Any new message must be derived from this consumer, identify the selected entry
and actual receiver, and let the owner establish its own facts. No speculative
selector or placeholder service is installed by this reassessment.

Live aggregates and closures can follow only when a concrete case needs their
additional recovery state. For this first case, the existing fallback/control
path suffices; a generalized deoptimizer is not a prerequisite.

## Validation and limits

The full native-message run passes all 13 production integration binaries and the
export, install-name and dependency audit. The revised
[source dispatch test](../../rust/birnam-siphon/tests/compiler/dispatch.rs) installs
a rejecting `Method fromNative:owner:` replacement and observes zero calls while
compiling/executing small and large functions and captured Blocks. Repeated runs
retain factory effects, changed arithmetic and collecting owner/Machine calls.
Actual TCC Unit and relocated-byte counters return to zero after final release.
Separate native candidate tests still exercise hot selection, replacement,
withdrawal and active lifetime; the compiler test retains real compiled recovery.

The old two-PC probe required two speculative native candidate constructions; the
revised source path requires none. Removing that work is established by the code
and production witness. No wall-clock speedup is inferred from correctness-test
durations. The broader allocation-elision producers and full workspace/CLI remain
unrestored.


## Implemented follow-up (2026-09-24)

The first discarded-clone case above is now implemented and measured; see
[its exact scope, owner messages and evidence](90-discarded-clone.md).
The absence of inspectable identity is established at an existing inline-call
boundary, not by clearing a live Frame slot. Bounded execution declines.
Owner-local semantic queries now cover the actual clone and allocation path;
no retired allocation-budget or deferred-identity protocol was restored.
The other decisions in this reassessment remain in force.


## Numeric chains and loops reviewed (2026-09-24)

The [numeric-chain audit](90-numeric-chain-review.md) retains checked arithmetic
and dataflow, rejects restoration of the legacy boxed fused driver and prediction
registry as prerequisites, and defers cross-iteration result caching. Consecutive
tail-loop stamps are optimizer policy, not language semantics; their unused
production bookkeeping is a cleanup candidate. Examine the first chain together
with unboxed intermediates, resolving real result-allocation/Frame observation
before defining an owner message. No new runtime behavior or API is introduced by
this review.
