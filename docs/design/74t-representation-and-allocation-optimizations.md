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

The bounded consumer is now enabled in normal builds after the correctness and
performance evidence below. `recovery-graphs.py --tails --verify` adds its compiler, arena and real
loop tests to the existing debug/release and full production gates.
`tail-entry-timing.py` checks the passed receipt's source and binary hashes,
uses unchanged host gates, and compares four alternating pairs. Its 1,000-tail
batches retain all ordinary callback work and collect outside the timed region;
hit counts, allocation totals, cold duration and plan metadata are recorded.
Loop optimization beyond this entry-transfer consumer and both Id allocation-
elimination families remain outstanding.


The tail-entry experiment passed debug/release and full verification with
498 workspace tests and 17 ignored diagnostics. Its first
[qualified matrix](../measurements/74t-tail-entry-timings.json) measured four
alternating pairs of 500,000 transitions: +4.62%, +5.97%, +0.17%, +4.78%,
median **4.70% faster**. Every allocation total matched and every eligible
transition used the plan. Median initial 128-transition duration was 0.500 ms
without the plan and 0.511 ms with it; this includes ordinary native-tier warmup,
not just entry-plan preparation. The retained additional plan data was 104 bytes.

The enabled configuration also passed full verification, recorded in
`74t-tail-entry-production.json`. Its first exact-source timing repeat was
not qualified: interval median idle fell to 35.88%, below the unchanged 50%
requirement. The mixed timings in `74t-tail-entry-production-timings.json` are
retained but do not establish either a gain or a regression. A quiet follow-up
is required before completing performance acceptance of the enabled source.

The [quiet enabled-source follow-up](../measurements/74t-tail-entry-production-retiming.json)
passed both host gates and all allocation/hit checks. Paired improvements were
+2.08%, +2.28%, −0.64%, +2.29%; median **2.18% faster**. This supports a modest
fixture gain, alongside the earlier 4.70% qualified result. The negative pair
and the unqualified intervening run remain in the receipts. Production now uses
the bounded plan for eligible registered tail transitions. No process was paused
for these measurements, and all measurement processes have completed.

This delivers the first loop-entry consumer; general loop dataflow and broader
application/cold/memory acceptance are still open. Aggregate and closure object
elimination are not supplied by removing these private host scratch lists.

## Allocation-elimination implementation contract

The first experimental aggregate consumer removes a discarded clone of a proven all-Oop
aggregate in a bounded, observation-free body. Eligibility follows the actual
floor clone Method and canonical generated dataflow, not the selector's name.
An escaping clone, an observing lookup/control method, unsupported storage or a
possible collection inside the body executes the ordinary code instead.

Removing a physical block cannot remove its observable allocation charge.
A private dead-allocation journal therefore retains the original vtable and
payload bits without publishing an Oop. Entries count toward the same live-object,
live-byte and automatic-collection accounting until the next collection. They
are deliberately untraced: admission must prove that no identity escapes and
that the object is dead before any observation/collection is possible. These
records cannot represent a live virtual value or a closure still being used.
Their storage and count are bounded; refusal keeps ordinary allocation.

At collection, foreign finalizers and weak-binding retirement require physical
settlement before the observable sweep: recreate the already-charged, unreachable
blocks from their saved bits, without charging a second allocation or allowing
reentrant collection. Ordinary mark callbacks still see the original logical
counts. Where the sweep has no observing finalizer/retirement, reclaim the dead
journal entries directly and include them in the usual reclaimed-object result.
A destroyed Machine's exact floor finalizer may be recognized as inert; arbitrary
finalizers do not inherit that permission. Sweep order and addresses remain
implementation details; no new ordering guarantee is introduced.

This requires direct tests of logical counts versus physical allocation savings,
collection thresholds, observer-triggered settlement, exact saved payloads,
bounded refusal, and existing roots/identity. The integrated consumer must keep
all normal PC/index/result-box accounting, preserve synchronous-boundary counters,
and retire its Frame before the omitted identity could be observed. This work
remains in progress; it does not yet enable aggregate or closure elimination.

The compiler derives a bounded whole-body no-escape plan from canonical portable
lowering and slot coloring. It accepts at most 32 zero-argument clone sends, 128
Frame slots and 64 operations per PC. Clone receivers must come from entry values;
local aliases are allowed, but returning a clone or sending to one refuses the
plan. Runtime admission verifies the actual floor clone Method, current lookup
and Machine controls, all-Oop storage without finalizers or trace hooks, and the
complete original allocation budget through Return. Payload snapshots are bounded
to 1,024 bytes and the journal to 4,096 logical allocations. Repeated clones of
the same prototype within one body share an immutable snapshot; each retains its
own allocation charge and would become a distinct object on settlement.

Source-derived registered-driver tests compare the consumer off and on for one,
three and sixteen clones. They check exact returned identity, logical counts and
bytes, fewer physical blocks, synchronous-boundary diagnostics, zero retained
Frame pins, mark-time observations and equal collection results. Escaping aliases,
replaced clone/lookup/control Methods, exhausted mutation epochs, raw storage,
finalizers, trace hooks, oversized objects and a full journal all retain baseline
execution. Separate collection tests show that saved bits do not keep an otherwise
unreachable child alive, that foreign finalizers see settled physical allocation,
and that collection-triggered input mutation happens at the same allocation
thresholds. Exact floor Machine-finalizer admission distinguishes active,
destroyed, stale-generation and malformed capabilities.

`recovery-graphs.py --allocations --verify` includes these tests and the journal
and compiler proofs in debug/release before the full production gates. The
`discarded-clone-timing.py` driver requires that source-matched correctness receipt
and unchanged host admission. It measures four alternating pairs for all six
combinations of 1/3/16 clones and 16/1,024-byte payloads, retaining execution time,
collection time, cold preparation, physical block/byte savings and peak owned
journal storage. The journal estimate excludes allocator bookkeeping and is not
a claim about process RSS. Small snapshots can cost more host storage than the
object payloads they replace; measurements must retain this tradeoff.

At this first checkpoint the consumer remained disabled pending performance acceptance.
Dead-result elimination does not complete scalar replacement of live aggregate
fields, materialization on exposure, or closure/environment substitution. Those
remain part of the full implementation obligation.

The [correctness receipt](../measurements/74t-discarded-clones.json) records passing
debug/release focused checks and full production verification: 506 workspace
tests, 18 ignored diagnostics, installed runtime, image, archive and relocation
gates. Physical settlement uses a private bounded scratch counter before restoring
the already-charged allocation total; a regression test verifies that settlement
does not overflow by charging an allocation twice. No public ABI or canonical
descriptor/image authority changed. Timings followed this correctness checkpoint.

The first [qualified matrix](../measurements/74t-discarded-clone-timings.json)
completed four alternating pairs of 64,000 calls for each of six shapes. Median
execution changes (positive means faster) were:

| Clones per call | 16-byte payload | 1,024-byte payload |
| --- | --- | --- |
| 1 | −2.09% | −14.02% |
| 3 | +4.74% | −8.25% |
| 16 | +5.95% | −8.40% |

Including collection gives −3.74%, +3.44%, +6.56% for the three small-payload
shapes and −12.66%, −5.30%, −4.66% for the large ones. All allocation/hit checks
passed. Peak journal storage per 128-call batch was 10,240/13,312/25,600 bytes
for small payloads and 139,264/142,336/154,624 bytes for large payloads, respectively.
The physical bytes omitted were 3,072/9,216/49,152 and
132,096/396,288/2,113,536 bytes per batch. These numbers exclude heap-registry
and allocator overhead and do not by themselves establish total-memory savings.
The host passed both gates without pausing another process.

These mixed results do not justify general enablement. Inspection identifies a
specific repeated cost: admission revalidates every payload slot at every clone
site even when the prototype was already validated earlier in the same callback-
free admission. The next experiment reuses that storage proof within the body
while retaining each site's actual Method guard and original allocation charge.
It also checks the snapshot's payload bound before scanning slot metadata. A
mixed-selector regression case changes only the final clone site's implementation;
the complete body must then use ordinary execution even after earlier sites have
already established the same prototype's storage proof.
The [revised correctness receipt](../measurements/74t-discarded-clones-cached-storage.json)
passes the same debug/release and full production gates (506 tests, 18 ignored).

The [cached-storage matrix](../measurements/74t-discarded-clone-cached-storage-timings.json)
also qualified, with the same 64,000 calls per row and four alternating pairs.
For 3/16 clones, execution medians improved by **6.32%/6.93%** at 16 bytes and
**2.38%/13.81%** at 1,024 bytes. Every pair improved for these four shapes.
Including GC gives **5.45%/7.80%** and **2.81%/15.43%**, respectively. Single-clone
cases still regressed: −4.04% and −15.12% execution medians. Physical savings and
journal storage match the first matrix; no process was paused.

Production admission now requires at least three clone sites and one actual
prototype identity shared across the body. Equal payloads in distinct prototypes
do not qualify. The storage proof is reused only within callback-free admission,
and each selector binding is still checked. This leaves isolated clones and
mixed-prototype bodies on ordinary execution; the timing driver now treats the
single-clone rows as ineligible controls. A registered-driver test checks both
equal-but-distinct prototypes and the same object supplied through different
arguments, retaining identical logical allocation totals and returned identity.
Enabled-source verification and timing follow below. This policy promotes only
the bounded discarded-result consumer; live aggregate scalar replacement and
closure/capture elimination remain unfinished.

The [enabled-source correctness receipt](../measurements/74t-discarded-clones-production.json)
passes debug/release checks and full production verification: 507 workspace
tests and 18 ignored diagnostics, plus installed runtime, image, archive and
relocation gates. Normal builds now use this bounded policy. Its exact-source
qualified timing repeat remains a separate acceptance step.

The [enabled-source timing matrix](../measurements/74t-discarded-clone-production-timings.json)
passed both host gates and every allocation/hit check. Four alternating pairs
again used 64,000 calls per row. Median execution gains were **4.23%/8.52%** for
3/16 clones at 16 bytes and **3.43%/11.42%** at 1,024 bytes. Every eligible pair
improved. Including collection gives **3.62%/9.28%** and **4.17%/12.48%**.
The excluded single-clone controls recorded zero optimization hits and zero
omitted objects; their execution medians were −0.04% and +1.32%, with mixed pairs.
No speedup is claimed for these controls. No process was paused, and the timing
process completed normally.

This establishes the bounded consumer's correctness and warm fixture benefit.
Broader application, cold/refusal and total-memory acceptance remain open,
including mixed-prototype admission cost. The next aggregate work must replace
private live fields and materialize shared identity before exposure; discarded
objects alone do not discharge that requirement. Direct closure/environment
substitution and the remaining full-family acceptance obligations are also open.

## Live aggregate field replacement and recovery

The next consumer analyzes a bounded prefix of ordinary Calls. Clone identities
are virtual values; an admitted native field projection substitutes the existing
Oop in that private payload. Registered method names do not establish projection
semantics. The [optional native declaration](../../abi/native-field-projection-v1.md)
comes from and pins the actual loaded Method image, and is revalidated against
the current floor before use. Relational declares its existing LogicVar name
getter; its cached factory, ordinary Store attributes, and Cell operations are
not treated as private-field operations.

The compiler tracks every physical Frame slot containing a virtual call result,
including aliases and call staging. The maps follow actual V2 edge copies,
result initialization and clearing. Unsupported operations end the prefix before
that PC executes. Runtime eligibility additionally stops at unknown or replaced
native Methods. The complete allocation budget excludes collection and callbacks
while virtual identities are live. At least one clone and one field projection
must be eliminated; this does not replace the earlier discarded-clone policy.

A private, single-use allocation capability allows physical materialization of
an already charged clone without changing live counts, bytes or the collection
threshold. Distinct capabilities remain distinct even if their payload snapshots
are shared. Materialization order does not invalidate other capabilities;
collection makes all outstanding capabilities stale, and identifier exhaustion
refuses before charging. The existing dead-only journal carries no extra
per-record identity overhead; only live virtual consumers retain capability maps.

Before an observing PC or an escaping Return, every virtual identity still in
the Frame is materialized once and all physical aliases are restored. This is a
bounded allocation-free *logical* transition: physical storage realizes charges
already made at the original clone Calls, without inserting a collection point.
The original PC token and generated entry then run normally, so any collection,
native observation or stackless control sees ordinary rooted objects and the
original pending/control state. Returning a projected field can leave its now-
dead clone unmaterialized. Shared Store/Cell input identities are never copied.

The live consumer is experimental and disabled in normal builds until integrated
correctness, recovery and profitability evidence support promotion. Field writes,
cyclic virtual graphs, raw/custom-traced storage and closure substitution remain
outside this initial prefix implementation, and remain explicit broader work.

The [first live-field correctness receipt](../measurements/74t-virtual-fields.json)
passes focused debug/release checks and full production verification: 513 tests,
18 ignored diagnostics, installed runtime, image, archive and relocation gates.
Registered-driver comparisons cover returning a field without constructing its
clone, escaping Return, two distinct clones sharing payload values, collecting
native observers, complete physical alias locations, replaced projection Methods,
withdrawn cached declarations, and automatic-collection boundaries that mutate
the original prototype. Observable counts/bytes and effect results agree with
ordinary execution. This is correctness evidence for the experimental consumer,
not a performance or full-family completion claim. Forced recovery at every
prefix boundary and explicit continuation-capture coverage remain next checks.
