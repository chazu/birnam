# 74U — Repeated invocation of one private closure

Status: bounded repeated-closure implementation and focused correctness gates verified; not promoted. The full numeric, loop, aggregate and closure/capture
objective remains owned by 74T and `birnam-sik`.

## Problem and intended behavior

Before 74U, the source closure consumer handled construction and one immediate
invocation. If a successor still needed the closure, it materialized the Block
before returning to ordinary execution. Two immediate uses therefore could not
share an allocation-free region. The retained unqualified measurements also
show admission costs exceeding savings for the single-use fixtures.

Extend the existing compiler-derived plan to describe up to 32 consecutive
non-tail invocations of the same source-created closure, with ordinary local
aliases between calls. Keep its original CodeObject, captures and one virtual
allocation identity throughout the admitted region. Recover that identity once
before an observer or escaping continuation. Shared Cells remain original
objects, and calls still resolve through current Id meaning.

This extends the current entry/capture/argument-returning callee scope. It does
not assume that arbitrary closure bodies are pure, that names grant authority,
or that allocation removal is profitable merely because it is legal. Unknown
sends, new literals, writes, additional closures, tail calls and unsupported
control end the region before their execution. The ordinary continuation then
handles them with all necessary identities materialized.

## Compiler representation

Add a bounded sequence description alongside the current first-constructor
plan. Derive it from canonical portable lowering and slot coloring; never
infer it by parsing generated C or trusting producer assertions about native
methods. Retain the existing first-literal provenance and descriptor checks.

For each admitted call, record:

- The original caller PC, call-site identifier, selector, argument sources and
  the original sequence of local stores and integer index allocations.
- Symbolic values referring to original caller inputs or the single virtual
  closure. Propagate previous call results through logical value identities,
  independently of reused physical slots.
- Complete physical closure aliases at invocation, and the original V2 edge,
  parallel copies, result destination and clear bitmap at completion.
- The returned input source implied by the admitted callee plan, plus aliases
  requiring materialization if the region ends at that successor.

Bounds remain explicit: 128 slots, 128 caller blocks, 4096 logical values and
64 KiB of nested program bytes; at most 32 admitted calls. Follow only a bounded
acyclic successor chain. A refused extension retains the valid shorter region;
it cannot silently execute the unsupported operation. Stop before the ordinary
return/tail/observing PC, so its existing driver behavior remains authoritative.

Test symbolic substitution against independent canonical physical execution,
including call-result bindings, argument changes, capture aliases, colored slot
reuse, all V2 copies and clear masks. Compare index-allocation order with actual
baseline emission. Selector spelling and malformed layouts remain refusal cases.

## Runtime execution and recovery

The original C constructor hook remains the entry point. It preserves existing
CodeObject/vtable resolution and capture evaluation, then supplies the floor's
registered plan. The public IdApi, Method ABI, canonical descriptors and images
remain unchanged; sequence metadata is private and derived again when needed.

Before removing the constructor allocation, establish the full region's
allocation budget, including original caller index boxes, intermediate caller
PC tokens, each callee's driver and alias/return boxes, and the original Block
charge. Decline if any original automatic collection boundary would occur inside
the region. Retain allocation/finalizer accounting through the existing journal.
Host scratch containing Oops must remain untraced storage, never a native array
that extends conservative roots across a later collection.

Use the existing current-constructor declaration, current invocation lookup,
registered callee generation lease and exact small-callee plan checks. Every
admitted call must have a valid current target; metadata alone does not confer
authority. Acquire/release leases through existing owner-thread machinery, and
preserve canonical Frame/control guards. No semantic assumption survives an
unknown operation, collecting boundary or reentry.

Execute each original caller prefix and publish its original Call action. On
success, apply the existing exact callee return and V2 caller completion. Charge
each intermediate PC token at its original place before executing the next
prefix. Keep one journal identity and patch all aliases together if recovery
becomes necessary. A forced recovery at each supported call boundary must
return the already-published ordinary Call without repeating construction,
previous calls, argument evaluation or committed effects.

At the region's final successor, materialize the Block if required by live
aliases. Return through the existing Resume mechanism and unwind the original
C constructor/capture locals before the normal driver allocates the final
successor token or enters any observing PC. Intermediate calls occur only in
the admitted region without observers or collection. The driver must validate
the actual final Frame/PC/action tuple even though the original C entry began
at PC zero.

## Acceptance sequence

1. Compiler sequence derivation and differential physical-state tests, with
   one-, two-, three- and 32-call cases and bounded refusal beyond the limit.
2. Real installed source execution through the floor consumer, requiring one
   virtual Block identity, the expected number of substituted calls and exact
   logical allocation charges. Include changed arguments and shared captures.
3. Escape/observer successors, distinct closures from separate factories,
   shared Cell mutation after recovery, changed invocation methods and lookup,
   and exact effects from unsupported continuation calls. Restore original
   methods and require immediate subsequent admission.
4. Forced recovery at every admitted call boundary, collecting successors,
   native observation, precise/conservative GC, allocation-threshold/finalizer
   controls and native ownership/retirement. Extend the existing V2 capture and
   suspension matrix to a region with several successful substitutions.
5. Full debug/release, installed-prefix, image, cold-archive and relocation
   verification. Commit correctness before performance acceptance measurements.
6. Paired fresh-release measurements for single use, repeated use, escape and
   refusal. Record actual hits, physical savings, execution/GC, cold preparation,
   metadata and journal storage. Retain negative results and unchanged host
   admission requirements. Application/resource acceptance remains mandatory.

The implementation must supply executable evidence for these requirements.
Passing the compiler tests alone does not complete this extension, and passing
the extension does not complete the full four-family roadmap.

## Initial implementation evidence

The compiler now derives a bounded `following` sequence alongside the original
constructor plan. Its differential tests cover 1, 2, 3, 32 and 33 uses, capture
and argument results, escaping aliases, prior call results, colored slot reuse,
physical stores and exact baseline-emitter index sequences. Observing operations
and changed call shapes terminate the extension; selector spelling grants no
authority. The 33-use case retains an ordinary continuation after call 32.

The runtime admits a prefix using current lookup, registered callee leases,
receiver capture-layout checks and exact leaf plans. It budgets original Block,
caller/callee index and intermediate PC-token charges before recording one
virtual identity. An inadmissible later callee or insufficient budget retains
a shorter prefix. Each call publishes its original action and applies canonical
V2 completion. The final continuation recovers all remaining aliases before
unwinding through Resume. `BIRNAM_DEV_CLOSURE_FORCE_AT` selects a zero-based
call boundary for forced recovery; existing forced-first-call behavior remains.

A test-floor source runner checks allocation charges, physical savings, completed
substitutions and materialization across both GC modes, escaping/non-escaping
results and every admitted forced-exit index. The existing collecting-successor
fixture now performs three outer substitutions and three nested substitutions.
The explicit capture/suspension fixture also performs three substitutions and
requires that exact count before pausing, in addition to its prior alias, shared
Cell, supplied-result, collection and native-pin checks.

The initial verification attempt caught a diagnostic-filter mistake: accepting
every `closure-elision:` line as a diagnostic also removed the fixture's ordinary
result line. The filter now recognizes only hit and completed-call diagnostics.
The [failed receipt](../measurements/74u-sequence-initial-verification.json) is
retained separately. The [corrected full verification](../measurements/74u-sequence-correctness.json)
passes all 61 commands: 528 workspace tests (19 ignored), 40 sequence comparison
groups covering 640 baseline/optimized/forced process executions, eight existing
measurement groups, eight strengthened capture/suspension groups, 32 live-API
groups, and installed-prefix, image, cold-archive and relocation verification.
The retained source hashes match the tested files. The earlier
[development receipt](../measurements/74u-sequence-development.json) was a focused
run with an older verified Codegen prefix and a newly built test floor; it is
not the production provenance claim.

The consumer remains experimental and disabled by default. Broader resource
acceptance, closure bodies and profitability remain open.

## Later-target mutation and immediate readmission

The source mutation fixture uses three calls through one private closure and
aliases its registered `value:` method under an unrelated selector for the
second or third call. After warming, it replaces only that alternate method
with an observing native callback. This requires runtime target admission to
retain a prefix of exactly one or two substitutions and materialize the same
closure before executing the replacement. The callback checks the original
captured Cell and actual argument identity, optionally collects, and records
the receiver address in untraced thread-local storage. The factory must return
that exact receiver, with a fresh closure on each invocation. A root for the
previous result prevents address reuse from invalidating the freshness check;
the currently recovered receiver relies on its ordinary Call/Frame roots.

Eight measured invocations require eight replacement effects, eight optimized
prefix hits when enabled, and zero physical allocation savings for the escaping
results. Without callback collection, baseline and optimized logical allocation
charges must match exactly. Collecting cases deliberately omit that byte-delta
comparison because the collector resets the underlying counter. Restoring the
original registered method must immediately admit all three substitutions on
the very next invocation, without another native effect or a cooldown.

The [focused development receipt](../measurements/74u-mutation-development.json)
passes 16 source processes covering both GC modes, both mutation positions and
collecting/non-collecting callbacks, each with optimization disabled/enabled.
It uses an older verified Codegen prefix and the newly built debug test floor;
the fresh installed-prefix verification is recorded separately. The probe is
available only through the opt-in feature floor and does not change the public
ABI or enable the experimental consumer.

The [full verification receipt](../measurements/74u-mutation-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), 16 mutation comparison groups covering 32 processes across
debug/release and both GC modes, all 640 sequence executions, eight existing
measurement groups, eight capture/suspension groups, 32 live-API groups, and
installed-prefix, image, cold-archive and relocation gates. All 239 retained
source hashes match the tested files. The fresh-prefix debug matrix's
[individual counters](../measurements/74u-mutation-debug-matrix.json) are also
retained: each non-collecting eight-invocation row charges exactly 10,112 bytes
in either mode. Enabled prefixes complete eight or sixteen calls, with eight
materializations/effects and zero physical savings; the first restored
invocation completes three substitutions and one materialization. Release
matrix success is retained in the full verification output. No timing or
promotion claim is made.

## Allocation-budget boundaries and collecting callbacks

The source budget probe prepares a real registered factory and host-owned
Machine, warms its closure consumer, then allocates ordinary padding to place
the heap a chosen number of bytes below the unchanged automatic-GC threshold.
It verifies that automatic collection is actually enabled with a live native
stack anchor. It never disables GC or changes the threshold. The probe uses a
private all-Oop object as the factory argument; its captured identity must
survive ordinary or recovered closure execution.

A root callback records the allocation counter and current Frame PC, PC count
and slot count. It changes one field of the private object and marks the newly
installed value without sending or allocating during collection. Baseline and
optimized processes must produce identical collection counts, callback traces,
field effects and post-invocation allocation counters. Optimizer counters must
also prove actual full, partial and refused admission; equivalent results from
an always-refusing consumer do not pass.

The matrix covers three and 32 calls, escaping and non-escaping results, both
GC modes, and nine initial allocation gaps from 64 to 16,384 bytes. A baseline
run independently supplies the whole invocation's allocation extent, adding
three cases eight bytes below, at and above that extent. The collector checks
its threshold before an allocation, so the last allocation may leave the
counter at or above the threshold without collecting within this invocation.
Those boundary cases must preserve that deferred behavior too.

The [initial development receipt](../measurements/74u-budget-initial-development.json)
passes 72 precise-GC processes over the initial nine gaps. The
[expanded development receipt](../measurements/74u-budget-development.json)
passes 192 processes over both GC modes and all twelve gaps. These use the
older verified Codegen prefix and newly built debug feature floor. Three-call
factories exercise prefixes of zero, one and three calls; 32-call factories
exercise zero, one, four, eight, seventeen and 32. Partial regions materialize
one closure before collection; complete non-escaping regions save one physical
allocation while preserving its logical charge. These are correctness results,
not timing or resource-promotion evidence.

The full verification runner now retains each profile's sequence, mutation
and budget matrices as structured data in its main receipt. It requires all
six matrices to pass and their per-profile floor hashes to agree with each
other and the independently reported control-test floor. At this budget
checkpoint, source finalizer/journal-lifetime checks, metadata/resource
acceptance and profitability remained open.

The [full correctness receipt](../measurements/74u-budget-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), 384 budget executions in 16 comparison groups, the existing
640 sequence and 32 mutation executions, and all control, API, installed-prefix,
image, cold-archive and relocation gates. Its `closure_source_matrices` field
retains all 1,056 source-process rows across six debug/release matrices. All
241 source hashes match the tested files; each profile's three matrix floor
hashes agree with its independently reported control-test floor. The closure
consumer remains disabled by default, with no timing claim.

## Source capture lifetime, finalizers and journal capacity

The source journal probe runs a warmed factory with a fresh private input and
child, then returns nil to its Id caller. A separate source call collects after
the factory and constructor native frames have unwound. The probe keeps only
untraced addresses for identity checks, while the source keeps the factory's
owner alive independently of its private captured input. An optional finalizer
on that input checks its detached payload identity, requires an empty dead
journal, and requires physical and logical allocation counts to agree during
the foreign callback.

Three collection modes distinguish actual root obligations:

- Runtime-root collection excludes native stack words and must reclaim both
  private objects immediately.
- Ordinary native-stack scanning retains its first-sweep observations, then a
  runtime-root collection checks exact eventual reclamation and accounting.
- A deliberately live native root protects the input during the first native
  scan. Its child must also survive and its finalizer must not run. After the
  native helper returns, runtime-root collection must reclaim both objects and
  run the observing finalizer exactly once.

This distinction follows a test diagnosis, not a change to collector policy.
The [initial capacity comparison](../measurements/74u-journal-capacity-initial.json)
failed on an eight-byte first-sweep difference. A
[repeated diagnostic](../measurements/74u-journal-native-repeat.json) showed
variation within baseline processes too: incidental native words retained
different dead objects, sometimes including the input. Exact first-sweep
reclamation is therefore not a reproducible baseline contract. The corrected
probe retains those observations, checks the initial logical allocation counts,
and compares cumulative reclamation and final heap state after native roots no
longer apply. Its additional live-root case proves absence of premature
finalization. No optimizer or collector behavior was changed to make this pass.

The capacity case executes 4,097 three-call factories without crossing the
automatic-GC threshold. Enabled execution must save precisely 4,096 physical
closures, complete 12,288 substitutions and refuse the next candidate without
growing journal storage. Baseline and enabled execution both charge 5,506,368
bytes. Collection must discard all journal entries; the very next factory call
must regain admission and complete three substitutions, without a cooldown.
That invocation receives nil, checking that admission uses its current input
after the previous captured objects have been reclaimed.

The [initial lifetime development receipt](../measurements/74u-journal-initial-development.json)
passes 32 source processes before capacity testing. The
[expanded development receipt](../measurements/74u-journal-development.json)
passes 72 processes spanning three-/32-call factories, the capacity case,
observing/non-observing collection, both heap-layout GC modes and all three
native-root modes. These use the older verified Codegen prefix and newly built
debug feature floor. Peak owned journal storage is 393,216 bytes at the
4,096-entry limit; after collection, retained vector capacity is 65,536 or
98,304 bytes depending on whether settlement consumed the dead-entry vector.
This counter includes snapshot payloads, reference-count headers and vector
capacity, but excludes allocator bookkeeping and other runtime/native storage.
It is not an RSS or whole-process resource claim. Cold/preparation/metadata and
application resource acceptance, broader closure bodies and profitability
remain open, and the consumer remains disabled by default.

The [full correctness receipt](../measurements/74u-journal-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), 144 journal executions in 72 comparison groups, the previous
640 sequence, 32 mutation and 384 budget executions, and all control, API,
installed-prefix, image, cold-archive and relocation gates. Its eight
`closure_source_matrices` retain all 1,200 process rows, including the native
first-sweep diagnostics. All 243 recorded source hashes match; each profile's
four matrix floor hashes agree with its independent control-test floor.
Debug and release both verify the 384 KiB capacity peak, bounded retained
capacity, exactly-once observing finalization and immediate restored admission.
The next step is paired measurement of repeated-use profitability together
with broader preparation/metadata/resource acceptance. This does not complete
the full numeric, loop, aggregate or closure family.

### Repeated-use measurement coverage

The measurement harness now generates static source factories with one, three
or 32 immediate calls through one private closure. Each use count has the same
four controls: return the captured Cell, return the escaping closure, replace
the warmed callee with an ordinary native method, and use an observing body
which the compiler refuses. The probe separately reports factory calls and
completed optimized invocations. Successful regions must complete every use;
replacement effects must occur once per use, and restoring the original method
must admit every use on the very next factory invocation. Allocation charges
must match baseline execution at every use count.

The [development receipt](../measurements/74u-measurement-development.json)
passes all 24 process executions with the newly built debug feature floor and
the older verified Codegen prefix. These are accounting and result checks,
not qualified timings. Full verification now retains the measurement matrix
for each profile alongside sequence, mutation, budget and journal matrices.
The timing runner requires a matching release measurement matrix, source
hashes and floor hash for every requested use count. Generated fixture hashes
are retained and checked again after measurement.

The paired matrix is selected with `--uses 1 3 32`. Each batch still contains
128 factory invocations followed by separately timed collection; a 32-use
batch therefore executes 4,096 closure calls. Reports retain cold execution,
materialization/preparation, metadata and journal storage alongside warm
execution and collection. All source factories return checked results, and
allocation/counter validation runs for every pair. The existing AC/host-load
qualification gates are unchanged. This prepares the profitability experiment;
it does not establish a speedup or promote the consumer.

The development captured-Cell-return cases retain 2,287 / 3,523 / 21,445 bytes
of reported plan/record payload for one / three / 32 uses. Each saves one
physical Block per factory, regardless of use count; the 128-factory journal
peak stays at 12,288 bytes. Repeated use therefore tests whether substituting
more calls repays the larger plan and per-call admission work, rather than
increasing the number of closure allocations removed. These metadata figures
exclude vector spare capacity, map nodes, reference counts and allocator/loader
storage, as specified by the probe; they do not establish whole-process memory
acceptance.

The [full measurement correctness receipt](../measurements/74u-measurement-correctness.json)
passes all 61 commands and production verification: 528 workspace tests
(19 ignored), all control/API/installed-prefix/image/cold-archive/relocation
checks, and ten retained source matrices containing 1,248 process executions.
The new measurement matrices contribute 48 executions in 24 comparison groups.
Each profile's five matrix floor hashes match its independently reported
control-test floor, and all 244 source hashes match. Both profiles confirm
the metadata and journal figures above, exact allocation parity, all repeated
substitutions and immediate admission after method restoration. Paired timing
is now admitted for all three use counts; profitability and the full four-family
acceptance obligations remain open.

### First paired repeated-use timings

The [paired matrix](../measurements/74u-repeated-closure-timing.json) contains
96 processes: four alternating baseline/enabled pairs for each of three use
counts and four shapes, with 64,000 factory invocations per process. It uses a
fresh release installation and the feature floor from the full correctness
receipt. Every pair passes result, allocation and optimization-counter checks;
source, generated-fixture, floor and installation hashes remain unchanged.

These timings are **unqualified**. The machine was on AC power, but a Firefox
process used 90.9% of one core at preflight, exceeding the unchanged 80% limit.
The subsequent interval gate passed: 124 intervals, median idle 58.20%, seven
below 30%. The failed preflight is still disqualifying; interval admission does
not upgrade this run.

Median execution overhead relative to the paired baseline:

| Uses per closure | Eliminated closure | Escaping closure | Replaced native callee | Compiler refusal |
| --- | --- | --- | --- | --- |
| 1 | 33.54% | 43.84% | 18.20% | 0.11% |
| 3 | 29.36% | 29.69% | 9.48% | 0.65% |
| 32 | 10.82% | 12.40% | 13.96% | 0.06% |

Including separately timed collection, the eliminated-closure overhead is
20.96% / 22.11% / 11.12% for one / three / 32 uses. The escaping-closure
overhead is 16.48% / 14.02% / 8.88%. All pair values, including noisy outliers
and the compiler-refusal controls, remain in the receipt. Repetition reduces
the proportional successful-path overhead in this matrix but does not establish
a profitable consumer.

For captured-Cell returns, enabled median materialization time is approximately
0.748 / 0.900 / 2.616 ms; the floor-preparation subset is 26.5 / 27.4 / 110.2 us.
Paired materialization overhead is 2.42% / 8.71% / 6.56%, and cold execution
overhead is 17.26% / 25.92% / 6.44%. Reported plan payload remains
2,287 / 3,523 / 21,445 bytes, and journal peak remains 12,288 bytes per batch.
These are instrumented fixture observations, with the earlier resource-scope
exclusions; application and whole-process acceptance are still open.

The consumer remains disabled by default. The next step is sampling the
successful repeated-use path to identify the remaining admission/execution
cost before changing it or expanding callee bodies. Numeric intermediate
deferral, loop invariant-work removal and broader aggregate/closure acceptance
remain part of the full objective.

The [diagnostic profiles](../measurements/74u-repeated-closure-profile.json)
sample one-use enabled and 32-use baseline/enabled execution after preparation.
The wrapper repeats the same measurement twelve times to provide a sampling
window; all twelve 32-use baseline/enabled accounting pairs pass. These are
diagnostics, not qualified timings. In the 32-use enabled profile, 1,046 of
1,549 samples are under `try_inline_closure`; 598 of those are in libc `getenv`.
Callee admission accounts for 64 samples, including 12 in entry materialization.
The one-use enabled profile has 198 `getenv` samples within 364 optimizer
samples. Counts include each topmost matching subtree once; Rust wrappers and
libc lookups are not double-counted.

The source rereads `BIRNAM_DEV_CLOSURE_FORCE_RECOVERY` and
`BIRNAM_DEV_CLOSURE_FORCE_AT` at every substituted call. The next change reads
these developer controls once after region admission, before its first effect.
An admitted region excludes observing callbacks and collection; the next
region must still read current controls. Forced recovery at every call position
must continue to pass. This removes repeated control lookup without caching
mutable Id method, layout or capture answers. Correctness and a new paired
matrix are required before attributing any gain to it or promoting the pass.
