# 74Q — Optimization roadmap with malleability and recovery

Status: proposed roadmap, 2026-09-12. Documentation only; this document does not
implement or promote another optimization. It follows the user's request for a
plan covering the opportunities beyond 74P, including transformations for which
deoptimization cannot undo an observable change.

Implementation status, 2026-09-13: [74R](74r-small-callee-recovery-plans.md)
delivers the first bounded inlining batch. [74S](74s-three-optimization-families.md)
implements inherited dispatch caching, guarded native Frame regions and selective
native copy propagation, with its separate performance acceptance. The current
four-family work is tracked by [74T](74t-representation-and-allocation-optimizations.md)
and `birnam-sik`.

| Family | Implemented behavior | Evidence and remaining acceptance |
| --- | --- | --- |
| Numeric representation | Production numeric fusion is enabled and retains original boxes. [74V](74v-unboxed-numeric-intermediates.md) carries raw intermediates and reconstructs identities at original boundaries; unboxed execution remains default-off. | Scratch reuse and batched raw snapshot storage are integrated. The separate counted Frame-index candidate passed its complete production gate, numeric/aggregate/V2 checks, 240 normal/test-floor driver cases and 120 Id-driven allocation cases per profile. Its 480-process exploratory comparison is mixed: selected precise-GC 32-operation unboxed cases improve about 6–7% including GC, while other configurations regress. The main-based composition excluding the held Error declaration is integrated with counting default-off after complete production and installed correctness acceptance. Default-path cost and application/resource acceptance remain open. |
| Loop optimization | Tail-entry transfer is enabled. [74W](74w-loop-invariant-numeric-work.md) reuses arithmetic across proven successive self-tail iterations; the integrated unchanged-input path skips the native kernel call. Invariant reuse remains default-off. | The additional stable-cache write reduction remains isolated. Its full production/driver checks and 384-process direct predecessor/candidate comparison are complete, with mixed results. Four valid measured-work profiles are retained; unresolved or inlined frames limit attribution. No general profitability or promotion follows. Application/resource acceptance remains open. |
| Temporary objects | Repeated discarded clones and private field projection regions are enabled under their measured minimum lengths, with exact allocation observations and bounded refusal retries. | Qualified fixture gains and installed-source identity, mutation, private-layout and retirement checks remain valid. The four existing application controls admit no aggregate plans, so they establish refusal/regression behavior rather than application benefit. Native caller-root preservation and baseline TCC resource diagnostics are now verified and integrated. The Error-field declaration and two-word snapshot experiment remain isolated. Application benefit and total-resource acceptance remain open. |
| Closures and captures | [74U](74u-repeated-closure-invocation-regions.md) eliminates private closure allocation across up to 32 immediate calls while retaining shared Cells. [74X](74x-closure-literal-results.md) adds fresh Integer literal results and V2 recovery. Both remain default-off. | Descriptor-only admission and deferred hot plan derivation are integrated. The complete 144-process normal-floor comparison after hot preparation shows roughly 13–17% median gains including GC at 32 uses, with mixed pairs and smaller/control results. The latest shared-support gate retains complete closure source/control coverage and adds evaluator lifetime checks. Preparation, wider bodies and application/resource acceptance remain open. |

The user has waived quiet-host admission for ongoing work. Continue exploratory
measurements with host diagnostics and explicit `qualified: false`; retain every
correctness, identity, GC/finalizer, V2, native-lifetime and compatibility gate.
The collection-workload ablation and diagnostic counters are retained in 74T:
that workload recorded no closure-consumer activity, so its noisy regression
cannot justify changing closure admission policy.

The earlier source-probe execution-state failures are no longer the next blocker.
The native caller-root repair preserves the outer evaluator's live values while
excluding only probe scratch from scanning. Shared verification support is
integrated at `ef00b7a5` after all 75 production commands, 3,164 optimization
source cases and 24 evaluator-lifetime/baseline-storage cases passed against
288 source inputs. See [74T verification support](74t-verification-support-integration.md).
This integration excludes the Error-field declaration and experimental consumer
changes; its proof does not promote them.

The immediate pending step is the exact main-based counted composition on
`codex/74y-production-composition`. Its source checkpoint `42c6e94c` combines
counted indices and reverse numeric-journal lookup with main's verified support,
without the held Error declaration. Its 295-input production gate passed all
75 commands. A fresh installation passed all 924 debug focused cases and the
release numeric60/ordinary-Id120 matrices. The release aggregate matrix stopped
after three unmatched pre-work heaps at one coordinate; all 202 individual
attempted processes passed their original checks, but only 160 were accepted
in comparable pairs. Commit `241820c1` retains the failed acceptance and a
sixteen-process diagnosis showing the same one-object/24-byte variation with
the counting control unchanged. Subsequent isolated inspection identifies a
REPL result-rendering closure retained by the native root interval; one later
diagnostic run instead retains a String containing `number`. The repair changes
aggregate fixture presentation through ordinary Id, preserving all roots,
explicit assertions and host return boundaries. On
`codex/74y-composition-quiet-repl`, fixture checkpoint `53dde2ef` passed all 240
preliminary release aggregate cases without retries. Its debug matrix also
passed 240 cases without retries, independently audited at `994cc164`. The new
source-bound production gate subsequently failed a nested evaluator lifetime
output assertion: the shared fixture also suppressed printing for non-measurement
callers. The failed proof and queue are retained at `9e811468`. The repair on
`codex/74y-scoped-repl-fixture` (`48a1d54d`) makes quiet presentation explicit in
aggregate measurement callers. All 50 default inputs match the original generator
and all 50 quiet inputs match the earlier measured inputs byte-for-byte. With
identical binaries, the original lifetime failure reproduces and all 12 repaired
cases pass. Its full gate has now passed all 75 commands and 3,188
source/lifetime/storage rows, independently audited against all 295 current
inputs at `9c8f3540`. The first follow-on watcher had stopped while the original
production process was still live; the resumed watcher retained that failure and
used stricter identity handling. The production job was never restarted. The
resumed watcher has published a fresh prefix. Debug numeric60 and ordinary Id120
pass; all 240 debug aggregate cases pass in 120 comparable pairs without retries.
The independent audit at `eaac0f5b` verifies source/install/floor bindings, raw
identity/effect assertions, logical charges and 960 collection phase snapshots.
The release aggregate matrix also passes all 240 cases in 120 first-attempt
pairs, independently audited at `c5b6e073`. Both profiles together cover 480
aggregate processes and 1,920 collection phase snapshots without retries.
All 1,848 focused installed cases and the final 240-case normal/test-floor driver
now pass. The independent current-source/install/floor and case audit is committed
at `42ac721f`. The source-bound comparison queue at `b4a44e52` has started the
existing comparison across main/counting-disabled/counting-enabled: 180 short
smoke processes followed, if successful, by 540 exploratory measurement processes.
The current pre-change proof, installation, all 120 baseline outputs and exact
19 changed source paths have been rechecked. This evidence is separate from the
preliminary aggregate matrices; installed correctness is accepted and actual
costs remain pending.

The verified counted composition is now integrated on main with its existing
counting switch default-off. All 295 production inputs match the accepted source
exactly; fresh main floor suites pass 218 tests per profile (15 ignored), recorded
in `74y-main-integration.json`. Public ABI, canonical artifacts, compiler,
modules and the held Error declaration are unchanged. The complete 180-process
comparison smoke passed and the 540-process exploratory measurement is running
against frozen installations. Integration does not promote counting or complete
the numeric/aggregate families; default-path cost and broader applications remain
open. See [74Y](74y-production-composition.md) for the source transfer and evidence.
For the original pre-quiet composition, the remaining release V2/loop/closure and
240 normal/test-floor driver checks passed and were audited at `d5651ea3`. The release aggregate matrix is the
only unfinished part of this composition's correctness acceptance. Complete
acceptance still requires all 1,848 focused cases and the normal driver;
neither timing nor integration is admitted yet.

A fresh pre-change main installation and all 120 matching ordinary Id checks
are complete at `241b946d`. The direct comparison runner at `f5d46b08` will compare
main, candidate counting disabled and candidate counting enabled using identical
programs and rotating order. Its incomplete-gate rejection is checked; its real
execution smoke and timings must wait for candidate acceptance. The research
480-process timing/audit at `16b234f6` applies only to its own checkpoint and
cannot qualify the new composition or identify disabled-path overhead.

Two isolated cost experiments preserve that composition's frozen sources. The
String-cleanup candidate on `codex/74z-inert-string-cleanup` recognizes only the
exact floor cleanup and its actual storage-owning public Runtime. Its independent
production gate passed all 75 commands, 3,164 optimization source cases and 24
evaluator/storage cases (`4c3c54c3`); fresh-prefix debug numeric60 and ordinary
Id120 checks also passed their audits (`2ab45fae`). All 240 fresh-prefix debug
aggregate cases then passed in 120 matched pairs without retries; the original
identity, effect, allocation and collection comparisons were independently
audited at `0ac76e5c`. All 924 debug focused cases and release numeric60/Id120
cases now pass their audits. Release aggregate admission then failed with 78
accepted cases across 100 actual processes and 11 incomparable starting-heap
pairs (`b9a175a6`). A quiet-presentation diagnostic at the failed coordinate
passes 16 processes/eight matched pairs. The unreached release V2/loop/closure
controls and all 240 normal/test-floor driver cases subsequently passed and were
audited at `04f45bcb`; the release aggregate failure remains unresolved in that
checkpoint. The initial quiet composition's full gate failed the nested evaluator
host-output assertion; the terminal proof is retained at `57bcd087`. Its program
exited zero and returned 42, but the shared quiet fixture hid the expected REPL
output. The separate `codex/74z-cleanup-scoped-repl` composition applies the
explicit quiet-caller repair at `aca122e8`. All 12 lifetime cases pass against the
same diagnostic binaries; 50 ordinary/quiet source pairs remain byte-identical
to their respective original fixtures. Its new 295-input production gate passes
all 75 commands and 3,188 source/lifetime/storage cases, independently audited
at `91ed3919`. A fresh normal release prefix is published; the exact 1,848 focused
plus 240 driver acceptance runner is active, bound in `21340ef0`. Performance
and integration remain pending.
The scoped control-read candidate on
`codex/74aa-scoped-counting-control` (`7ba56db1`) reduces 15 environment-control
reads to one within a proved observation-free fused numeric invocation. It
retains live reads between invocations and around observing publication methods.
All 219 runtime tests pass in each profile; its independent production gate
also passes all 75 commands, 3,164 optimization source cases and 24 lifetime/storage
cases with 295 current inputs independently audited (`a6a2e674`).
Installed and direct timing acceptance remain pending. Neither experiment is
integrated or promoted.

The AA composition on `codex/74aa-scoped-control-quiet` applies the same explicit
quiet-caller repair at `dd1bf0b1`. All 12 evaluator-lifetime checks pass with the
original production-proved AA diagnostic floors; 50 ordinary and 50 quiet
generated programs match their respective previously tested versions. Runtime,
compiler, modules and allocation/root behavior are unchanged. The source-bound
acceptance queue (`56e420a8`, 295 inputs) waits for the current String build to
finish before running this composition's own 75-command production gate, fresh
normal installation, 1,848 focused cases and 240 normal/test-floor driver cases.
This is build scheduling, not a quiet-host timing requirement. A failed preceding
build pauses for inspection of potentially shared fixture defects. The queue
does not integrate, promote or time the candidate; all four families remain
incomplete until their broader acceptance requirements are met.

Broader closure bodies now have an executing candidate under
[74AB](74ab-numeric-closure-bodies.md), isolated on
`codex/74ab-numeric-closure-bodies`. Commit `af2aecd1` connects numeric caller
provenance to the exact acquired callee's Portable bytes and warm arithmetic
prediction. It retains a physical callee Frame, original numeric allocations
and action publications, and uses the ordinary V2 return/pin-release path.
The experimental closure flag controls it; the production default remains off.

Commit `6ecce1b0` bounds retained provenance, extends diagnostic allocation
headroom and adds permanent numeric source/control gates. Both profiles pass
214 active runtime tests; compiler checks pass 92 per profile. Release evidence
covers repeated calls and every admitted call recovery point (320 cases), V2
capture/suspend ownership (20), complete automatic-GC boundary comparisons (224,
independently audited), and changed real numeric inputs/arithmetic methods (10).
The initial normal release source matrix also covers tail and ordinary bodies
with literal admission disabled. The 297-input production gate passes all 75
commands and 4,360 source/lifetime/storage rows, independently audited at
`f5458ba2`. Its successor numeric-cost queue has started its own expanded gate;
fresh installation and measurements of that composition and the shared-prediction
descendant remain pending.
A follow-on diagnostic branch, `codex/74ab-numeric-action-proof` at `b4c0f898`,
compares all eight persistent action buffers after each physical numeric callee
return, before its caller can overwrite them. All 80 source cases pass across
both floor profiles, including tail/ordinary bodies, captures, both GC modes and
forced recovery at each call. Caller PCs and live pins agree, and final acquired
leases balance. All 214 release floor tests and 123 selected debug runtime tests
pass. The follow-on `codex/74ab-numeric-costs` branch at `1d0dde8b` also extends
the existing cost fixtures with actual numeric bodies. All 96 counter cases
(48 per floor profile) pass and were independently audited for logical allocation
charges, physical closure removal, completed calls, method replacement/restoration
and measured native-resource stability. All 64 default generated counter and
normal-floor fixtures remain byte-identical. Both timing runners now accept the
numeric body only with its corresponding full correctness evidence.

The 299-input expanded source requires both the action and numeric cost matrices.
A bound queue at `7a533aad` waits for the original numeric gate to finish, then
runs the expanded full gate, publishes a fresh normal release prefix, verifies
90 ordinary-Id driver cases, and runs three alternating exploratory timing pairs
(108 processes). Each stage requires the previous one to pass and current source
hashes to match. The two-batch diagnostic samples do not establish profitability.
Full acceptance, application/resource measurements and profitability remain
required. The candidate is not integrated or promoted.

A further isolated implementation at `b758b0ae` on
`codex/74ab-shared-numeric-predictions` removes repeated prediction work inside
one admitted closure region. Every call still validates its current descriptor,
operands and native arithmetic bindings. Only equal raw input vectors belonging
to the same compiled owner share immutable prediction storage; actual argument
and capture Oops, allocations, Frames and publications remain per-call. No cache
survives the constructor invocation. Changed inputs/Methods and retired callees
are covered by the new unit test.

All 215 floor tests pass per profile, plus 798 focused source cases covering
counters, identities, exact action state, repeated-call recovery, automatic GC
boundaries and V2 ownership. At 32 calls per factory, 256 factory invocations
complete 8,192 optimized calls with 256 prediction-kernel evaluations, preserving
logical charges and 256 physical closure savings. This is a removed-work result;
full production acceptance and direct paired comparison with the unshared
candidate remain required. The direct comparator at `d09d16b8` requires both
current full proofs and all 90 ordinary-Id verification cases per prefix, checks
identical fixtures and revalidates source/artifact bindings throughout. Four
rounds balance both enabled and disabled implementations across process positions
and predecessors; separate paired metrics expose enabled benefit and baseline
cost. Thirteen synthetic harness/schedule checks pass; no real timing follows
from them.

The sharing queue at `84009ad9` waits for the unshared cost run to pass, then
runs its own 300-input production gate, fresh release installation, 90 driver
cases and 288 exploratory comparison processes. Waiting avoids overlapping our
new builds with the unshared timings. Existing queues retain their original
sources. Neither implementation is integrated or promoted.

After that comparison, use the observed execution, GC-inclusive and resource
costs to select implementation changes or admission policy. Close the application,
cold/preparation and lifetime/resource gaps for all four families. Existing
native-unit counts, journal capacity, scratch bytes and process footprint are
partial observations; none alone proves total memory savings or leak freedom.

The integrated-main repeated resource matrix now passes 60 normal-release
processes and 420 checkpoints across three alternating pairs for all five
consumer configurations. Its independent audit checks 30,720 wrapper invocations,
use after eviction, complete coordinates and current source/install bindings.
See [the resource evidence](74t-verification-support-integration.md#repeated-resource-evidence-on-the-integrated-main-baseline).
Whole-process differences are mixed. The admitted loop shape shows higher
enabled footprint in all three pairs, but identical rounded malloc allocated
totals; fragmentation and dirty/swapped totals differ. Shorter diagnostic
controls retain equal private code allocations plus a 400-byte enabled cache.
This does not establish a leak or attribute the footprint to optimizer storage.
Full teardown and general metadata accounting remain open.
Supported cases must pass the acceptance contract below before promotion. None
of the four full families is complete yet.


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

## Counted-index and numeric-closure composition

[74AC](74ac-counted-numeric-closure-composition.md) composes the accepted numeric
closure implementation with counted accounting from main, isolated on
`codex/74ac-counted-numeric-closures`. The source union is 304 inputs with only
the production inventory/validator script requiring a merge. Commit `ef5392a7`
passes 92 compiler and 220 floor tests per profile (15 floor tests ignored).

Initial joint source checks found that the original literal-heavy numeric body
does not exercise counted copy regions. An explicit second-operation variant
without a new literal does: its batch saves 168 index objects plus eight closure
objects while preserving 21,120 logical bytes and returned Cell identities.
`1644bf5d` adds separate counted-index diagnostics and retains exact closure-only
savings assertions. All 40 original source-generator coordinates remain unchanged;
40 quick joint cases pass in 20 directly compared index off/on pairs.

The complete 1,280-process sequence matrix is active and source-bound at
`4d9d4e0c`: both profiles/GC modes, one/two/three/32/33 calls, escaping/non-escaping
and every supported forced exit. Budget, changed-input and capture/suspend joint
matrices, full composed production and fresh installed acceptance remain required
before integration. This composition does not include the separately queued
String cleanup, scoped control reads or shared numeric prediction changes. No
family-completion or timing claim follows from these initial joint checks.

## Reference and scope of precedent

The [Self implementation work](https://bibliography.selflanguage.org/implementation.html)
demonstrates receiver specialization and inlining together with dynamic
invalidation and reconstructed Frames. Its
[dynamic deoptimization work](https://bibliography.selflanguage.org/dynamic-deoptimization.html)
describes recovery information at defined interrupt points. These are precedents
for the approach, not evidence that Birnam already has their machinery or will
reproduce their performance. Birnam's current contracts and counterexamples above
remain the acceptance authority.


The 74AC controls descendant (`f86a61fd`, queue checkpoint `f117cf51`) now requires
joint counted/numeric sequence, budget, V2 and changed-input matrices in its full
production gate. Its 284 focused release cases and 220 floor tests/profile pass;
full production and fresh combined installed acceptance are queued. This source
remains isolated from main until acceptance completes.

The accepted counted composition's exploratory comparison is complete in
`88161ebf`: 540 normal-floor processes, 100 batches and three rotating rounds.
Counting on/off has a +6.3% median across execution case medians (+2.6% including
GC); against the earlier baseline, the enabled candidate is -1.9% and -3.7% by
those same descriptive summaries. All host observations failed admission, so
these are unqualified mixed results, not a promotion claim. The next relevant
cost experiment is the scoped control-read change after its independent
acceptance. Defaults remain unchanged and all four families remain incomplete.


The scoped control-read composition has passed its own 75-command production
gate (3,188 source/lifetime/storage cases, commit `10c2be5f`). Its fresh release
installation is published and focused installed acceptance is running. The
[74AA direct cost comparison](74aa-scoped-counting-control.md#direct-scoped-read-cost-comparison)
is implemented in `e378b9fb` and queued in `a4c71220`: accepted counted versus
scoped counted, counting off/on on both builds, four rotating positions. It
requires complete installed evidence and exactly the three scoped-read source
changes before measuring. Twelve admission checks pass. Actual candidate
measurements, runtime integration and any default promotion remain pending.
