# 74T — Numeric, loop, aggregate and closure optimization

Status: implementation in progress. This document tracks the user's request to
implement all four remaining families in [74Q](74q-optimization-roadmap.md).
Recovery infrastructure alone does not complete a family. The outstanding 74S
application timing gate remains separate.

## Contract

Preserve ordinary Id meaning, mutable method lookup, object identity, shared
Cells, observable Frames, stackless control and active native-code ownership.
No selector or prototype name grants intrinsic authority. The public IdApi,
Method ABI, canonical descriptors and image formats remain unchanged.

Compiler eligibility and recovery descriptions must be producible using
ordinary Id objects. Runtime admission validates storage, roots, authority and
lifetime; it does not turn a producer's semantic assertion into trusted proof.
An optimizer must independently establish that the operation being replaced is
the operation for which it has a guarded summary.

The existing code establishes two constraints that govern implementation:

- `birnam-prelude::numeric_binary` uses signed 128-bit intermediates for integer
  addition, subtraction and multiplication, then checks the signed 64-bit range.
  Division also checks zero and range; mixed arithmetic converts to floating
  point. Error operands and operator redefinition remain ordinary behavior.
- The floor exposes live allocation counts and triggers automatic collection
  using allocated bytes. Non-escape alone cannot authorize allocation removal.
  Removing an allocation must preserve observations at subsequent boundaries,
  including GC timing and finalizers; suppressing collection is not a proof.

## Implementation sequence and completion evidence

| Work | Required production behavior | Evidence required before completion |
| --- | --- | --- |
| Shared recovery | Validated acyclic identity graphs, existing Oop inputs, exact numeric bits, one materialization per virtual identity; roots survive collecting materialization and publication stays atomic | Invalid and stale recipes; repeated aliases versus equal distinct objects; GC between construction steps; Id-authored recipes; capture/restore and native retirement |
| Unboxed arithmetic | Derive numeric chains from current admitted implementations; carry raw intermediate values with exact overflow/error rules and materialize at observation boundaries | Integer extrema, zero division, mixed numeric/error operands, changed operators and lookup, forced exits at every supported point; separate arithmetic and allocation accounting |
| Loop optimization | Recognize actual Id control behavior; remove invariant work within bounded execution intervals, preserving ordinary mutation and stackless transition boundaries | Representative loop, effects once and in order, invariant invalidation, suspension/capture responsiveness, guard overhead and profitable execution |
| Temporary-object elimination | Derive escape, identity and allocation-observation proofs for acyclic aggregates; scalar-replace private fields and materialize before exposure | Actual allocation savings, shared graph identity, reflective/native escape, collection during materialization, finalizer/collection threshold counterexamples |
| Closure/capture elimination | Derive a directly invoked private closure and substitute its environment into the admitted body; retain shared mutable Cells | Actual closure allocation savings, multiple uses and escaping fallbacks, shared Cell updates, method replacement, native observation and continuation capture |

These are implementation obligations, not a claim that every source program is
optimizable. Unsupported or unstable cases execute canonical code. Broader shapes
must use the same analysis and recovery contracts; passing a synthetic kernel
does not stand in for an installed compiler consumer.

## Recovery graph

The compiler representation separates input identities from newly constructed
identities. A value reference selects an existing input or an earlier graph node.
Every node denotes one fresh identity even when two nodes have equal numeric
values. Multiple references to one node must resolve to the same Oop. Nodes are
ordered before their users, so cycles and forward references are rejected before
Birnam allocation. Graphs explicitly describe every destination Frame slot.

The first implemented node shapes are exact signed integers, exact floating-point bits and
all-Oop aggregates with an existing vtable input. Existing objects, including
captured Cells, are retained as inputs. Raw storage, finalizers and trace hooks
require additional contracts and are not inferred from a producer's description.
Closure-specific construction must also retain its registered implementation and
capture layout; an arbitrary aggregate is not automatically a valid closure.

Structural validation is pure compiler code with fixed node, input, slot and
field limits. Runtime validation checks the actual heap and layout before
materialization. All inputs and constructed nodes stay precisely rooted until
the complete graph has been installed or rejected. Allocating construction and
callback-free Frame publication are separate phases. Recovery authority is
revalidated after any phase that can run foreign code.

`Machine recoverAt: pc withGraph: recipe` uses the same scoped, one-shot
registered-call authority as `recoverAt:with:`. Outside such a request it returns
nil. The recipe is `[inputs, nodes, slots]`; these are ordinary zero-based
`size`/`at:` objects, not a frozen binary encoding or new IdApi entry. Nodes are
`[0, integer]`, `[1, float]`, or `[2, vtableInputIndex, fields]`. Nonnegative value
references select inputs. Negative references select node `-reference - 1`.
For example, two `-1` references share node zero; two separate integer nodes
remain distinct even when their values are equal.

The decoder snapshots and roots every returned carrier through ordinary sends,
then validates the whole graph before construction. Limits are 128 inputs,
64 nodes, 128 destination slots and 512 aggregate fields in total. Runtime
checks require actual heap inputs and declared Oop fields with no custom trace
hook. Even an empty aggregate cannot impersonate a raw numeric carrier. Layout
and scoped authority are rechecked after collection before each construction;
publication uses the existing acquired-call recovery check and pin transfer.

This operation prepares new identities explicitly requested by a producer. It
does not authorize eliding any source allocation, replacing an escaped identity,
or using an arbitrary aggregate in place of a registered closure.

## Verification and measurements

For each family retain a canonical source fixture, pass-off and pass-on modes,
forced recovery points and ineligible/mutation controls. Run debug and release
differential checks over values, identities, errors, ordered effects and resource
lifetime. Complete the source-build, installed-prefix, image, cold archive and
relocation gates before calling the production change verified.

Record hits, misses, recovery counts, allocations, retained memory, code/metadata
size and preparation cost, plus paired cold/warm/steady timings. Host qualification
uses the existing thresholds. A noisy host can delay qualified timing but does
not justify dropping implementation or correctness obligations.

## Numeric implementation preparation

Prelude now supplies an [optional native declaration](../../abi/native-numeric-semantics-v1.md)
for its exact add, subtract, multiply, divide and remainder implementations.
The declaration is an unsafe native-module contract, comparable to native Method
and trace-hook contracts. Arbitrary Id annotations cannot attest opaque native
code. The floor queries the Method's actual resident image, checks that the
declaration is in that same image, and retains a loader reference for its lifetime.
There is no IdApi field or Method calling-convention change, and modules without
declarations continue normally.

The declaration also checks the initialized floor's Integer identity and the
current API functions used for type checks, payload reads, ErrorVt imports and
boxing. Each use revalidates this context. An instrumented/copied API table can
withdraw optimization; restoring its functions can restore eligibility. This
prevents silently bypassing a native boxing or import wrapper.

The first production consumer specializes a successful boxed numeric call inside
the existing stackless synchronous boundary. It retains every ordinary lookup,
Frame transition and result allocation. Both actual operands must have canonical
Integer storage. The ErrorVt symbol must already exist so its first interning is
not omitted. Current ErrorVt rebinding preserves the original argument identity;
overflow, zero division and other operands invoke the original Method. Each
method becomes eligible for declaration discovery after 64 calls, with at most
256 cached method records. A selected summary retains its image through GC even
if its record is evicted.

`numeric_chain::Plan` separately compiles bounded raw integer dataflow: up to
64 inputs and 32 operations. The C kernel retains all intermediate raw results
and returns the first failing operation without allocating or invoking Id.
Checked range predicates avoid C signed-overflow and division undefined behavior,
including MIN remainder -1. Differential tests compile 96 real TinyCC kernels
and compare 49,152 executions with the Prelude's i128 arithmetic rule, checking
partial results and surrounding canaries.

### Production raw-chain integration and current acceptance

`numeric_chain::Template` derives straight-line call chains from canonical
lowering, retaining physical inputs, alias dataflow, separate literal occurrences,
PCs and allocation accounting. Hot descriptor records can own one compiled raw
kernel. Unsupported source operations and invalid or excessive metadata refuse
admission. Future target predictions use already-hot native declarations; merely
predicting a call does not count toward the declaration discovery threshold.

The driver integration computes the raw chain once and carries its raw results
between ordinary calls. Each call still performs its actual dispatch and checks
both actual canonical Integer operands, current ErrorVt behavior and the actual
native implementation declaration before using a predicted result. Mismatches
use the existing boxed specialization or original Method. A prediction is only
arithmetic data: it cannot authorize a target, skip an effect, or replace an
object identity. This is sufficient even if an observing lookup, overridden
Machine publication method, or collector changes execution between operations.

Every baseline Frame, index box, numeric result box and transition remains in
place. Result allocation still performs its original GC check after reading
operands. Tickets contain raw integers and owned compiler/declaration metadata,
with no borrowed Frame words or unrooted Oops. The current cache refinement
retains these owners through the drive, including a collecting cache eviction,
and releases the final active owners when the drive ends.

Cached chains now reuse their original binding predictions and pinned native
declarations. They do not repeat dictionary lookup or consult the numeric-method
registry for an actual target that matches a cached binding. Every use still
queries the declaration's current floor/API context and validates both actual
operands and ErrorVt behavior. A stale binding takes the existing method-registry
path; a stale numeric prediction recomputes the current operation normally.
This removes repeated preparation without weakening any observation guard.

A per-runtime pool retains at most four free pairs of temporary raw buffers.
Each active drive leases distinct storage and owns the pool independently of
the caches. Nested drives cannot overwrite an outer drive's intermediates;
clearing the registry releases an old pool after its final lease ends. Raw
future results stay in untraced host buffers rather than native stack arrays,
where conservative collection could mistake their bits for Oop roots.

This replaces the initially proposed requirement for an observation-free whole
chain with independent validation at every actual call. A future faster mode
may amortize those checks only after proving a bounded interval has no intervening
observations, accounting for every allocation and guarding all Machine control,
lookup and implementation dependencies. The fully checked integration supplies
the differential reference for that work.

The real stackless-driver tests enable the integration and verify positive raw
hits. They force changed staging operands, operator rebinding, ErrorVt rebinding,
and collecting cache eviction at each of three operations, comparing ordered
observations, values, identities and allocation bytes against disabled arithmetic
specialization. Normal installed builds currently keep raw-chain prediction
**disabled**: the initial diagnostic pilot showed overhead, not a speedup. The
previous guarded boxed-call optimization remains enabled. The installed Prelude
fixture additionally exercises division overflow, zero division/remainder and
MIN remainder -1 through hot portable bodies; those checks do not by themselves
prove raw-chain hits in an installed build.

The ignored `production_numeric_chain_timing` test compares raw chains with the
existing boxed-call optimization (not with all optimization disabled), alternating
order for three- and sixteen-operation chains. Its initial four-pair pilot used
2,000 invocations per sample. Allocation bytes matched exactly and raw hits
covered every call. It is diagnostic evidence only: preflight passed, but no
interval monitor qualified the measurement. The overhead motivates reducing
admission/validation cost before promotion, rather than enabling a slower pass.

The [follow-up diagnostic receipt](../measurements/74t-numeric-chain-pilot.json)
binds the measured release binary to the correctness receipt below. Median paired
overhead was 9.53% for three operations and 7.14% for sixteen. This remains an
unqualified warm constant-input pilot of the initial integration, not a
whole-application performance result or a measurement of the cache refinement.

The timing fixture also accepts `BENCH_NUMERIC_BATCHES` (1–500),
`BENCH_NUMERIC_OPERATOR` (`add` or `divide`) and `BENCH_NUMERIC_VARYING=1`.
Each batch measures 2,000 invocations, with collection outside its timed interval.
Multiple batches lengthen the sample without silently changing its allocation
or collection regime. Varying-input cases cycle through 1–32; all predicted-call
hits and allocation totals are checked. Automatic-GC and whole-application
profitability remain separate acceptance obligations.

The cache refinement's [initial longer matrix](../measurements/74t-numeric-chain-cache-timings.json)
completed the constant-input addition case under the unchanged host gate:
median regressions were 3.30% for three operations and 1.26% for sixteen, using
100,000 invocations per sample. The next case failed interval admission (47.7%
median idle against the required 50%), so the full matrix is **not qualified**.
These results do not justify enabling the pass. The reusable timing controller
is `scripts/numeric-chain-timing.py --receipt <passed-correctness-receipt>
--output <new-timing-receipt>`; it verifies source and release-binary hashes,
waits for unchanged host admission, monitors intervals and records partial cases.

The [complete follow-up matrix](../measurements/74t-numeric-chain-cache-rk-timings.json)
passed every preflight and interval gate. It used four alternating-order pairs
of 100,000 invocations for each row/length below. Positive percentages mean a
speedup over the existing boxed numeric consumer; negative values mean overhead.

| Operation and inputs | Three operations | Sixteen operations |
| --- | ---: | ---: |
| Addition, constant | +1.32% | -0.54% |
| Addition, varying | -3.56% | -0.91% |
| Division, constant | +2.08% | -1.72% |
| Division, varying | -4.19% | -1.06% |

All allocations matched and every enabled numeric call used its raw result.
The user-authorized `rk` pause ended with verified resumption. These are
qualified warm arithmetic measurements, with GC outside the timed batches;
they do not establish cold amortization or application profitability. The
changing-input regressions keep this implementation disabled in normal builds.

### Bounded driver fusion (production consumer)

The production consumer executes the admitted private PC entries as one bounded
numeric region, retaining their ordinary Frame writes and all original boxes.
It commits numeric call results through the existing V2 transition methods
locally, avoiding repeated operand copying, lookup and driver dispatch between
arithmetic operations. Explicit returns and terminal numeric tail calls must
both reach the existing outer `finish_stackless_return`; a tail-call PC cannot
be relabeled as a Return PC in its descriptor.

Admission precedes the first body effect and checks all of the following:

- The current Frame/descriptor and every private PC entry match the canonical
  lowering. The straight-line region remains bounded to 32 numeric sends.
- Machine slot and control methods and metacircular lookup are their current
  canonical floor implementations, with no resumable registration for a method
  the region bypasses. Every numeric target matches its live pinned declaration
  and initialized API context; ErrorVt does not alias Integer.
- All selectors already exist and the full region's allocation budget fits
  before the next automatic collection. Count all PC tokens, literal/index
  boxes, publication operands and numeric result boxes; only the entry PC token
  has already been allocated when native selection runs.
- There are no pending stackless code retirements. Dropping an ordinary
  dispatch-read guard calls `drain_pending_stackless_retirements`, which can
  collect and invoke observers. An arithmetic-only effect summary is therefore
  insufficient to remove that dispatch boundary.
- The complete raw kernel succeeds. Faulting arithmetic, unknown semantics,
  insufficient room or any failed guard executes the ordinary first PC, before
  speculative work has published a Frame change or allocated a Birnam object.

This requires no public IdApi, Method ABI, descriptor or image-format change.
The existing fully checked driver integration remains the differential reference
for forced refusal, allocation order, Frame state, malleability and ownership.
The consumer reuses the already-enabled guarded Frame regions inside each PC;
bypassing those regions would make the comparison worse than the existing
production baseline. Synchronous boundary depth and high-water accounting remain
identical to ordinary numeric calls. Numeric preparation requires 64 admitted
chain starts, independently of how many PCs each body contains.

End-to-end tests compare explicit Return and terminal TailCall bodies under
replacement of each of the four Machine control methods, observing lookup,
saturated mutation epochs, operator replacement, ErrorVt aliasing and native
declaration withdrawal. Actual automatic-collection tests place the threshold
before, inside and after the region and mutate an input during collection.
Arithmetic-fault tests retain the original sequence of later observing sends.
Fusion is enabled in normal builds after the qualified arithmetic matrix below.
The per-call prediction experiment remains disabled.
`numeric-chain-timing.py --fused` measures this implementation independently
from the prior per-call prediction experiment.


## Progress

- Implementation contracts checked against current lowering, numeric primitives,
  allocation accounting and production recovery.
- Implemented pure graph validation, exact numeric/aggregate construction with
  scoped roots, and the ordinary Id graph-recovery protocol. Tests exercise
  shared references, distinct equal integers, NaN payload and signed-zero bits,
  malformed references, storage/layout rejection and actual finalizer mutation.
- A compiled Id producer supplies all three node categories through ordinary
  sends. Real registered recovery tests cover one-shot admission, collecting
  recipe reads, Frame aliases, precise GC, capture/restore and code-retirement
  pins. The prior dense recovery protocol has regression coverage.
- `python3 scripts/recovery-graphs.py --verify` records debug/release test binary
  hashes, source hashes, unchanged compatibility authorities and the full
  production gate in `docs/measurements/74t-recovery-graphs.json`.
  The initial run passed: nine new focused tests plus the prior dense-recovery
  test in each profile, and 479 workspace tests with 15 diagnostic tests ignored.
  Installed-prefix, image, cold-archive and relocation checks also passed.
- Native implementation declarations, guarded boxed numeric calls and raw integer
  kernel generation are implemented. Their tests include current target changes,
  observing lookup, actual collection at result allocation, operand mutation
  during that collection, native image pinning and API-wrapper rejection.
  `python3 scripts/recovery-graphs.py --numeric --verify` records this checkpoint
  separately in `docs/measurements/74t-numeric-preparation.json`.
  Debug/release checks and full production verification passed: 487 workspace
  tests, 15 ignored diagnostics, installed Prelude arithmetic and API-context
  probes, image/lifetime, cold archives and relocation. No timings are claimed.
- Raw-chain dataflow is now connected to the actual stackless driver and has
  forced-change differential coverage. Its performance experiment remains
  disabled in normal builds pending profitable admission. The follow-up receipt
  is `docs/measurements/74t-numeric-chains.json` (`--chains --verify`); historical
  preparation receipts above describe their own source checkpoints. Debug/release
  differential checks and full verification passed: 490 workspace tests,
  16 ignored diagnostics, installed-prefix, image, cold-archive and relocation
  checks. Normal-build arithmetic remains on the prior boxed consumer.
- At the raw-chain checkpoint, admission/validation overhead still needed
  reduction. The cache and production fusion work below address that step;
  loop analysis and both allocation-elimination consumers remain outstanding.
- The cache/buffer refinement passed debug/release differential checks and full
  production verification: 491 workspace tests, 16 ignored diagnostics, installed
  Prelude/API-context probes, image/lifetime, cold archives and relocation.
  `docs/measurements/74t-numeric-chain-cache.json` records this checkpoint.
  Cached declaration withdrawal/restoration and active-owner release after
  collecting cache eviction have explicit coverage. The complete qualified
  matrix above establishes that driver fusion is still needed before promotion.

- Bounded numeric driver fusion now passes focused debug/release checks and full
  production verification: 494 workspace tests, 16 ignored diagnostics, with
  installed-prefix, image/lifetime, cold-archive and relocation checks.
  `docs/measurements/74t-numeric-fusion.json` records this source checkpoint.
  Whole-body admission, exact allocation budgeting and existing V2 transitions
  retain the baseline Frame and all object identities. At that checkpoint it remained
  disabled while qualified timing and admission costs were measured. The
  production enablement and evidence follow below.

- The first fused timing attempt did not pass preflight within 120 seconds:
  Firefox exceeded the unchanged 80% per-process CPU limit. No arithmetic rows
  were timed and no speedup is inferred from that attempt. The retained receipt
  is `docs/measurements/74t-numeric-fusion-timings.json`; no process was paused.

- A preparation regression now checks three- and sixteen-operation bodies
  through 128 real registered calls. Neither body compiles a numeric candidate
  until 64 eligible chain starts have occurred; the initial native-tier warmup
  makes the first fused invocations calls 80 and 68 respectively. Both retain
  exactly the pass-off allocation total. The probe reports the entire compiling
  call and cumulative cold-call duration as diagnostic data, separately from
  host-qualified steady execution. The first release probe observed about
  0.19–0.21 ms for the compiling call; this is neither isolated compiler cost
  nor a qualified latency claim. The follow-up receipt is
  `docs/measurements/74t-numeric-fusion-admission.json`.

### Qualified arithmetic and production enablement

The follow-up [complete matrix](../measurements/74t-numeric-fusion-admission-timings.json)
passed all unchanged AC/CPU preflight and interval gates. Each cell comprises
four alternating pairs of 100,000 calls, with automatic collection kept outside
2,000-call arithmetic batches. Every enabled arithmetic call fused; every
allocation total matched its paired baseline. No process was paused.

| Input/operator | Three operations | Sixteen operations |
| --- | ---: | ---: |
| Constant addition | 6.01% faster | 20.60% faster |
| Changing-input addition | 5.47% faster | 21.30% faster |
| Constant division | 6.73% faster | 20.28% faster |
| Changing-input division | 5.80% faster | 22.02% faster |

Every individual pair improved. These are warm arithmetic fixture results,
not application-wide or allocation-elimination gains. Together with the actual
collection/mutation differential tests and delayed preparation evidence, they
support enabling bounded fusion under the existing guards. All original boxes,
physical Frames and public compatibility contracts remain intact.

Normal builds now select fusion for admitted hot numeric chains. Test defaults
also select fusion; the older per-call experiment explicitly opts into its own
mode. `74t-numeric-fusion-production.json` records verification of the enabled
configuration, including installed Prelude arithmetic, mixed/error fallbacks,
API-context withdrawal and image/retirement checks. Application and full cold/
metadata profitability measurements remain separate outstanding acceptance work.
The loop, aggregate-elimination and closure-elimination consumers remain required.

The enabled production configuration passed the full gate: **495 workspace tests,
16 ignored diagnostics**, including the installed numeric fixture. Its exact
source/binary [qualified timing matrix](../measurements/74t-numeric-fusion-production-timings.json)
also passed all four cases and all allocation/hit checks:

| Input/operator | Three operations | Sixteen operations |
| --- | ---: | ---: |
| Constant addition | 6.84% faster | 20.50% faster |
| Changing-input addition | 10.84% faster | 21.06% faster |
| Constant division | 9.74% faster | 22.79% faster |
| Changing-input division | 9.39% faster | 21.77% faster |

All individual pairs improved again. Variation between the two complete matrices
is retained rather than attributed solely to enablement. No process was paused
for either matrix, and no benchmark process remains running after completion.

## Tail-loop entry transfer experiment

The loop consumer targets an invariant of each registered implementation: its
entry layout. Recursive and mutually recursive Id loops repeatedly rebuild the
same receiver/argument/capture destination list at tail transitions. A pure
compiler plan derives the entry bindings and disjoint nil-fill ranges once from
validated V2 metadata. It supports at most 128 Oop slots; larger layouts use the
ordinary path. The plan is process-local and contains no Oops or code addresses.

The driver still publishes and decodes the actual TailCall action, performs
ordinary current lookup, and acquires the resolved registered implementation.
Only then can the plan replace entry-list construction and arena pop/push. Every
argument is copied from the actual call, missing arguments receive canonical nil,
extra arguments remain ignored, and captures are reread from the current exact
receiver payload. There is no selector-name or presumed loop-control intrinsic.
The transfer writes every new Frame slot once, advances its handle generation,
preserves its return destination and stack-depth diagnostics, resets its V2
transition/root state and transfers the same code pins as the baseline.

One optimized interval is one existing tail transition, bounded by 128 slots.
No lookup, callback, publication, iteration or suspension boundary is removed.
The consumer adds no polling operation. Its only eliminated allocations are
private host scratch lists; it does not claim Birnam object-allocation savings.
Plans are initialized lazily through an optional-plan cell in each descriptor;
the four-argument/one-capture fixture then owns 104 bytes of additional plan data.
Unsupported plans are cached as refusal, without changing canonical descriptors,
the public floor table, Method ABI or image encoding.

The complete-state differential test compares all Frame words, pending/root
state, return destinations, scratch contents, generation changes and code pins.
Malformed captures and exhausted generations must reject before publication.
The source-derived registered loop rotates equal-valued but distinct arguments,
changes its capture and method target, installs observing lookup, collects,
and suspends/captures after three optimized transitions. Restoration after GC
must preserve the next result and effect sequence. No later loop callback can run
across that suspension/capture boundary.

The experiment is disabled in normal builds pending correctness and performance
acceptance. `recovery-graphs.py --tails --verify` adds its compiler, arena and real
loop tests to the existing debug/release and full production gates.
`tail-entry-timing.py` checks the passed receipt's source and binary hashes,
uses unchanged host gates, and compares four alternating pairs. Its 1,000-tail
batches retain all ordinary callback work and collect outside the timed region;
hit counts, allocation totals, cold duration and plan metadata are recorded.
Loop optimization beyond this entry-transfer consumer and both Id allocation-
elimination families remain outstanding.
