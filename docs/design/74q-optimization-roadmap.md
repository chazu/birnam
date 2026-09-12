# 74Q — Optimization roadmap with malleability and recovery

Status: proposed roadmap, 2026-09-12. Documentation only; this document does not
implement or promote another optimization. It follows the user's request for a
plan covering the opportunities beyond 74P, including transformations for which
deoptimization cannot undo an observable change.

Implementation progress: [74R](74r-small-callee-recovery-plans.md) delivers the
first bounded Stage 1 batch (single-PC entry values, local aliases and Return).
[74S](74s-three-optimization-families.md) implements inherited dispatch caching
and guarded native Frame regions, with separate profitability experiments.
Further native optimization and broader continuation shapes remain pending.

## Direction

Build a selective optimizing compiler around ordinary Id semantics. Start by
broadening small-method inlining, then use the resulting visibility to remove
redundant work. Add more demanding representation changes only when the compiler
can establish their identity, escape and allocation obligations.

The eight families below are opportunities, not eight guaranteed wins or an
exhaustive list of compiler passes. Their benefits overlap. A useful optimizer
must also remain cheap for cold code and code that repeatedly refuses optimization.
There is no numerical whole-program speed target implied by this roadmap.

## Starting point and evidence

- [60P](60p-v2-control-completion.md) completes V2 suspension/capture/restore and
  removes Frame layout V1.
- [74K](74k-runtime-owned-native-tier.md),
  [74L](74l-guarded-default-lookup.md) and
  [74N](74n-native-tier-dependency-revalidation.md) supply native PC candidates,
  guarded canonical lookup and dependency revalidation. 74L still performs a
  fresh dictionary read; it is not a call-site target cache.
- [74P](74p-production-recovery-and-leaf-inlining.md) supplies scoped production
  admission for reconstructing one registered callee and a hot projection-call
  consumer. The original caller, pending Call, roots and code ownership survive.
  Its ordinary `Machine recoverAt:with:` operation admits an Id-described recipe
  only within runtime-owned recovery authority. It is not an unrestricted API
  for creating activations or a complete Id-authored optimizer.
- The [74P correctness receipt](../measurements/74p-production-recovery.json)
  records passing debug/release integration tests and all production gates,
  including 458 workspace tests. Its task was explicitly completed without
  qualified timings at the user's request.
- The subsequent [exploratory timing receipt](../measurements/74p-unqualified-timing.json)
  records five pairs of 10,000 hot projection calls: 6.0–8.7% less elapsed time,
  with a 7.7% paired median. Allocations match; controls show no consistent gain.
  These battery-powered results suggest a local benefit, not qualified timing
  acceptance or an application-wide estimate.
- [74H](74h-guarded-frame-read-reuse.md) found no convincing performance benefit
  from isolated guarded read reuse. [74M](74m-native-tier-performance-isolation.md)
  and 74N did not establish an independent native-tier speed benefit and exposed
  preparation overhead. Those negative results constrain the plan: do not assume
  an additional pass pays for its guards or compilation.

The current portable lowering has entry values, literals, global lookup, closure
construction, bindings and Call/TailCall/Return/Continue terminators. Arithmetic
and language control constructs are reached through ordinary sends. A loop pass
or an arithmetic pass therefore needs a derived optimization representation and
proof of the relevant Id operations; there is no ready-made pure arithmetic or
loop IR to optimize today.

## Non-negotiable semantic contract

Keep full malleability and expressibility in Id semantics. A selector's spelling,
a familiar class name or a once-observed implementation never makes an operation
permanently intrinsic. Specialized behavior must be derived from the currently
admitted implementation and guarded dependencies. Unknown implementations use
ordinary execution.

Preserve the public IdApi/Method ABI, canonical descriptor encoding and image
formats. Private compiler/runtime metadata may evolve. Initial candidates use
existing baseline descriptors and resume boundaries. If a later optimization
cannot express its recovery with these constraints, document the specific gap;
do not quietly change an artifact contract to enable it. Images retain canonical
meaning and identities; private executable candidates are rebuilt or discarded.

Arbitrary native methods remain callable. Treat them and unknown Id sends as
possible observers, mutators and reentry points unless there is a sound, guarded
summary of their behavior. Reconstruct observable state before entering them.
Recovery metadata does not make opaque native C activations reconstructible.

This roadmap retains owner-thread execution. It supplies neither concurrency,
transactions nor rollback of external effects.

### What deoptimization can and cannot repair

Deoptimization resumes the logical computation already in progress. It does not
restart a method, repeat argument evaluation or undo a committed effect.

| Transformation | Required justification |
| --- | --- |
| Omit a physical callee Frame | Restore every observable logical Frame and its live state before observation. |
| Omit a private, dead calculation or redundant copy | Prove it has no observable effects and no live result requiring recovery. No inverse operation is necessary. |
| Keep a live value in a different representation | Supply its exact required value, identity and aliases at recovery/observation boundaries. |
| Delay construction of an unobserved object | Prove absence of relevant allocation/GC/finalizer observations and materialize shared identity before exposure. Non-escape alone is insufficient. |
| Skip or repeat an effectful send, write or I/O operation | Invalid unless independent semantic analysis establishes equivalence. Recovery cannot supply that proof. |
| Replace an escaped object with an equal-valued object | Invalid when identity or aliases differ. Retain the original object. |
| Move allocation past an observable collection/finalizer checkpoint | Invalid when the checkpoint's observations change, even if later recovery constructs the object. |

[74O](74o-deoptimization-architectural-fit.md) provides actual identity and
finalizer counterexamples. Every new optimization must identify which row it
relies on. Do not classify all operations as either pure or impure and assume
that classification is enough: reads, writes, allocation, identity exposure,
lookup, reentry, suspension and native observation require distinct treatment.

## Opportunity map and proposed order

| Family | First useful scope | Prerequisites / stage |
| --- | --- | --- |
| Broader method inlining | One small callee, then bounded nested calls | Compiler-generated recovery plans; stages 1–2 |
| Specialized dispatch | One observed receiver/target, then a small bounded set | Complete lookup dependencies, guarded effect summaries and lifetime protection; stage 2 |
| Redundant reads and computations | Reuse values inside an observation-free region | Alias/write analysis and mutation/GC boundaries; stage 3 |
| Conventional native optimization | Copies, dead pure values, constants and register use | Explicit live-state reconstruction obligations; stage 3, then ongoing |
| Unboxed arithmetic | One proven numeric operation chain | Current Id operation meaning, exact arithmetic/error rules and identity-aware recovery; stage 4A |
| Loop optimization | A small loop with proven invariant work | Recognized Id control behavior, alias/effect analysis and bounded recovery points; stage 4B |
| Temporary-object elimination | One acyclic, unescaped aggregate without observable allocation effects | Escape/identity analysis and materialization recipes; stage 5 |
| Closure/capture elimination | One immediately used, non-escaping closure | Inlining plus environment/alias analysis and allocation eligibility; stage 5 |

Stages 4A and 4B are independent choices after their prerequisites. Stage 5 can
reuse their analysis but does not require every loop or arithmetic optimization
to be implemented. Select individual passes by profiles, not by completing this
table mechanically.

## Stage 0 — Keep measurements and optimization overhead honest

Establish separate counters for eligibility, actual optimized execution, rejection
reasons, recovery reasons, generated bytes and candidate preparation/compilation.
Keep instrumentation equally enabled in paired variants and distinguish its cost
from normal production execution.

Measure cold startup, warmup, steady execution, compilation time, memory, metadata
size and repeated invalidation. Include three kinds of program: a fixture that
exercises the intended pass, an ineligible control, and an application workload
selected from an actual profile. Never treat the projection fixture as coverage
of general Birnam execution.

Revisit deferred candidate preparation as a separate bounded change: retain the
minimum safe admission facts for cold code and prepare expensive candidates only
when hotness justifies it. Account for retained input bytes and eviction. Keep it
separate from inlining measurements so one improvement cannot hide another cost.

Deliverable: a reusable measurement protocol and baseline receipts. Qualified
AC-powered timing remains a performance follow-up; it does not reopen completed
74P correctness work or prevent designing and testing the next slice. Distinguish
implementation completion from evidence supporting a performance claim.

## Stage 1 — Compiler-generated recovery for one small callee

Extend the current projection matcher into a small, bounded compiler pass over
admitted portable bodies. First support entry values and ordinary local aliases
(`Bind`) ending in Return. This is a deliberate first increment using existing
IR, rather than claiming arithmetic or arbitrary sends are already inlineable.

Generate the live-value mapping from lowering and slot coloring. Preserve baseline
housekeeping allocation and reflective scratch-action behavior, as 74P does.
Recover or decline before any source allocation, unknown send, global lookup,
closure construction, callback or other observation requiring the callee Frame.
A harmless-looking alias must still respect overridden Machine operations.

Then extend eligible regions only at existing baseline resume boundaries. A PC
number alone does not establish a legal recovery point: the required live slots,
root-state PC and pending transition must all agree. If recovery would resume
inside a baseline block and repeat effects, decline that region. Do not use
method restart as a substitute for a missing continuation.

Deliverables:

- A compiler-produced, process-local recovery plan consumed by the production
  admission engine, including explicit refusal reasons for unsupported state.
- A positive alias-return case beyond 74P and negative cases that expose a Frame
  or mutate microcode. Baseline and optimized execution must preserve object
  identity, allocation behavior and exactly-once effects.
- A bounded code-size/region limit and isolated measurements of the incremental
  change against 74P. A correct but unprofitable matcher is not a reason to keep
  broadening its default coverage.

This is the recommended next implementation batch. General nested inlining,
call-site target caching and source-object allocation elimination are excluded.

## Stage 2 — Nested inlining, richer recovery and dispatch specialization

Before eliminating a call chain, extend recovery to an entire logical chain with
all-or-nothing validation and publication. The current one-child operation must
not simply be called repeatedly if a later failure can leave partial state.
Retain existing visible Frame identities; reconstruct only absent Frames.

Add a private optimized IR that carries baseline provenance and distinguishes
semantic operations from removable implementation work. Initially derive it from
canonical lowering; canonical artifacts remain the recovery authority. Start with
one nested call and explicit maximum depth, not unrestricted recursive inlining.

For each admitted exit, describe:

| Fact | Why it is needed |
| --- | --- |
| Exact implementation/generation and baseline descriptor for each activation | Resume the acquired code and preserve its lifetime. |
| Logical caller relationships, resume PCs, root-state PCs and pending transitions | Continue at the correct point without replaying effects. |
| Sources of live values and shared aliases | Restore all required slots without manufacturing distinct identities. |
| Oop roots and any native spill locations across a safe point | Keep values alive before and throughout reconstruction. |
| Result destinations and continuation/delimiter relationships | Preserve return, suspension, capture and restore behavior. |
| Assumption dependencies and the next observation boundary | Exit before an invalid assumption affects observable behavior. |

Treat unfamiliar exit-state shapes as ineligible until tested. In particular,
mid-body same-Frame recovery, a chain of absent callees and detached continuation
state are different cases; 74P is not proof of all three in combination.

Develop call-site specialization as a separately measurable consumer. Begin with
one receiver shape and exact active implementation, retaining ordinary lookup on
miss. A small polymorphic set comes only after this works. Guard lookup behavior,
dictionary/delegation dependencies and every supported mutation path; receiver
shape alone is insufficient. Unknown or effectful lookup must still run normally.
Preserve code acquisitions through argument evaluation, reentry and replacement.
This stage explicitly proposes the new target-cache contract; 74L does not already
authorize skipping dictionary reads.

Deliverables: two-level reconstruction, rejection without partial publication,
redefinition during argument evaluation and nested execution, native inspection,
precise GC, V2 suspend/capture/restore and actual generation retirement. Show that
Id-defined code remains callable and replaceable throughout.

## Stage 3 — Remove redundant work exposed by inlining

Build conservative effect and alias summaries in the derived IR. Default unknown
operations to ordinary observation/mutation boundaries. Any summary of an Id
implementation is conditional on that implementation's dependencies remaining
valid; never infer purity from a selector name.

Start with local copies, dead pure values and repeated computations whose inputs
cannot change. Then consider repeated Frame/object reads and dead stores, with
proof that no alias or observer can see a difference. Recovery/debug-visible
values are live even when the optimized computation itself no longer needs them.
GC and finalizer callbacks invalidate assumptions independently of dictionary
mutation; an epoch covering method changes is not a heap-value version.

Use native register allocation and instruction optimization within these regions.
Retain baseline object allocation initially. A specialized arithmetic send may
still create the normal result box; optimizing computation and eliminating that
box are separate changes. This stage must preserve error behavior, overflow rules,
evaluation order and the current Id definition of the operation.

Deliverable: one profiled redundant-work case with an isolated win, controls and
boundary-mutation tests. Revisit 74H only where a larger region amortizes guards or
a profile identifies sufficient repeated work. If it remains inconclusive, leave
that pass out rather than weakening its guards.

## Stage 4A — Numeric representation specialization

Introduce raw numeric values only for a proven chain of operations. Define exact
integer overflow, conversion, floating-point, error and operator-redefinition
behavior before native lowering. Avoid reassociation or other native arithmetic
shortcuts unless they preserve the selected semantics.

Distinguish existing boxed operands from newly computed virtual results. Existing
Oop identity must remain available wherever required. Every recovery point needs
an identity-aware materialization recipe; equal numeric values do not imply the
same object, and aliases must not become separate boxes.

Begin with no opaque calls inside the raw-value region. Keep results boxed where
identity, reflective enumeration, allocation or finalizer behavior cannot be
proved equivalent. Retaining boxes while accelerating arithmetic remains valid.

Deliverable: one numeric kernel with exact result/error/identity comparisons,
forced exits at every admitted point and separate accounting for arithmetic and
allocation gains. General numeric unboxing is not the first deliverable.

## Stage 4B — Loop optimization

Recognize loop structure only after establishing the relevant Id control-flow
semantics. First move one proven invariant computation or redundant guard out of
a bounded region. Keep ordinary operations at any boundary where mutation,
reentry, observation or suspension invalidates that proof.

Long-running optimized loops require bounded safepoints with complete recovery
and root information. Specify their maximum work interval and test suspension,
capture and mutation responsiveness. Do not add an observable per-iteration poll
or move effects merely to simplify implementation. Large uninterrupted native
regions must not compromise the stackless runtime's control contract.

Deliverable: one representative loop, recovery before and after relevant effects,
zero duplicated work with external consequences, and measured guard/loop overhead.
Vectorization or aggressive unrolling are later profile-driven choices, not
prerequisites for this stage.

## Stage 5 — Virtual objects, closures and captures

Implement escape and identity analysis, including exposure through reflection,
unknown lookup, native calls, closures, images, weak references and GC hooks where
those mechanisms apply. Analyze allocation observations separately from escape.

Start with one acyclic virtual aggregate containing existing Oops and proven
private values. Reconstruct its graph once per identity before exposure and root
all materialized nodes throughout construction. Validate the recipe and retain
inputs before allocations that may collect. Keep this potentially allocating
materialization phase separate from the atomic Frame publication phase.

Then consider a directly invoked, non-escaping closure whose environment can be
substituted into an inlined body. Mutable captured Cells must retain their shared
identity and update behavior; initially keep those Cells allocated or refuse the
case. Closure elimination is not permission to turn shared captures into copies.

Initially exclude cycles, finalizer-bearing objects, opaque/raw storage and
already exposed identities. Each requires an additional explicit contract and
counterexample tests before inclusion. Refuse allocation elimination whenever
its required observations cannot be preserved.

Deliverable: one aggregate and one closure case with allocation savings, alias
identity checks, forced materialization, GC during materialization, and fallback
through native observation and continuation capture. No claim of general scalar
replacement follows from those two cases.

## Shared ownership, Id policy and resource limits

Compiler policy owns eligibility, cost decisions, effects/alias analysis and
recovery recipes. These concepts must be representable using ordinary Id data
and semantics. A native implementation is an initial implementation choice, not
a new privileged language definition. Test an Id-authored producer for each new
recipe category rather than assuming the existing dense slot-vector test proves
all future representations expressible.

The runtime owns checked admission, roots, generation leases, safe transfer and
resumption. Keep validation separate from policy. Hold acquisitions while a
candidate is being prepared or executing; binding removal or cache eviction does
not authorize unloading active code. Preserve old roots and ownership until
complete replacement state has been installed.

Bound inlining depth, region size, polymorphic variants, compilation work,
metadata memory and repeated invalidation. Use backoff or baseline execution for
unstable sites. Compiler failure, unsupported state, stale metadata and exhausted
optimization budgets must withdraw optimization safely. No new pass may make
ordinary native participation contingent on recovery metadata.

## Acceptance for each implementation slice

Record a concrete source workload, the exact removed work, guarded assumptions,
recovery points and forbidden cases before coding. Compare against the same
canonical program with only the proposed pass disabled, and separately against
the ordinary baseline. Interacting passes need individual and combined results.

Correctness evidence must cover returned values, identities/aliases, ordered
effects, errors, mutation during argument evaluation and callbacks, GC/finalizers,
Frame inspection, suspension/capture/restore and native-code retirement as
applicable. Force each admitted exit and malformed/stale metadata rejection.
Pass debug/release tests, compatibility authority checks and the full production
gate; record source and binary hashes. A focused passing test does not replace
installed-prefix and image/lifetime checks.

Performance evidence must report actual hits, misses and recoveries alongside
cold/warm timing, allocations, retained memory, metadata/code size and compilation
cost. Use paired ordering and independent runs, including ineligible and churn
controls. Report variation; do not sum overlapping speedups. Qualified claims
require the repository host gate. Exploratory measurements remain explicitly
labelled, including any user-authorized completion without qualified timing.

Retain a pass when it provides supported benefit for its intended workload without
unacceptable regressions or semantic compromises. An inconclusive result can
justify a narrower candidate or deferral. A semantic counterexample requires a
stronger eligibility condition or rejection, not more favorable benchmarking.

## Reference and scope of precedent

The [Self implementation work](https://bibliography.selflanguage.org/implementation.html)
demonstrates receiver specialization and inlining together with dynamic
invalidation and reconstructed Frames. Its
[dynamic deoptimization work](https://bibliography.selflanguage.org/dynamic-deoptimization.html)
describes recovery information at defined interrupt points. These are precedents
for the approach, not evidence that Birnam already has their machinery or will
reproduce their performance. Birnam's current contracts and counterexamples above
remain the acceptance authority.
