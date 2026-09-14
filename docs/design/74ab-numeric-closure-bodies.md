# 74AB — Numeric bodies in private closures

Status: implementation in progress on an isolated main-based branch. Numeric
closure regions now execute behind the existing experimental closure flag, whose
production default remains off. Full production and performance acceptance are
still required.

Before this slice, the closure consumer accepted small callees that returned an
input, capture or fresh integer literal; arithmetic sends made a body ineligible.
This slice extends private-closure optimization to bounded numeric bodies using
the existing canonical numeric dataflow and runtime native declarations. This
is part of the broader-body obligation in 74T; it does not complete that family
or the other three families by itself.

## Required behavior

A source-created, nonescaping closure may contain an admitted straight-line
numeric body. Arguments and captures retain their original identities; captures
are not copied out of shared Cells or treated as constants. Every arithmetic
selector must resolve to the current declared native implementation. Selector
spelling, compiler annotations and prototype names grant no authority.

Preserve overflow, zero division, mixed/error operands and changed methods by
ordinary execution whenever whole-body admission cannot prove success. Keep
source literal identities distinct, including equal integer values, and retain
all original numeric allocations for the initial consumer. The optimization
target is closure construction and eligible invocation overhead. Unboxing is a
separate consumer with its own recovery and observation contract.

## Compiler description

Derive an owned numeric-body description from canonical PortableProgram lowering
and canonical slot coloring. Reuse numeric `Template` dataflow and
`RecoveryMaps`; do not duplicate arithmetic rules or infer operators from names.
The initial numeric template supports two through 32 sends. Single-send and
unsupported bodies retain ordinary execution; extending that existing template
must not silently broaden other consumers' admission policies.

Record the full ordered integer-allocation description, including each driver
PC token, local index box, literal occurrence, call-window staging index,
publication operand and successful arithmetic result. Record each original
Call, TailCall, Continue and Return publication with its canonical PC and
operand. A return names either an original argument/capture or one particular
allocation occurrence, never a numeric value used as an identity key. Refuse
receiver/implementation exposure and layouts that differ from canonical coloring.

This description contains no Oops, runtime addresses, public ABI fields or new
image format. Its existence does not admit execution. Unit verification must
cover source provenance, distinct equal literals, earlier-result returns,
argument/capture aliases, both tail and ordinary returns, malformed layouts,
observing bodies and receiver exposure. Compare the allocation description to
canonical generated C and numeric-template allocation totals.

## Runtime integration

Retain existing constructor, CodeObject/capture layout, materialization owner,
callee registration/generation and current Method checks. Admit native numeric
operations through the existing declaration and API-context checks, with native
owners pinned through use. Whole-body allocation/collection and control admission
must precede the first omitted constructor allocation or changed Frame.

The existing leaf shortcut cannot simply return a computed numeric result.
Numeric bodies publish internal actions, and the Machine retains reflective
Call/TailCall/Return/Continue scratch state after a callee returns. Canonical
Frame transitions also have lifetime and generation duties. The runtime path
must preserve those states and existing V2 copies/clears, or retain the physical
callee Frame and original transitions. No action observer, GC, pending retirement
or capture/suspend request may see a placeholder receiver. Recovery must restore
the one original closure identity and every live alias before such exposure.

Keep the public IdApi/Method ABI and canonical descriptors/images unchanged.
Failing admission leaves original constructor and execution untouched. After
admission, a failure belongs to this path and cannot replay original effects.

## Acceptance

First complete the compiler description and differential allocation tests.
Then implement and test runtime admission/execution, with exact reflective action
state and Frame lifetime comparisons alongside values and identities. Exercise
changed arithmetic and invocation methods, collecting observers, both GC modes,
all supported forced exits, live API wrappers, eviction/retirement, and V2
capture/restore before running the normal installed source consumer.

Production verification, application benefit and cold/steady/resource evidence
remain necessary. Existing alias/literal closure controls must still pass. Any
timing under the quiet-host waiver is exploratory and `qualified: false`.

## Compiler description delivered

`closure_elision::numeric_body::NumericBody` now combines existing numeric
dataflow/recovery maps with ordered allocation occurrences, original publication
PCs/operands and exact return identity. The missing-feature regression first
failed at the new description seam while confirming existing leaf admission
refused the numeric body (`74ab-numeric-body-red.json`).

The implementation passes 89 default debug codegen tests and all 90
feature-enabled codegen tests in each profile. The retained compiler receipt
binds 49 source inputs. Its new tests compare all index/literal allocations to
canonical emitted C, check input/capture and earlier/final-result returns, retain
distinct equal literals and signed-extreme literal occurrences, and reject
receiver exposure, observing operations, backedges and noncanonical layouts.
Description allocation totals also match the existing numeric template, including
the entry driver token.

The compiler description itself grants no execution authority. Runtime admission
and the executing consumer are described below. Existing leaf eligibility,
runtime defaults, the floor ABI and serialized descriptors/images are preserved.

## Entry prediction delivered

Runtime preflight can now predict from explicit entry Oops before installing a
callee Frame. It uses only an existing warm numeric candidate, verifies current
native declarations and controls, and leaves Birnam allocations, source method
execution, hotness and loop history untouched. Each prediction owns its raw
buffers and compiled numeric unit; it does not acquire callee execution authority.

The regression covers both GC modes and tail/ordinary returns, cold refusal,
invalid/duplicate/missing inputs, mixed operands and overflow, changed Methods,
live native declarations, ErrorVt rebinding, observing lookup refusal, concurrent
prediction storage and generation retirement after normal Method withdrawal.
All 213 active runtime tests pass in each profile (15 diagnostic tests ignored);
`74ab-numeric-entry-runtime.json` retains sources, full logs and prior failed
fixture attempts. This prediction is a prerequisite of the executing consumer
below; it cannot authorize constructor omission on its own.

## Numeric caller regions delivered

`NumericPlan` now uses the same canonical caller provenance, capture ordering,
constructor stores, invocation aliases, repeated-call bounds and V2 continuation
analysis as the leaf plan. Its distinct callee type contains the numeric body
description; existing leaf execution cannot accept it accidentally. The default
`Plan::derive` retains its previous eligibility and representation.

The region tests cover 1/2/3/32/33 calls, tail and ordinary numeric bodies,
argument/capture and earlier/final numeric returns, live closure aliases at
recovery successors and noncanonical layouts. A subsequent call depending on a
fresh result ends the current region; entry aliases may continue. Such a stop
retains the original continuation and closure recovery aliases. These are
compiler eligibility rules, not language restrictions.

All 92 feature-enabled compiler tests and seven runtime closure tests pass in
each profile; `74ab-numeric-closure-plan.json` binds the tested sources and logs.
Numeric-body/template structural equality includes allocation occurrences and
publication/recovery metadata, but equality alone grants no execution authority.

The executing consumer below connects this plan to exact acquired callee
provenance and physical Frame execution. It retains original actions, allocations
and generation pins; the leaf return shortcut is insufficient for numeric bodies.

## First executing numeric consumer

The constructor continuation now admits a numeric plan under the existing
experimental closure flag. It compares the actual acquired callee's retained
Portable bytes, including captures and literal occurrences, and predicts all
arithmetic before charging the omitted constructor. Its numeric candidate and
raw prediction remain owned throughout use. The selected acquisition transfers
to a physical callee Frame; existing fused execution performs original PCs,
allocations and action publications, then the normal return path applies V2
copies/clears and releases the Frame pin. Original constructor recovery remains
available before each admitted call. The production default stays off.

The fresh release installation passes 20 source cases spanning tail/ordinary
bodies, both GC modes, three repeated calls, fresh identities, collecting escapes,
eviction, Method replacement and forced recovery. The release test floor passes
320 repeated-call/accounting cases (1/2/3/32/33 calls, escapes, every admitted call
exit and both GC modes) and 20 fresh-result V2 capture/suspend cases with source
aliases, supplied results, GC and generation-pin comparisons. Both profiles pass
213 runtime tests and 92 all-feature compiler tests. Receipts are indexed by
`74ab-numeric-wiring-focused.json`.

Two test-setup failures are retained. The shared fresh-result V2 probe expects
its literal mode flag; the numeric matrix now selects that mode while normal
source testing independently checks literal-flag independence. The 32-call numeric
allocation extent exceeds the budget fixture's original 16KiB first gap; the
64KiB setup is also rejected by the test floor's old input cap. The bounded diagnostic
cap and retained callee-source storage are addressed below, with complete numeric
budget traces. Exact action-state checks and full production acceptance remain
required. No timing or default-promotion claim follows.

## Bounded provenance and production-gate coverage

Retained callee Portable bytes are capped by the compiler's 64KiB program limit
and exist only when the closure consumer is enabled. Preparation and the compiled
candidate share one owned byte buffer; a runtime test verifies that the candidate
retains exact provenance after preparation storage is dropped. All 214 active
runtime tests pass in each profile, with 15 diagnostic tests ignored.

The test-only budget input cap is extended to 64KiB; the production collection
threshold and root scanning are unchanged. The complete 224-case numeric budget
matrix passes, independently audited as 112 baseline/enabled pairs with identical
collection bytes, PCs, slot counts and remaining charges. Every 3/32-call and
escape/GC configuration covers refusal, a shorter admitted prefix and the whole
region. Prior setup failures remain in their original receipts.

Ten additional source cases use actual captured integers and call arguments,
change both after warmup, verify fresh results survive collection, and replace
arithmetic with an observing method. Both ordinary and optimized executions
perform exactly six replacement effects and retain the returned capture identity.

The regular production gate now requires numeric source12, V2control20,
sequence320, budget224 and input10 cases in each profile, with complete coordinate
and actual numeric-body source checks. Its validator rejects missing/duplicated
sequence cases, missing V2 cases and wrong source hashes. Full gate execution,
exact residual action-state comparisons, application/resource measurements and
profitability remain pending; none of the four families is declared complete.

## Residual action-state comparison

The diagnostic floor now snapshots the eight persistent Machine action buffers
immediately after a numeric callee returns, before its caller publishes another
action. This uses the common physical Frame return path. A shortcut that only
manufactured a result would miss that return event or leave different action
contents. Each snapshot validates the actual action vtable and full payload
length. Published Machine capability fields are checked against their owner
before normalizing only those two identity words for comparison across processes;
publication epochs, all payload fields and untouched buffers remain exact.

The source matrix covers ordinary and tail arithmetic returns, literal and
captured operands, both GC modes, optimization disabled/enabled, and forced
recovery before each of three calls. All 40 cases pass with each diagnostic floor
profile. Caller continuation PCs, slot counts and live Frame/continuation pins
match. Final acquired-entry counts return to their baseline. Intermediate entry
acquisition counts intentionally differ because region admission acquires the
selected calls together; virtual closure slot contents are tested by the existing
identity/recovery matrices rather than compared as raw physical Oops here.

The probe holds only scalar metadata and introduces no Id allocations, roots or
callbacks. Its hook and exported method exist only with `ffi-test-hooks`. The
comparison uses the existing published compiler/host with an explicit newly built
floor overlay; this is not a fresh production-installation proof. The first run
omitted `BIRNAM_TEST_CLOSURE_CONTROL=1`, so the method was not installed and the
source exited 61 before reaching the probe. That setup failure is retained in
`74ab-numeric-actions-release.json`; the corrected release and debug receipts
record all 80 cases. No numeric execution counterexample was observed.

The production gate now requires these 40 coordinates in each profile and
checks their generated body hashes, complete action-buffer shapes and exact
baseline equality. A pure validator exercise rejects missing/duplicate cases,
a different source body, a missing buffer, a changed publication epoch and
changed live pins. Full acceptance of this newly instrumented source, resource
and application measurements, and profitability remain pending. This closes
one explicit state-equivalence evidence gap; no family is declared complete.

## Numeric closure cost fixtures

The existing repeated-closure cost probe now has an explicit numeric-body mode.
For the private, escaping and replaced-method shapes, each callee performs two
ordinary integer additions and then returns its captured Cell. This preserves
the existing identity assertions while measuring closure and invocation overhead
around a real admitted numeric body. The fourth shape remains an observing
callee refused by the compiler. Uses range over 1, 3, 4, 8, 16 and 32; each shape
runs with the closure pass disabled and enabled. No runtime code changes are
needed for this measurement extension.

The probe requires equal logical allocation bytes, one physically removed
closure per private factory invocation, exact optimized-call counts, stable
private native units and relocated bytes through collection, no new warmed
preparations, bounded pending records, and every replacement-method effect.
Restoring the original method must regain admission on the first subsequent
call. These checks are retained unchanged for numeric bodies. The counters do
not prove total memory savings or absence of leaks outside their measured state.

The production gate now requires a complete 48-case numeric cost matrix in each
profile and checks the actual generated source hash and all counter comparisons.
Its validator rejects missing or duplicate coordinates, an unrelated body,
missing allocation charge, absent optimization hits and lost replacement effects.
Both the diagnostic and normal-floor timing runners accept `--numeric-body` and
require the corresponding complete cost matrix in a current full production
proof. Normal-floor timing additionally retains the existing 90-case driver
verification, source/install binding, silent timing, alternating order and
separate execution/collection/preparation/process measurements.

All 32 default counter sources and all 32 default normal-floor sources remain
byte-identical to their prior versions. Numeric fixtures are distinct and are
recorded by hash. The source/prefix/floor overlays and both-profile counter
receipts are retained separately from production acceptance. Two-batch smoke
samples remain unqualified diagnostics; their variation is not evidence for a
profitability threshold. Complete the expanded production gate and fresh normal
release verification before the paired exploratory timing run. The user's quiet
host waiver does not bypass those correctness prerequisites.

## Experiment: share pure predictions within one admitted region

The current admission loop evaluates the raw kernel once per selected call and
retains a separate leased input/output buffer for each prediction. Repeated calls
with the same current compiled chain and numeric operand bits can instead share
one immutable prediction. The reuse scope is the existing at-most-32-call
admission vector; no cache survives the constructor invocation.

Every call must still validate the current Machine controls, descriptor record,
exact callee provenance, entry acquisition, capture layout, current Integer
storage/operand bits, ErrorVt binding and native arithmetic declarations. Reuse
is considered only after those checks. Its key is the actual owned compiled chain,
descriptor identity and full ordered numeric input vector. Equal numeric values
may share speculative arithmetic results; original argument/capture Oops, fresh
integer allocations and each call's physical Frame remain distinct and unchanged.
A different input or compiled owner evaluates a fresh prediction. A changed
binding refuses admission even if a matching earlier prediction exists.

Use `Rc<Ticket>` only for this read-only numeric closure consumer. The ordinary
per-call prediction cursor and loop-reuse path retain their existing owned-ticket
behavior. Keep buffer pooling bounded, retain native owners until the last shared
prediction is released, and preserve every numeric action publication and logical
allocation. Diagnostic counters must show one entry-kernel evaluation per private
or escaping factory invocation when all selected calls have identical operands,
while all current action/identity/GC/recovery/counter checks continue to pass.
Compare this isolated experiment with the unshared candidate after correctness;
no speedup, threshold or default promotion follows from eliminating repeated work
alone. Existing production/cost queues keep their original source checkpoints.

The isolated sharing implementation passes 215 floor tests per profile. The new
unit test covers equal values in distinct Integer objects, changed numeric bits,
changed/restored arithmetic Methods, another compiled descriptor owner, and
retirement while an earlier shared prediction remains alive. The common entry
preflight test continues to reject cold, malformed, observing and stale contexts.

All 798 focused source cases pass: 710 release cases cover ordinary/numeric cost
counters, exact action state, real inputs, every repeated-call recovery point,
automatic-GC boundaries and V2 capture/suspend ownership; another 88 debug cases
cover numeric counters and action state. At 32 calls per factory, 256 factory
invocations complete 8,192 optimized calls using 256 entry-prediction kernel
evaluations. Logical allocation charges remain 4,751,360 bytes for that fixture,
with 256 physically eliminated closures. These are removed-work counts, not a
speedup estimate. The source-bound audit also rejects the former 8,192-kernel
count for that coordinate. Full production acceptance and the direct paired comparison are recorded below.

## Direct comparison with the unshared implementation

`closure-prediction-comparison.py` compares independently verified unshared and
shared normal release prefixes using identical numeric source fixtures. Both
versions run with the closure pass disabled and enabled. Over four rounds each
mode occupies each process-order position once, and each ordered adjacent pair
occurs once; another four rounds reverse that schedule. This exposes default-path
cost separately from the enabled sharing change. Execution, execution plus GC,
cold execution, materialization, whole-process time and peak RSS remain separate
metrics, with per-round observations and paired reductions retained.

Admission requires each version's current full 75-command production proof,
complete 48-case numeric cost matrix, published release artifacts and shipped
sources, and all 90 ordinary-Id verification coordinates. The runner rechecks
the actual verification output for results and exact hit/forced-recovery reports,
requires identical generated fixture hashes, and binds both source trees,
installations and proof files throughout the run. Timed processes have neither
probe floors nor diagnostic reports. The existing host gates remain available;
`--exploratory` retains unqualified measurements under the user's quiet-host waiver.

Thirteen synthetic admission/parser and schedule checks pass, including changed
sources/artifacts, missing production/cost cases, stale driver evidence, duplicate
coordinates, missing forced-recovery reports and unexpected timing diagnostics.
These are harness checks only; they contain no real program execution or timing
claim. The full sharing gate now binds this runner as its 300th source input.
The prior 798-case focused proof remains tied to the unchanged runtime and source
fixtures before this measurement-inventory extension.

The unshared cost queue completed before the sharing gate and fresh normal
release installation. The sharing queue has now completed production verification,
90-case normal driver verification, and four direct comparison rounds (288 normal
processes, 100 batches each). Neither integration nor promotion is automatic.

## Completed shared-prediction acceptance and exploratory costs

[The completion audit](../measurements/74ab-shared-prediction-completion-audit.json)
revalidates all 300 source inputs, 75 successful production commands and 4,536
source/lifetime/storage cases. Both compared release installations are published,
match their shipped sources and proofs, and pass their 90 ordinary-driver
coordinates. All 288 comparison outputs parse successfully, with exact balanced
ordering and fixture hashes. The audit recalculates 864 paired metric reductions
from the recorded raw outputs and checks queue logs and current installed files.

For each shape, the following values summarize the six repeated-use case medians
(1, 3, 4, 8, 16 and 32 calls). Positive means the shared version used less time;
these values describe this run and do not isolate a causal implementation effect.

| Closure pass | Shape | Execution | Execution plus GC |
| --- | --- | ---: | ---: |
| Disabled | Private | -4.13% | -5.81% |
| Disabled | Escaping | +1.42% | +6.57% |
| Disabled | Refused | +18.86% | +17.08% |
| Enabled | Private | +5.43% | +7.83% |
| Enabled | Escaping | -0.30% | -0.05% |
| Enabled | Refused | -10.93% | -16.29% |

All 72 host intervals failed qualification. The disabled/refused controls vary
substantially, and only two private execution cases improve in all four pairs.
The implemented removal of duplicate prediction evaluations is verified, but
these costs do not establish a general speedup or justify a default/threshold
change. The four optimization families remain incomplete. This candidate's
acceptance also excludes the subsequent combined Unit ownership and counted
closure/String composition in 74AG; that work has its own source and gates.
