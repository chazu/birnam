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


### Forced prefix recovery and stackless control

The live-field consumer now supports test-only forced exits after each admitted
Call. Admission still proves the complete clone/projection region before choosing
the earlier test exit; an exit immediately after a clone therefore exercises
actual materialization, rather than falling back for lack of a field projection.
Allocation admission charges only the executed prefix. Physical slot maps restore
all still-live aliases, including a first clone used to create a second distinct
clone after recovery. A clone already dead at a field-only Return need not be
materialized.

The differential driver matrix also covers suspension and continuation capture
after recovering a live clone. An ordinary observing Method replaces the next
Machine Call publication; the driver then suspends or captures at the original
V2 boundary. The fixture enters with an acquired registered entry, verifies its
active/continuation pin transfer, collects while paused, and resumes with the
pending Call result. A later native observer verifies the original clone identity
and aliases across another collection. Exactly one observation occurs before
pausing and one after resumption: no effect is replayed or skipped. These cases
also run with forced recovery after the earlier admitted Calls.

The live-field consumer remains experimental and disabled. Profitability, cold
preparation, metadata/storage cost and refusal cost remain promotion obligations;
closure substitution and the broader four-family requirements remain open.

The [recovery verification receipt](../measurements/74t-virtual-fields-recovery.json)
passes focused debug/release comparisons and full production verification:
514 workspace tests, 18 ignored diagnostics, installed runtime, image, archive
and relocation gates. No live-field timing claim follows from this receipt.


### Live-field performance matrix

`scripts/virtual-field-timing.py` checks a full `--fields --verify` receipt and
matching release binary/source hashes before timing. It measures four alternating
on/off pairs for field-only Return, escaping clone Return and withdrawal of a
previously cached field declaration, at 16- and 1024-byte payloads. Each row runs
128 calls per batch with collection separately timed between batches, retaining
the ordinary logical allocation charges and collection threshold. The harness
asserts one omitted physical clone per successful field-only call, one recovered
identity per escaping call, and zero optimized calls after declaration withdrawal.

The receipt records warm execution, execution plus collection, the first 128
calls including candidate preparation, physical savings, peak retained journal
storage and retained field-plan/cache storage. The latter quantities exclude
allocator/loader overhead and transient admission scratch; they do not establish
total process memory savings. Both modes retain existing candidate/plan preparation,
so the first-128-call comparison measures activation cost, not the incremental
cost of adding the pass to a compiler that does not prepare it at all. Native
fixture compilation, setup and correctness assertions outside the invocation are
excluded from each timed batch. The existing AC/host-idle gates remain unchanged.


The [first qualified live-field matrix](../measurements/74t-virtual-field-timings.json)
is negative: enabled execution is 3.10%/12.95% slower for field-only returns,
6.45%/17.11% slower for escaping clones, and 12.35%/24.48% slower after cached
projection withdrawal (16/1024-byte payloads respectively). All six medians
including separately timed collection are also negative. The host gate passed;
these are measured admission/recovery costs, not grounds for promotion. Retain
this receipt while reducing repeated layout lookup, premature payload snapshots
and transient admission allocations. The first-128-call numbers remain noisy
and are not an incremental compiler preparation measurement.


Admission now keeps bounded input/result/operation/recovery scratch on the host
stack. Current selector Methods and pure projection declarations are inspected
before copying clone payloads; a region with no admitted projection refuses
without creating snapshots. No speculative value is executed or published until
all cloned payloads pass the existing storage, finalizer, tracing and size rules.
The layout proof reads the exact vtable registry once, checks the explicit prefix
and repeated tail over the required extent, and never inherits a parent's layout.
Repeated clones of the same prototype within this callback-free admission share
the saved payload proof while retaining separate allocation capabilities.

New differential controls refuse an explicit raw payload word, a raw repeated
tail and an oversized aggregate. A layout test covers replacement, unknown kinds,
missing registration and exact-vtable ownership; both debug and release focused
verification include it. The existing forced-exit, collecting-observer, automatic
collection and continuation-capture comparisons remain required after this change.

The [revised admission receipt](../measurements/74t-virtual-fields-admission.json)
passes focused debug/release comparisons and full production verification:
515 workspace tests, 19 ignored diagnostics, installed runtime, image, archive
and relocation gates. The consumer remains disabled pending repeat measurements.


The [qualified admission repeat](../measurements/74t-virtual-field-admission-timings.json)
reduces the large-payload field-only regression from 12.95% to 1.36%, escaping
clone regression from 17.11% to 8.74%, and withdrawal regression from 24.48% to
10.90%. Small-payload medians remain negative (4.06%, 8.42% and 11.56%). This
supports removing the payload-dependent admission overhead, but still does not
justify enabling the consumer. Both qualified receipts retain every paired row.


The next revision removes the duplicate canonical-control check at entry. Pure
preflight may inspect private Frame/payload data and query pinned declarations,
but the existing full control/retirement/epoch guard still runs before the first
original PC and before any logical allocation charge. Unsupported declarations
therefore return to ordinary execution without paying that execution guard.
Added differential cases replace ordinary lookup and Machine Call publication;
both preserve their observation counts and have zero live-field optimization hits,
including under forced-exit test settings. Existing collection and recovery
requirements still apply unchanged.


Review rejected the native-stack scratch representation despite the passing
focused/production gates: conservative native-stack scanning can treat dead
Oop-valued array entries as roots at a later observation. Scratch inputs, values,
nodes and recovery aliases therefore remain in untraced host vectors. The
single layout query, deferred/shared snapshots and final-only execution guard
remain; no timing of the rejected stack representation supports promotion.
Future scratch reuse must retain untraced storage and handle reentry/lifetime.

The [untraced admission receipt](../measurements/74t-virtual-fields-untraced-admission.json)
passes focused debug/release checks and full production verification: 515 tests,
19 ignored diagnostics, installed runtime, image, archive and relocation gates.
It supersedes the stack-scratch implementation for subsequent timing decisions.


The [qualified untraced-scratch matrix](../measurements/74t-virtual-field-untraced-timings.json)
uses the current implementation and all six medians remain negative: enabled
execution is 4.00%/2.48% slower for field-only Return, 7.35%/8.13% slower for
escaping clones, and 9.61%/10.10% slower after declaration withdrawal (16/1024
bytes). Including collection, the corresponding regressions are 5.00%/3.21%,
5.18%/6.24% and 7.99%/7.47%. All four execution pairs in each case are negative.
The host gates passed. The declaration/layout changes reduce the original
large-payload cost, but the two-Call clone/projection shape is still insufficient
for promotion. Cold activation is variable and supports no gain claim.

The next performance work is longer field-reading regions with one admission
covering several eliminated sends, and reusable untraced scratch with explicit
reentry ownership. Keep escaping and withdrawn-declaration controls alongside
those regions. These remaining profitability obligations do not complete the
aggregate family or replace the outstanding closure, numeric, loop and broader
application/metadata/refusal acceptance requirements.


### Reusable untraced scratch and longer field regions

Each compiled live-field candidate now retains at most one cleared scratch set.
An invocation takes ownership of its boxed vector headers and untraced vector
storage without keeping a pool borrow across native queries, generated PCs or
observations. An owner-thread reentry while a set is in use can obtain a separate set; cleanup
releases all snapshot/declaration references and logical values before returning
one set to the pool. Refusal and execution-error returns use the same cleanup.
The scratch box is lazy, so an inactive candidate does not inline all eight vector
headers or allocate backing buffers. Oop-valued storage remains off the native
stack and outside the collector's root graph.

The source-derived comparisons now include 3 and 16 projections. They retain
escaping identities, ordinary lookup/Call overrides, declaration withdrawal,
raw/oversized refusals and capture/suspend behavior. Every supported Call boundary
in the longer two-clone case is forced to recover. A collecting native observer
reenters the same registered block with a different prototype and field value.
The test also holds a scratch checkout live around this real driver execution,
verifies that its original value is unchanged, and checks that the driver obtains
a separate set; subsequent ordinary warm admissions reuse the cached set.

The timing matrix accepts `--projections 1 3 16` (the default) and measures all
three result/refusal shapes at both payload sizes for each selected projection
count. It asserts zero warm scratch-pool misses. Retained plan/cache bytes now
include the lazy scratch box and cached vector capacities, while active nested
scratch and allocator/loader overhead remain excluded. Existing two-Call receipts
remain historical evidence and use their original driver/source hashes.

The [pooled-region verification receipt](../measurements/74t-virtual-fields-pooled-regions.json)
passes focused debug/release checks and full production verification: 516 tests,
19 ignored diagnostics, installed runtime, image, archive and relocation gates.
Live-field execution remains disabled pending the expanded timing matrix.


The [qualified pooled-region matrix](../measurements/74t-virtual-field-pooled-region-timings.json)
passes host admission and 36 interval samples (median idle 65.42%, no low-idle
intervals). Four alternating pairs run 32,000 calls per sample for all 18 shapes.
For 3 and 16 projections, every field-only and escaping-clone execution pair is
positive. Median execution gains are 4.09–5.58% for field-only Return and
2.33–6.46% for escaping clones. Including separately timed collection, those
medians are 1.84–3.30% and 1.00–5.83%, respectively. Logical allocation charges
agree in all pairs; field-only calls omit one physical clone, escaping calls
materialize one original identity, and all warm scratch-pool miss counts are zero.

The single-projection small-payload case remains mixed (0.07% median execution,
2.85% regression including collection); escaping single-projection medians are
negative. Withdrawal controls still regress by roughly 3–5% in most rows, with
mixed pairs at 16 projections. No gain is claimed for these refusal controls or
for noisy first-128-call activation timings. Retained field-plan/cache/scratch
storage for the 16-byte field-only fixture is 1488/2142/7161 bytes at 1/3/16
projections, compared with 856/1510/5537 bytes when execution is disabled; journal
storage remains additional. These figures exclude allocator/loader overhead and
active nested scratch, and do not establish a total process memory saving.

Promotion should admit at least three actual declared projections, cheaply
exclude plans too short to contain them, and reduce repeated work for persistently
refused regions. A bounded refusal retry interval must still fall back to ordinary
execution on every skipped attempt and retry immediately when current lookup
bindings change; declarations changing without an epoch update need bounded
reconsideration too. Verify this policy against the same successful, escaping,
withdrawn, mutation, capture and forced-recovery cases before an enabled-source
qualified matrix. Closure substitution and broader family/application acceptance
remain part of the original full objective.


### Bounded production admission and refusal retries

The live-field consumer is enabled in normal builds with a minimum of three
actual, currently declared field projections. Programs too short to contain a
clone plus those Calls are rejected before allocating symbolic plans or physical
slot maps; remaining plans must contain a sufficiently long zero-argument Call
prefix. Runtime admission still classifies actual Methods and requires the three
projections before preparing snapshots. These checks are profitability policy,
not intrinsic meaning attached to a selector or class name.

A refused candidate skips the next 63 optimization attempts at the same vtable
mutation epoch. Every skipped invocation executes ordinary Id code. A binding
change forces an immediate retry; a declaration changing without a binding change
is reconsidered within 64 invocations. Saturated epochs do not use the retry
shortcut. Initially absent declarations are not permanently negative-cached;
previously admitted summaries remain pinned and are queried again on retry.
Successful execution and execution errors do not enter refusal backoff.

A registered-driver test verifies both initial declaration absence and withdrawal
of a cached declaration, then restores each without an epoch change and checks
the exact retry bound. Method replacement during backoff immediately runs all
three replacement sends; restoring the getter immediately resumes optimization.
Short regions remain in the differential and timing matrices as excluded controls,
with zero live-field metadata and zero attempts/hits. Timing records include
attempt and retry-skip counts, bounding refusal attempts per collection-separated
batch while successful regions still check every invocation.

The [enabled-source verification receipt](../measurements/74t-virtual-fields-production.json)
passes focused debug/release checks and full production verification: 517 tests,
19 ignored diagnostics, installed runtime, image, archive and relocation gates.
The enabled-source qualified matrix is the next promotion check; broader aggregate,
closure, numeric, loop and application acceptance obligations remain open.


The [enabled-source matrix](../measurements/74t-virtual-field-production-timings.json)
and [focused repeat](../measurements/74t-virtual-field-production-retiming.json)
both pass unchanged host qualification. Each uses four alternating pairs of
64,000 calls per sample. The first matrix covers all 18 shapes; the repeat covers
3/16-projection eligible and withdrawal shapes. Short controls have zero metadata,
attempts, hits and omissions. Every 64,000-call withdrawal sample at an eligible
size makes 1,000 admission attempts and skips 63,000, preserving ordinary results
and allocation charges.

The enabled-source field-only medians are 4.14%/4.46% at three projections and
0.75%/1.39% at sixteen (16/1024-byte payloads). The focused repeat gives
5.51%/3.61% and 6.25%/3.17%, respectively. The first sixteen-projection small
payload row regresses 0.91% when collection is included; its repeat improves
4.16%. Escaping-clone medians are positive but variable, ranging from 0.21–5.71%
in the first matrix and 0.39–4.30% in the repeat, which includes a -22.82%
individual execution outlier. All pairs, cold values and negative results remain
in the receipts. Short and withdrawal controls have mixed timing pairs; they
support the admission-count evidence, without a control speedup claim. These
measurements support the bounded consumer while broader workload and resource
acceptance remain open. They are not a whole-program performance guarantee.

### Next implementation: source closure construction and captured environments

The next consumer must analyze an ordinary source-created, immediately invoked
private closure and substitute its captured environment into an admitted body.
`Prepared::new` currently refuses Block literal operations, so this requires
compiler integration in addition to the existing recovery machinery. Keep
shared captured Cells as their original Oops, including when a body reads or
updates them through ordinary messages; effectful uses end the admitted region.

The current Portable Block constructor has an observable cold path: it validates
the CodeObject and invocation arity, installs a vtable carrier/owner and reflective
methods when absent, and checks existing carrier meaning and materialization
ownership otherwise. Its warm allocation stores the original CodeObject followed
by the ordered captures. Closure elimination must establish current constructor,
lookup, carrier and code-ownership conditions before omitting that allocation;
uncached setup continues through the original constructor. A generic aggregate
or a copied Cell is not a substitute for these conditions.

Derive the nested body and capture bindings from PortableProgram data, initially
using the existing small-callee entry/alias/Return proof. Add the literal PC
lowering needed to retain a virtual closure, its allocation charge and all caller
Frame aliases. Reconstruct the exact Block payload and fresh identity before
exposure, replacement, native observation or stackless control. Verify direct
use, multiple-use/escaping fallbacks, shared Cell mutation, current method and
constructor changes, forced recovery, collecting observers, continuation capture
and generation retirement before promotion. This is the next part of the full
four-family objective; broader numeric, loop and aggregate obligations stay open.

### Source closure compiler plan and literal-aware PC emission

`birnam-codegen::closure_elision::Plan` now derives an immediate invocation from
canonical PortableProgram lowering and coloring. It retains the literal's Node
occurrence and nested bytes, ordered capture sources, argument sources and the
original entry Oop returned by the small-callee entry/alias/Return proof. Shared
capture aliases remain references to the same input; the plan never copies a
Cell or infers invocation authority from selector spelling. Observing callee
bodies, wrong arity and passing the virtual closure as an argument decline.

The plan records every physical closure alias at call publication, including
staging scratch, and follows the canonical V2 parallel copies, result
initialization and clear bitmap. A closure retained after the first invocation
is represented for recovery rather than silently treated as dead. Tail
invocations have no caller successor. Original index-box sequences are separated
at construction and checked against actual baseline C output for both ordinary
generation-owned and materialization-owned constructors.

The additive `emit_pc_specialized_step_with_portable_blocks` entry emits one
literal-aware PC with the original constructor and ownership disposition. Tests
compare its body bytes with baseline emission and reject missing, duplicate,
wrong-occurrence and wrong-program literal specifications. The existing emitter
and existing entry functions remain byte-identical. The compatibility checker
excludes only the added development entry from its historical file comparison;
mutation controls confirm changes to the original emitter still fail admission.
No IdApi field, Method signature, descriptor or image format changes.

Verification: [closure-plan receipt](../measurements/74t-closure-plans.json),
generated with `python3 scripts/recovery-graphs.py --closures --verify`, passes
all five new checks in debug and release, the existing numeric/loop/aggregate
checks, and full production verification (522 tests passed, 19 ignored),
including installed-prefix, image, cold-archive and relocation gates.

This is compiler support, not a runtime allocation-saving consumer. Native-tier
`Prepared::new` still refuses Block literals. Next connect the literal to its
original resolver, CodeObject cache, generation/materialization owner and
registered callee; preserve cold setup, prove warm constructor admission, and
charge or recover the exact fresh Block before any observer. Source-created
closure allocation savings and the remaining full-family acceptance obligations
are still outstanding. No new performance claim accompanies this checkpoint.

### Exact warm constructor admission

The Portable Code module now exports the optional
[native Block construction declaration](../../abi/native-block-construction-v1.md).
It recognizes its actual ordinary or materialized constructor, compares the
current producer API with the calling floor, and verifies exact program bytes,
capture count, invocation arity, existing carrier meaning and materialization
owner. Cold setup and changed or unknown constructors decline. The original
constructor implementation remains unchanged.

The query makes no Id allocation or send. Reverse export inspection avoids
interning a missing `BlockVt` name; multiple export aliases can conservatively
decline. The floor's private `birnam_id_warm_block_construction_v1` entry supplies
its own API comparison context and obtains the declaration from the actual
resident constructor image, pinned through the query. It declines outside an
existing owner-checked runtime entry. Neither this native export nor the producer
declaration adds an IdApi field, changes a Method, or authorizes allocation
elimination on its own.

The [constructor receipt](../measurements/74t-closure-construction.json) passes
53 verification commands. Its C fixture loads the real floor, Prelude,
Collection, Error and Portable Code modules and exercises both the producer
declaration and floor admission from an ordinary native Method. It checks cold
refusal, distinct CodeObjects with equal program bytes, original supplied
CodeObject identity, shared mutable captures, exact materialization ownership,
changed allocation API and restored admission, unknown constructors, invalid
carrier extent, changed parent and all ten context operands. A separate process
omits Prelude and verifies refusal without allocation when the Block family is
absent. These checks pass in debug, release and the installed prefix. Full
production verification again passes 522 tests, 19 ignored, and image,
cold-archive and relocation gates.

Next, attempt elimination at the original constructor boundary, after the
literal's existing resolver, CodeObject cache lookup and capture reads. This
keeps ownership and cold setup in the original generated unit rather than
recreating its caches in another native candidate. The Codegen wrapper must
resolve its existing Portable Code capability once, then either enter the
admitted continuation or call that original constructor; retrying capability
lookup on refusal could duplicate observable API instrumentation. The success
path still needs current callee/Frame-control admission, complete post-constructor
slot writes and original boxes, virtual allocation accounting, and exact Block
recovery before observation. That allocation-saving continuation is not yet
implemented. All broader four-family acceptance obligations remain open.

### Experimental source closure allocation elimination

The first allocation-saving closure consumer now executes through the ordinary
portable compiler, original generated unit and stackless driver. It is selected
only with `BIRNAM_DEV_CLOSURE_ELISION=1` and starts after 64 eligible constructor
entries. Default emission retains the original C body. This initial continuation
handles a literal followed by a non-tail invocation of the proven small callee;
tail invocation and observing callee bodies continue through ordinary execution.

The generated wrapper runs the original resolver, CodeObject cache lookup and
capture reads, then resolves the Portable Code capability once. A refusal calls
that exact constructor without repeating capability imports. Admission checks
the current Codegen API, Machine controls, constructor declaration, registered
callee plan, capture identities, all-Oop layout, native retirement state and the
complete no-collection allocation budget. The callee generation remains leased
through substitution and recovery. Neither selector spelling nor a matching
class name grants optimization authority.

The successful path records one fresh virtual Block with the original CodeObject
and ordered captures in untraced host storage. It retains the original allocation
charge and index boxes, applies every post-constructor Frame store and canonical
V2 Call completion, and preserves the reflective ReturnAction scratch state.
Every surviving closure alias is patched to one recovered Block before the next
PC can observe it. If no alias survives, the existing dead-allocation journal
retains ordinary collection/accounting behavior without physical allocation.

A private, single-use driver continuation records the exact Machine, Frame,
successor PC and returned action after Call completion. The original C stack
unwinds before the driver consumes it and enters the successor. Directly calling
the next PC inside the constructor helper would incorrectly retain the literal's
C capture locals across collection; the driver continuation avoids that lifetime
extension. This state changes no serialized descriptor or IdApi field.

End-to-end testing exposed an existing native preparation mismatch: actual
one-PC Block bodies have zero driver arguments, while preparation had always
built a one-argument descriptor. Preparation now honors the manifest's actual
driver arity, enabling its existing leaf proof for these real callees. Other
zero-argument native region candidates remain ineligible until their emitter
supports that entry convention. A source-derived regression compares preparation
with the actual production block compiler's descriptor.

The [elision receipt](../measurements/74t-closure-elision.json) passes the full
production gate: 524 workspace tests, 19 ignored, installed-prefix, image,
cold-archive and relocation checks. The installed source fixture compares
ordinary, enabled and forced-recovery execution over returned capture identity,
distinct escaping closures, shared Cell mutation, method replacement effects,
eviction and collection. Its diagnostics must prove both allocation-elision and
escaping-materialization hits; forced constructor recovery must also execute.
Heap tests compare recovered payloads and padding with physical allocation and
verify that recovery does not charge twice. Compiler tests compare the complete
constructor continuation's stores with baseline physical Frame execution.

`BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=1` recovers the Block before ordinary invocation,
without repeating construction or the caller prefix. `BIRNAM_DEV_CLOSURE_REPORT=1`
reports experimental hits/recoveries on stderr for verification; it is not a
timing mode. No speedup or production promotion is claimed. Remaining work
includes suspension/capture, collecting observation and forced exits across more
boundaries, Codegen API instrumentation controls, metadata/cold/refusal cost,
qualified timings and broader closure shapes. The full numeric, loop, aggregate
and closure acceptance objective remains open.

### Collecting closure observation and active runtime context

The closure admission guard now compares the Codegen API's nil identity with
the owner-checked active Runtime. It no longer reads the process singleton to
perform that comparison. A regression uses two independent heaps and replaced
send/intern/integer callbacks to require refusal without invoking any callback.
This preserves the same production API authority while permitting isolated
runtime verification without initializing a global floor.

The installed `closure-observation.birnam` fixture exercises a caller whose
recovered closure remains live across an observing successor. Its callee takes
an argument and captures two original Cells. The observer verifies both closure
aliases, capture order and returned argument identity, enters another optimized
factory, evicts materializations, collects, and invokes the same closure again.
The caller must subsequently return that same identity. Repeated execution
checks fresh identities and exact effect counts; mutation through the original
Cell remains visible through the recovered capture.

Warmup is separate from the first collecting call. Evicting on every warmup
iteration prevented the initial fixture from exercising the intended candidate.
The smoke verifier now requires both an outer materialization hit and a nested
allocation-elision hit between explicit collecting-call markers. This prevents
successful baseline fallback or unrelated warmup hits from satisfying the test.
Ordinary, optimized and forced-constructor-recovery outputs must match under
both conservative and precise collection. The fixture proves live identity and
ownership survival; it does not establish dead-object retention equivalence or
quiescent native-unit retirement.

Verification is recorded in
[74t-closure-observation.json](../measurements/74t-closure-observation.json).
All 55 commands pass, including debug/release probes and the full production
gate with 525 workspace tests, 19 ignored, installed-prefix, image, cold-archive
and relocation checks. The retained source hashes match the tested files.
Closure elimination remains experimental and disabled by default. The next
control proof must drive an explicit host-owned Machine: after an optimized
constructor and a collecting observer, replace the next ordinary Call
publication with capture or suspension, collect while paused, then restore or
resume with a supplied result. Compare every recovered alias, shared Cell,
effect count and active/continuation generation pin with ordinary execution.
The installed source fixture alone cannot prove that host control lifecycle.
Further forced boundaries, API instrumentation, broader bodies and qualified
cold/metadata/refusal/application measurements remain required, alongside the
other three families' full acceptance obligations.

### Recovered closures through capture and suspension

`closure-control-smoke.sh` adds the explicit host-owned Machine lifecycle to
the installed compiler test. The test floor enables the existing
`ffi-test-hooks` feature and requires `BIRNAM_TEST_CLOSURE_CONTROL=1` before
installing its small host bridge. The ordinary installed floor must expose no
such bridge, even with that environment variable set. The script selects the
separate debug/release test floor through the existing `BIRNAM_ID_DYLIB` host
override; it does not replace the installed library or alter its manifest.

The source factory constructs and invokes its private closure through the real
Codegen runtime. After a verified optimization hit, a native observer checks
the recovered closure's physical Frame aliases and collects. Only then does it
replace the next ordinary Call publication with capture or suspension. While
paused, the host bridge verifies the active/continuation generation pin transfer,
evicts materializations and collects again. Its remembered closure identity is
deliberately untraced, so precise collection requires actual Frame/Continuation
roots to preserve the object.

Restoration supplies the checkpoint Call's result. The successor observer checks
that exact input, the same recovered closure and its aliases, and the original
captured Cell. The caller returns that same closure, which remains invocable
after Machine destruction and sees later changes to the shared Cell. Exactly
two observations occur, with no checkpoint execution or replay. Wrong-Machine
restoration must refuse without consuming the continuation; repeated restoration
or resumption must refuse after completion. Active and continuation generation
pins must return to their pre-test counts.

The comparison covers ordinary, optimized and forced-constructor-recovery paths,
debug/release floors, precise/conservative collection, and capture/suspension:
24 executions in eight differential groups. Optimization/recovery diagnostics
must occur inside the controlled invocation, not just during warmup. Both
observers' physical alias positions are compared across all three paths.
Capture and suspension use separate processes: eviction queues retirement until
the enclosing host call returns, and that pending retirement correctly prevents
later optimizations within the same call. A second case in the same source
invocation would otherwise pass by falling back.

The [control receipt](../measurements/74t-closure-control.json) records the
passing 55-command verification, 525 workspace tests (19 ignored), installed
prefix/image/cold-archive/relocation gates and all eight control groups. Source
hashes match the tested files, and selected test-floor hashes are retained.
This proves the
bounded closure consumer can use the existing V2 continuation machinery; it
adds no production recovery protocol or public ABI field. Closure promotion
still requires broader refusal/instrumentation and forced-boundary coverage,
allocation/resource acceptance and qualified cold/warm/application measurements.
Broader closure bodies and the remaining numeric, loop and aggregate obligations
remain in progress.

### Constructor refusal callbacks and disabled preparation

An instrumented copied Codegen API exposed an execution-time discrepancy in
the experimental wrapper. Ordinary construction called the capability imports,
vtable/extent checks and selected constructor. The refusal path additionally
called `IdApi.dlopen` and `IdApi.dlsym` to locate the private floor hook. Those
callbacks are observable even when the attempted optimization makes no Frame
changes, so returning the same Block was insufficient.

The private hook is now resolved while binding a generated unit and retained as
a process-local function pointer in the already-pinned floor image. Constructor
execution reads that pointer without loader callbacks. If no hook was bound,
the wrapper uses its original resolved constructor. Failed hook resolution is
not permanently cached; later unit binding can retry. Capability lookup remains
at the original constructor boundary, and current floor admission still decides
whether substitution is valid. This changes neither IdApi nor serialized code.

The real-module C fixture `closure-api-context.c` compares the baseline wrappers
with experimental refusal across 20 paired cases: ordinary and materialized
constructors, present/missing/malformed capabilities, changed vtable/extent
queries, and nil or non-nil constructor answers. It requires the exact ordered
callback sequence and original vtable, CodeObject, captures and owner operands.
Before the fix the first valid case recorded extra loader callbacks and failed.
This fixture covers refusal without an active Machine; the installed elision,
collecting-observation and control fixtures separately require successful use
of the hook bound by real Codegen.

Default-disabled staging also stops deriving closure plans. Previously it
lowered and analyzed every staged Block again even though generated code could
not select the closure consumer. A staging regression uses an actual nested
portable inventory and manifest, requires no closure staging allocation in a
fresh disabled registry, and checks that ordinary native plans and descriptors
match across disabled/enabled/disabled staging. Enabled entries retain their
exact manifest alignment and canonical descriptor match.

The [API-context receipt](../measurements/74t-closure-api-context.json) retains
debug/release and installed callback comparisons, staging checks and the full
production gate: all 61 commands pass, with 526 workspace tests, 19 ignored,
all eight continuation comparison groups and installed-prefix, image,
cold-archive and relocation checks. Source hashes match the tested files.
No timing claim follows from eliminating this unnecessary
work. Enabled preparation, repeated refusals and live API overrides still need
cost and differential coverage before closure promotion; broader closure shapes
and the other three families' acceptance obligations remain open.

### Live API overrides and restored closure admission

The explicit test floor now instruments API operations around one hot invocation
of a real source-created closure factory. Candidate creation and warmup finish
before instrumentation. The probe temporarily replaces one operation, executes
the compiled factory, restores the original table and executes it again. The
test-only API pointer and saved callbacks are confined to the existing opt-in
host bridge; no production instrumentation API is added.

The eight cases cover loader-only observation and replacement of send, interning,
integer construction, allocation, import, vtable lookup and payload-size query.
All cases also observe loader callbacks. Send/intern/boxing replacements must
withdraw Codegen-context admission; constructor API replacements must withdraw
constructor admission. The callback traces include selector names and arities,
interned/imported names, integer values, allocation sizes, and the ordered
storage queries relevant to each case. Ordinary and experimental execution must
produce identical traces and return closures retaining the original shared Cell.

During the seven replacements that require refusal, the measured call must have
no closure optimization hit. Loader-only instrumentation permits an optimization
hit but must record no new loader callbacks. After restoring the original
functions, the next invocation must optimize again and return a distinct fresh
closure with the same captured Cell. These requirements distinguish withdrawal
and restored admission from cold code that never attempted optimization.

`closure-control-smoke.sh` runs 32 differential groups (64 executions) across
debug/release floors, precise/conservative GC and all eight cases, in addition
to its capture/suspension groups. It compares complete non-diagnostic output and
requires the expected hits inside both instrumented and restored invocations.
The [live-API receipt](../measurements/74t-closure-live-api.json) records this
matrix and the full production verification. All 61 commands pass, including
526 workspace tests (19 ignored), all 32 live-API groups, the eight continuation
groups, and installed-prefix, image, cold-archive and relocation gates. Retained
source hashes match the tested files.

This extends the prior unit context guard and direct-wrapper refusal proof to
real compiled execution. Enabled preparation, steady execution, cold/metadata
costs and repeated-refusal measurements remain the next evidence needed for
closure promotion. Broader shapes, resource acceptance and the full numeric,
loop and aggregate requirements remain in progress.

### Source closure cost measurement harness

The opt-in test floor now measures real source factories in four paired modes:
return the captured Cell, return the escaping closure, replace the warmed
callee, and refuse an observing callee at compilation. Debug/release smoke
checks require exact hit/refusal/materialization counts, matching logical
allocation charges, one saved physical Block per eligible nonescaping call,
and exact results and replacement effects. Production floors expose neither
this probe nor its counters.

`scripts/closure-timing.py` requires a matching full correctness receipt and
verified test-floor hash, builds a fresh release installation, and records
alternating process pairs. It separates execution, collection, first-128-call
cost, materialization and process wall time. Preparation counters and scoped
metadata payload estimates supplement allocation and journal storage counts;
they are not RSS measurements. Probe assertions and counters remain in the
measured execution. Source and installed binary hashes are checked again after
timing. Host admission thresholds remain unchanged; explicit exploratory runs
retain unqualified results when admission fails.

The [measurement correctness receipt](../measurements/74t-closure-measurement.json)
passes all 61 commands, 526 workspace tests (19 ignored), all eight new paired
measurement smoke groups, continuation and live-API matrices, and full installed,
image, cold-archive and relocation verification. It contains no qualified timing
claim. The consumer remains experimental and disabled by default; performance
measurements are the next step, with all four families still in progress.

### Closure loader-cost diagnosis and pinned declarations

The first [retained source timing matrix](../measurements/74t-closure-exploratory-timing.json)
uses a fresh release installation, four alternating pairs per shape and 64,000
measured calls per process. AC admission fails on battery; the interval check
passes, but the entire matrix remains unqualified. Execution takes about 5.30
times baseline for eliminated closures, 5.37 times for escaping closures, and
5.82 times for a repeatedly replaced callee. Including collection gives about
2.79, 2.71 and 2.86 times baseline respectively. Physical allocation savings and
logical allocation charges match the expected counts. The observing-callee
control has no attempted optimization and does not show a comparable regression.
These are instrumented fixture costs, not application performance claims.

The [diagnostic profiles](../measurements/74t-closure-loader-profile.json) identify
repeated resident-image discovery as the dominant admission cost: 488 of 648
samples within successful admission, and 488 of 543 within repeated refusal.
These separate sampling runs include compilation and collection elsewhere in
the process; those sample ratios describe only the named admission subtree.
A deterministic counter check confirms the problem: 256 warmed invocations
perform 256 discoveries. The new regression requires zero discoveries after
warmup for all four source shapes, alongside the original semantic assertions.

Each bounded closure record now retains at most one exact constructor
declaration and its native-image pin. A changed constructor address requires
fresh discovery. Missing declarations are not permanently cached. The selected
reference keeps its pin through use, even if the record is evicted; dead weak
descriptor records and bounded-record eviction release cached ownership. This
cache contains no Id objects, semantic answers or callable callee entries.
The declaration is queried again at every use, preserving current API, program,
capture, carrier and owner checks; ordinary lookup, layout, Frame, GC and native
generation admission remain unchanged. Old image pins are released outside the
registry borrow. The [pinned-constructor correctness receipt](../measurements/74t-closure-pinned-constructor.json)
passes all 61 commands, 526 workspace tests (19 ignored), eight measurement
comparison groups, eight continuation groups, 32 live-API groups, and the
installed, image, cold-archive and relocation gates. Source hashes match.
The consumer stays experimental and default-off pending a repeated cost matrix
and the remaining closure/resource acceptance work.

### Closure costs after retaining constructor declarations

The [repeated matrix](../measurements/74t-closure-pinned-constructor-timing.json)
uses verified commit `ef014b5e`, a fresh release installation, four alternating
pairs per shape and 64,000 measured calls per process. Every measured process
performs zero constructor discoveries after warmup. Hit, recovery, refusal,
replacement-effect and allocation counts still match exactly. Source and binary
hashes remain unchanged. AC admission again fails on battery; interval admission
passes with 64.19% median idle and no low-idle intervals. All results remain
unqualified.

| Source shape | Execution overhead vs baseline | Execution plus GC overhead |
| --- | ---: | ---: |
| Captured Cell return; closure eliminated | 19.91% | 15.85% |
| Escaping closure; allocation recovered | 21.57% | 8.26% |
| Warmed callee replaced; repeated refusal | 35.22% | 14.82% |
| Observing callee; compiler refusal | 1.84% | -0.79% |

These are paired medians, with all individual pairs retained. Materialization
overhead medians for the three admitted source shapes are 2.73%, 5.01% and 3.17%;
first-128-call overhead remains 21.81%, 29.44% and 29.60%. Preparation-only samples
are short and variable. The table demonstrates that removing repeated discovery
addresses the dominant regression, while the remaining admission and recovery
work still costs more than it saves on these fixtures. It does not establish
profitable closure optimization or application performance.

Next, profile the remaining successful-admission and refusal paths. In
particular, establish whether earlier current-target refusal can avoid
constructor inspection and temporary payload preparation, and whether untraced
host scratch reuse pays for successful calls. Preserve immediate admission
after restoring API/method state, all semantic queries needed for success,
original allocation accounting and collecting-observer behavior. Do not promote
the closure consumer until the remaining costs and resource obligations pass.
Broader numeric chains, loop invariant-work removal, aggregate graphs and
closure shapes, plus application/cold/resource acceptance, still belong to the
full four-family objective.

### Early refusal for ordinary native closure callees

The [post-cache diagnostic profiles](../measurements/74t-closure-refusal-profile.json)
repeat the real source factory twelve times and begin sampling after preparation.
They show no repeated loader discovery. Constructor queries still account for
55 of 174 samples inside repeated-refusal admission, with additional temporary
payload work performed before callee classification. The regression counter
records 256 constructor queries for 256 calls to a replaced ordinary native
callee, despite zero successful optimization hits. These are diagnostic
samples, not qualified timing measurements.

A read-only rejection now checks the current invocation lookup for registered
resumable presence before constructor discovery/query and host payload work.
Absent or ordinary native targets execute the original constructor and call.
This check acquires no entry and retains no negative cache. Successful
admission still performs the original subsequent lookup and generation
acquisition; constructor API, ownership, layout, code/capture and control/GC
checks remain in place. The earlier lookup result grants no execution authority.

The source measurement regression requires one current constructor query per
successful invocation and zero queries for replaced native callees or compiler
refusal. After each replacement measurement, the probe restores the original
method and requires the very first invocation to optimize again, query the
constructor, recover the escaping closure and preserve the shared capture,
without replaying the replacement effect. This restoration check runs outside
the execution/counter snapshots and is included in process wall time.
The [early-refusal receipt](../measurements/74t-closure-early-refusal.json)
passes all 61 commands, 526 workspace tests (19 ignored), eight measurement
comparison groups, eight continuation groups, 32 live-API groups and installed,
image, cold-archive and relocation verification. Source hashes match. Paired
timing follows this correctness checkpoint; the consumer remains off by default
and no full family is declared complete.

The [early-refusal timing matrix](../measurements/74t-closure-early-refusal-timing.json)
retains four alternating pairs per shape and 64,000 calls per process from a
fresh release installation of the verified source. Source/binary hashes and
all counters match, including zero refused constructor queries and immediate
restored admission. AC admission fails on battery, so timings remain unqualified.
Execution overhead medians are 18.74% for eliminated closures, 24.90% for
escaping closures, 19.73% for replaced-callee refusal and 1.67% for compiler
refusal. Including GC gives 16.49%, 10.77%, 10.09% and 0.40% respectively.
The earlier replaced-callee matrix showed 35.22% execution overhead. This
iteration reduces that refusal cost, but successful invocation remains
unprofitable on the measured single-use fixtures. All individual pairs remain
in the receipts; these results do not justify promotion.

The next scope extension is [74U](74u-repeated-closure-invocation-regions.md):
retain one virtual closure across several immediate, proven invocations. This
addresses the existing multiple-use obligation and offers a region large
enough to test whether admission costs can be repaid. It does not remove the
remaining cold/resource/application gates or complete any full family by itself.

### Repeated closure invocations: initial compiler/runtime delivery

[74U](74u-repeated-closure-invocation-regions.md) now implements one virtual
closure across up to 32 immediate calls. Compiler-derived metadata retains
original input identities, changed arguments and results, physical stores,
index allocations and complete V2 transitions. The runtime admits a prefix
under current constructor/callee and control authority, budgets every original
allocation before elision, and recovers one shared identity before unsupported
or escaping continuations. It unwinds the original C locals before the final
observing successor. Forced recovery can select each admitted call boundary.

The [correctness receipt](../measurements/74u-sequence-correctness.json) passes
61 commands, 528 workspace tests (19 ignored), 40 sequence comparison groups
(640 process executions) across both GC modes and debug/release, eight
strengthened capture/suspension groups requiring three substitutions before
pause, the existing measurement/live-API matrices, collecting nested observers,
and full installed/image/cold-archive/relocation gates. Source hashes match.
The 33-use fixture verifies recovery after the 32-call bound; allocation charges,
physical savings and completed-call counts are checked for normal and every
supported forced exit, including escaping closures and shared captures.

This is a correctness checkpoint, with no performance claim or promotion.
Later-target mutation and partial admission, dedicated collection-threshold
and finalizer cases, resource acceptance and profitability remain next within
74U. The full numeric/loop/aggregate/closure and application obligations remain
open; no full family is marked complete.

### Later-target mutation in repeated closure regions

The 74U test-floor source matrix now replaces the second or third invocation's
registered method with an observing native method after warming. The runtime
retains exactly one or two admitted calls, recovers one closure, and executes
the replacement once per factory invocation. The replacement may collect;
the returned closure retains its original captured Cell and observed identity.
Restoring the registered method immediately admits all three substitutions.
The full matrix passes 32 source processes across debug/release and both GC
modes, with exact non-collecting allocation-charge parity. The
[full verification receipt](../measurements/74u-mutation-correctness.json)
passes all 61 commands and 528 workspace tests (19 ignored), including the
existing sequence, control, API and installed-prefix gates. Detailed fresh-prefix
debug counters are retained separately in 74U. Dedicated long-region
allocation-threshold/finalizer cases, resource
acceptance and profitability remain open, along with the broader numeric,
loop and aggregate obligations. The closure consumer remains disabled by default.

### Closure admission at automatic-collection boundaries

The real source budget probe places a warmed host-owned invocation below the
unchanged GC threshold with ordinary allocations. It compares baseline and
optimized root-callback effects, allocation counters and the current Frame's
PC and dimensions for three- and 32-call regions, escaping/non-escaping results
and both GC modes. The expanded development matrix passes 192 processes and
requires refused, partial and full admission. It includes independently measured
whole-invocation allocation boundaries and preserves the collector's existing
before-allocation trigger. The [full correctness receipt](../measurements/74u-budget-correctness.json)
passes all 61 commands, 528 workspace tests (19 ignored), and 384 budget
executions across debug/release. See 74U for the concrete prefixes and receipts.

The full verifier now retains structured per-profile sequence, mutation and
budget results (1,056 source processes) with floor hashes cross-checked against
the control tests. All 241 recorded source hashes match.
Dedicated source finalizer/dead-journal lifetime evidence, resource acceptance
and profitability remain required before closure promotion. Completing the
other families still includes deferring numeric intermediate materialization
and removing invariant loop work, beyond the enabled fusion and tail-entry
consumers. The four-family objective is unchanged.

### Source closure finalizers and bounded journal storage

The source journal probe separates construction and inspection into distinct
Id calls. Its captured private input and child must be reclaimed after their
actual roots expire. An observing finalizer checks the original detached
payload and requires the deferred allocation journal to be settled before its
callback. An intentionally live native root must prevent early finalization;
after its helper returns, collection must finalize the input exactly once.
Initial native-sweep variation is retained separately from deterministic
runtime-root reclamation, following a reproduced variation within baseline
processes. See 74U for the failed comparison and diagnostic evidence.

The expanded development matrix passes 72 source processes. It also executes
4,097 three-call factories below the automatic-GC threshold: exactly 4,096
closures can be eliminated, the next attempt refuses without journal growth,
and the first invocation after collection regains admission using its current
nil input. Owned journal storage peaks at 384 KiB and retains 64–96 KiB of
vector capacity after collection. These counters exclude allocator bookkeeping
and other process/native resources. Whole-process resource acceptance and
profitability remain open; no closure promotion or full-family completion is
claimed.

The [full journal correctness receipt](../measurements/74u-journal-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), 144 journal source processes across debug/release, and the
previous sequence, mutation, budget, control, API and installed-prefix gates.
It retains all 1,200 sequence/mutation/budget/journal rows in eight matrices;
their per-profile floor hashes agree with the independent control output and
all 243 source hashes match. The next closure step is repeated-use profitability
measurement and broader preparation/metadata/resource acceptance.

### Repeated-use measurement verification

[74U](74u-repeated-closure-invocation-regions.md) extends the timing harness to
one, three and 32 calls through each private closure, with eliminated, escaping,
native-replacement and compiler-refusal controls. The probe verifies allocation
charges, completed substitutions, replacement effects and immediate restored
admission for each use count. The timing runner requires retained source/floor
evidence for the exact requested factories.

The [full correctness receipt](../measurements/74u-measurement-correctness.json)
passes all 61 commands, 528 workspace tests (19 ignored) and the production
gates. Ten matrices retain 1,248 process executions, including 48 new measurement
executions; all 244 source hashes and the per-profile control/matrix floor
hashes match. Paired timings follow this checkpoint. The closure consumer stays
disabled by default; raw numeric intermediate deferral, loop invariant-work
removal, broader aggregate/closure coverage and application/resource acceptance
remain required for the full objective.

The [first repeated-use timing matrix](../measurements/74u-repeated-closure-timing.json)
retains 96 process rows and all paired controls. Preflight failed because a
Firefox process exceeded the unchanged CPU limit; the interval gate passed,
but the run remains unqualified. At 32 uses, execution overhead is 10.82% for
eliminated closures and 12.40% for escaping closures (11.12% and 8.88% including
collection). The pass remains disabled. Diagnostic profiles recorded in 74U
identify per-call developer-control environment reads as the largest sampled
optimizer cost. Those controls now read once per admitted region. The
[full correctness receipt](../measurements/74u-control-hoist-correctness.json)
passes all 61 commands, 528 workspace tests (19 ignored), all ten source matrices
with 1,248 process rows, and the production gates. All source/floor hashes match;
every supported forced position remains verified. Retiming is next.

The [qualified control-hoist repeat](../measurements/74u-control-hoist-qualified-timing.json)
now passes both unchanged host gates and all 96 matrix rows. At 32 uses,
eliminated/escaping closure execution time falls 46.10%/45.16%, or 33.29%/34.64%
including collection. The authorized `rk` pause was resumed and verified.
One-use regions still regress by 38-43%; three uses are near execution break-even.
The pass remains disabled pending a measured threshold, broader bodies and
application/resource acceptance. 74U retains the full paired and cold/preparation
results, including the preceding unqualified run and all refusal controls.


### 74V unboxed production checkpoint

[74V](74v-unboxed-numeric-intermediates.md) now executes bounded numeric chains
with deferred intermediate boxes and original V2 Frame recovery. The consumer
is default-off. Physical savings are 2/15/31 result boxes for tested three-/16-/32-
operation chains, with unchanged logical charges. Real installed Prelude and
Codegen comparisons cover every supported PC and both GC modes. A release
function-address mismatch found by those comparisons was repaired by sharing
one immutable owner of canonical numeric API function identities; ABI fields
and signatures remain unchanged, and modified API contexts still refuse.

The [full production receipt](../measurements/74v-unboxed-production-correctness.json)
passes 63 commands, 538 workspace tests (19 ignored), and 1,540 retained source
rows across twelve matching-floor matrices. A separate follow-up adds recovered
Frame control and captured-input checks; its extended production gate and
numeric performance/resource/application acceptance remain open. Loop invariant
work removal, broader aggregate acceptance and closure profitability/body
coverage remain part of the unchanged four-family objective.

## Installed aggregate admission, private layouts and host retirement

The [debug](../measurements/74t-installed-aggregate-debug.json) and
[release](../measurements/74t-installed-aggregate-release.json) matrices now exercise
actual Id source through the installed compiler and real host REPL. Each profile
passes 24 fresh processes, covering one/three/sixteen projections, field-only
Return, a final tail getter, two successive clones and an escaping observer under
both GC modes. Four phases per process provide 192 counter measurements overall.
The exact floors come from the completed 74X literal-control production gate;
all 267 production source hashes still match. A fresh release installation supplies
the compiler and Relational's real native name projection. No runtime change or
new test ABI was needed: the existing numeric bridge reports allocation bytes
and physical omissions, while its numeric completion/recovery counters stay zero.

Three- and sixteen-projection field-only bodies omit exactly one clone per call,
or two for the successive-clone body. The one-projection controls omit none.
The observer checks distinct clone identity and repeated aliases, retains the
escaped identity across collection and verifies its projected field afterward.
Escaping and final-tail-getter shapes omit no physical clones. The latter is a
precise source boundary: a three-getter body ending in a tail send has only two
ordinary projection Calls, below the admission minimum; a longer admitted prefix
must recover its still-live receiver before that tail send.

Each process replaces the getter through ordinary `Reflect defineMethod:` and
checks every replacement effect, original field identity and canonical result.
An object-level override creates a private vtable without an explicit payload
layout. Removing the override restores inherited behavior but does not establish
an all-Oop storage proof for that new vtable; allocation elimination correctly
remains refused. The same compiled body readmits an unchanged, explicitly laid-out
prototype, with the original logical allocation bytes and exact physical savings.
This preserves the distinction between mutable dispatch and storage authority.

The phases return through the real host REPL. An initial single-script experiment
kept optimization disabled even for the unchanged prototype after removing the
Id method. Native admission excludes pending code retirements, and an active
outer Machine cannot drain retirement work held by dispatch readers. Returning to
the host between phases permits readmission. The final check evicts Codegen
materializations, collects, and still obtains the original field identity. This
supports the supported aggregate consumer and real host lifecycle; it does not
claim qualified timing, total-memory savings, arbitrary private layouts, or
completion of the full four-family objective.

## Application profiles with default and experimental settings

The [application diagnostic receipt](../measurements/74t-application-diagnostic-profiles.json)
retains four completed production runs: the existing portable collection pipeline
and cold-materialization workload, each with default settings and with unboxed
numeric, invariant-loop, closure-elision and literal-callee switches enabled
together. Aggregate consumers remain enabled in both configurations. The same
fresh installation and exact fixture bytes are used in both configurations;
all current production inputs still match the completed 74X control gate. Every
run checks the expected application answer, including its final post-GC check.
These combined application checks complement the consumer-specific differentials;
they do not replace their identity/effect/control coverage.

Sampling begins after reported warmup on the verified child PID and executable.
The collection workload uses 25 repetitions instead of five to provide a sampling
window; its per-batch work is unchanged. Cold materialization keeps the original
five batches and finishes before the requested three-second sampling window.
Both processes exit normally in both configurations. The receipt retains the
full drivers, source fixtures, outputs, sampled call graphs, binary hashes and
point RSS observations. It is diagnostic evidence under competing verification
load, with no paired timing or memory-improvement claim.

For the default collection run, GC appears in 540 of 2,212 main-thread samples
and symbol interning in 570. With the experimental switches, those inclusive
counts are 598 and 513 out of 2,280 samples. Cold materialization samples include
compilation, relocation, retirement and collection; the default run has 380
materialization and 275 retirement samples out of 785. These groups overlap and
must not be added together. The shorter experimental cold sample has 320 samples
and cannot be compared by absolute count. Missing candidate-preparation or
consumer symbols do not prove zero hits: sampling and inlining can hide short
work and generated code can lack symbols.

These profiles make collection and cold materialization useful regression
controls for the combined optimizer. They do not establish application speedups
from the four families. The immediate performance work remains the source-matched
loop and unboxed matrices and closure profitability threshold, followed by paired
application/cold/resource acceptance. Shared symbol and dispatch work is recorded
as context, without broadening this four-family implementation into an unrelated
optimization pass.
