# Design 90 — Numeric chains and loop reuse reassessment

2026-09-24, audited against `8797a374`. This examines the next family after the
[first discarded-clone consumer](90-discarded-clone.md), under the
[revised optimization contract](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
It changes the implementation queue, not runtime behavior.

## Decision

Keep the small checked-arithmetic and dataflow algorithms. Do not restore the
legacy prediction/ticket registry, boxed fused driver or cross-iteration cache
as prerequisites. Examine a concrete arithmetic chain together with unboxed
intermediates: these are overlapping opportunities, not two mandatory successive
ports. Leave loop-result caching deferred until a current consumer demonstrates
that arithmetic itself costs enough to justify comparing and retaining results.

## What the retained code actually does

| Part | Actual work | Decision |
| --- | --- | --- |
| [Template](../../rust/birnam-codegen/src/numeric_chain/template.rs) | Follows lowered operands, aliases and literal occurrences through straight-line unary-argument sends. Selectors identify sends, not arithmetic meanings. Also counts every old index, PC and result box. | Reuse the dataflow. Separate it from obsolete `step_boxes`, `remaining_boxes` and `body_boxes` duties when adapting a consumer. |
| [Plan and emitter](../../rust/birnam-codegen/src/numeric_chain.rs) | Checked i64 arithmetic in source order; backward result dependencies; transitive input masks; a raw C kernel and optional changed-input kernel. | Reusable algorithms. No new compiler service is required by the algorithm. Native compilation, if worthwhile, belongs with existing Codegen/TCC ownership. |
| [Prediction machinery](../../rust/birnam-runtime/src/runtime/numeric/chain.rs) | Reads inputs, obtains legacy numeric declarations, compiles a separate kernel, pools vectors, retains compiled bindings and constructs prediction tickets. Its per-call path is disabled in ordinary builds, explicitly because validation costs more than the arithmetic replaced. | Do not port as the starting architecture. It reads foreign Integer storage through the old aggregate Runtime and relies on retired declaration/compiler integration. |
| [Boxed fused driver](../../rust/birnam-runtime/src/runtime/native_tier/fused_numeric.rs) | Predicts results, then invokes original PC entries, decodes actions and performs transitions while substituting arithmetic results. Every original box remains. A whole-body allocation-budget check excludes collection/observers. | Do not restore this second driver or its budget protocol. Dispatch savings are plausible, but this code does not establish a benefit in the current system. |
| [Loop cache](../../rust/birnam-runtime/src/runtime/numeric/chain/loop_reuse.rs) | Compares current and previous raw inputs, copies previous results, recomputes changed dependencies, and updates retained vectors. Requires consecutive self-tail iteration stamps. | Defer. Reusing predictions is not the same as removing the surrounding messages, allocations and transitions. |
| [Recovery maps](../../rust/birnam-codegen/src/numeric_chain/recovery_maps.rs) | Tracks which physical slots alias each intermediate result at action/transition boundaries. | Useful if an actual virtual numeric value must become inspectable. Do not require all these maps for a case that never exposes such state. |

The production [native emitter](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-native-backend/src/native.rs) does not
install these legacy numeric-chain consumers. Production Execution has the
separate [scalar shortcut](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-execution/src/numeric.rs); this review
does not establish that shortcut's profitability or change it.

## Requirements versus inherited policy

**Arithmetic semantics and late binding remain necessary.** Establish the actual
selected native implementation and owner context, read numeric values through
their owner, and retain first-error order. A raw kernel's refusal is not a Birnam
Error. Before any observable effect, it can fall back to ordinary execution;
after effects, recovery cannot replay them. Replaced result factories and mixed
operands must retain their behavior. Reading raw values once does not authorize
using them across a callback that can mutate the inputs.

**Consecutive-tail stamps are not an arithmetic correctness requirement.** Once
a pure operation's current meaning and raw inputs are established, its numeric
answer does not depend on whether the previous evaluation occurred in this loop
or in an independent call. Restricting reuse to a loop can be a bounded caching
policy, but proving that policy is not a language invariant. This does not permit
reusing observable result objects: equal numeric bits and object identity are
separate concerns.

The production Frame engine still maintains `self_tail_entry` during ordinary and
planned tail replacement and clears it on push/restore. Its
[stamp reader](../../rust/birnam-execution/src/execution/loop_iteration.rs) has no
current production consumer; the non-test caller is in the deferred Runtime
chain code. This is a concrete cleanup candidate. Remove the optimizer-only flag,
stamp reader and associated policy tests when cleaning up the rejected cache
integration. Keep Frame generations, descriptor/code ownership and stale-view
checks: those have independent execution purposes. No such cleanup is performed
by this review.

**Collection exclusion was serving a real need in the old driver, but its chosen
mechanism is not a prerequisite.** That driver bypasses calls while executing
baseline allocations. A collecting allocation could expose Frame state or change
later methods, so blindly deleting its budget check would be incorrect. Replace
the architecture rather than pretending the old driver is safe without the check:
use a bounded pure segment, ordinary fallback, and actual recovery where a
supported observer can run. Do not restore heap-budget prediction to avoid this
design decision.

**Numeric chains differ from the delivered clone case.** The clone consumer
returns an already-rooted argument without allocating after its final verdict.
An arithmetic chain normally returns a newly constructed Integer. Result creation
can collect or call replaceable methods. Before such an observation, required
logical Frame values and aliases must be available, or the candidate must decline
before consuming effects. Bounded execution can remain baseline-only initially,
but that alone does not settle callback/collection observation in an unbounded run.
Materialize only values needed by actual execution or supported inspection, never
dead boxes to reproduce allocation history.

## Existing seam and the next concrete candidate

Start investigation with an uncaptured body equivalent to:

```birnam
[:a :b :c | [[a + b] * c]]
```

The opportunity is to remove the intermediate Integer and repeated Frame/dispatch
work while returning a normal final Integer. This is a candidate, not an admitted
implementation or performance promise. Named methods, captured Cells, general
loops and mixed numbers need not be supported in its first implementation.

Today Execution sends the current Integer receiver
`evaluate:receiver:argument:with:` with a selected NativeEntry, left Object,
right Object and borrowed evaluator NativeEntry. Integer checks its own entry,
reads its own operands, invokes the evaluator with native Words for operation and
values, then uses its current `fromWord:` factory. This is a single-operation
boxed-result contract; chaining it unchanged still constructs intermediate boxes.
The existing `canOmitIndexing` query certifies index construction, not arbitrary
arithmetic or numeric recovery.

A concrete chain therefore still needs an Integer-owned way to establish the
selected operations and handle raw intermediates, plus a settled final-result
observation/recovery boundary. Derive that message from this consumer before
implementation. Do not install a placeholder selector, export a numeric table,
let Execution read Integer payloads, cache foreign functions as semantic authority,
or move a native compiler into Execution. No new API signature is approved here.
If only a pair of checked operations is needed initially, the existing pure
operator evaluator is sufficient to prototype the computation; a separate JIT
kernel, hotness registry, reusable buffer pool and cross-call cache are not
prerequisites.

Measure baseline versus the complete candidate, including guards, result creation,
recovery and construction cost. Show actual messages/allocations removed, with
replacement, mutation, errors, aliases and observation tests. A reused-operation
counter or a faster isolated arithmetic kernel is insufficient.

## Evidence and limits

The [old loop report](../archive/design/74w-loop-invariant-numeric-work.md#exploratory-unchanged-input-fast-path-comparison)
ends without a repeatable net gain. Its completed exploratory measurements have
mixed signs and explicitly unqualified host conditions. They concern the retired
implementation and are not current performance evidence or thresholds to inherit.
No current whole-chain or loop speedup is asserted by this review.

Current focused checks pass: nine Codegen numeric-chain/dataflow/recovery-map tests
and one Execution loop-stamp test. The latter confirms the retained policy works,
not that the policy is needed:

```sh
mise exec -- cargo test --offline -p birnam-codegen --lib numeric_chain
mise exec -- cargo test --offline -p birnam-execution --lib loop_iteration
```

The old native-kernel differential and integration fixtures were inspected but
not run: they belong to the deferred Runtime harness. These focused checks do not
prove production numeric fusion, unboxed recovery or loop profitability.


## Implemented first slice

The [numeric-predicate consumer](90-numeric-predicate.md) combines a checked
arithmetic operation and comparison with an unboxed intermediate. Its existing
Boolean result avoids the final allocation in the illustrative multiplication
chain above. Execution commits through a synchronous Integer-owned visitor before
cleanup callbacks, retaining the existing inline boundary. General numeric-result
construction/recovery and loop caching remain pending; neither is implied complete
by this bounded implementation.
