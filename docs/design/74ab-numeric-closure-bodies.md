# 74AB — Numeric bodies in private closures

Status: implementation in progress on an isolated main-based branch. Numeric
closure regions now execute behind the existing experimental closure flag, whose
production default remains off. Full production and performance acceptance are
still required.

The closure consumer currently accepts small callees that return an input,
capture or fresh integer literal. An ordinary arithmetic send makes the body
ineligible. Extend private-closure optimization to bounded numeric bodies using
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
