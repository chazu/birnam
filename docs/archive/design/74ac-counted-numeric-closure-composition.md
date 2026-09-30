# 74AC — Counted Frame indices with numeric closure bodies

Status: isolated composition in progress; both development switches remain
independently default-off. No integration or promotion.

The counted-index implementation is integrated on main after its full production
and installed acceptance. The numeric closure implementation at `6ecce1b0` has
passed its full production gate. Their separate proofs do not establish the
behavior of a closure constructor executing numeric callees while both passes
are active. Compose these exact implementations on main `5e667907` and close that
interaction gap before numeric closures join main.

## Composition and invariants

Carry the numeric compiler plans, exact callee provenance, checked warm entry
prediction and physical callee Frame execution from `6ecce1b0`. Keep main's
counted-index ledger, numeric-journal reverse lookup and quiet aggregate callers.
Their only shared source edit is the production inventory/validator script;
merge its input lists and checks without dropping either acceptance surface.
Check the exact union of both source inventories and keep ABI, canonical
artifacts, modules and the Error implementation unchanged.

Every logical allocation charge, collection threshold, collection/finalizer
observation, exposed identity and shared Cell must retain canonical meaning.
The numeric closure path must retain real callee entry leases, physical Frames,
original action publication and V2 return/recovery. Counted indices remain
already-dead, unexposed canonical slot operands; they cannot stand in for numeric
results or a closure identity. Baseline and observing/escaping paths remain
ordinary execution. Both controls must remain independently switchable.

## Execution and acceptance

1. Compose the verified sources and audit their union. Run debug/release floor
   and compiler suites on the actual combined source.
2. Add an explicit counted-index option to the numeric closure source, sequence,
   budget, input and capture/suspend checks. Their ordinary default inputs must
   remain byte-identical. Verify counting off/on with closure baseline, enabled
   and forced recovery, both GC modes and every supported call exit.
3. Keep physical savings attributable: closure probes currently report every
   deferred allocation as `physical_saved`. Counted indices need a separate
   diagnostic count, so original closure-savings assertions remain exact after
   subtracting only independently checked index savings. Do not weaken checks
   by merely allowing a larger total. Preserve original logical charge and
   collection comparisons, including budgets and stale/changed methods.
4. Make the joint numeric closure matrices required by this composition's full
   production gate. Then publish a fresh normal prefix and run counted installed
   acceptance and ordinary numeric-closure driver checks against that exact build.
5. Only complete correctness permits integration with defaults unchanged.
   Application, resource and exploratory profitability evidence are still needed
   before default promotion or completion of either optimization family.

The String cleanup and scoped control-read changes are absent. The shared numeric
prediction descendant remains on its independent acceptance/cost path; introducing
it into this composition requires the same joint checks. Existing frozen jobs
and installations remain intact. No qualified timing is claimed.


## Initial composition evidence

The composition preserves the exact union of both production inventories: 304
inputs. All 27 numeric-side source changes and all 19 counted-side paths retain
their accepted contents except the deliberately merged production inventory and
validator script, their only intersecting edit. `74ac-composition-source-audit.json`
records the source union and both original production proof hashes.

Fresh debug/release runs each pass 92 compiler tests and 220 floor tests, with
15 floor tests ignored. `74ac-composition-tests.json` retains the command output
and exact source bindings. This establishes that the two implementations build
and retain their existing individual tests together. Joint enabled source,
accounting, GC and V2-control checks remain the next required work.


## Joint counted sequence

The first eight source observations show that simply enabling both flags does
not prove both mechanisms execute: the original two-operation body introduces a
new numeric literal in each PC, preventing counted copy-region admission. A
second eight-case variant uses `first + first` for its second operation. Its
allocation-free copy PC admits counted indices. Across both GC modes it preserves
21,120 logical bytes per measured batch and the returned Cell identities;
counting saves 168 index objects, and closure elimination independently saves
eight closure objects (176 total when both are enabled). These are allocation
counts, not a timing estimate.

The sequence diagnostic now reports `counted_indices` separately. The checker
subtracts exactly that independently recorded count before applying every
original closure-savings assertion; it also requires positive index savings in
the explicit `--numeric-copy-region --counted-frame-indices` cases. The new shape
requires `--numeric-body`, and all 40 original generator coordinates remain
byte-identical. The retained red check rejects the old diagnostic floor because
it cannot supply the new counter; that is an instrumentation prerequisite, not
a runtime semantic failure.

The initial release quick matrix passes 40 cases in 20 counting off/on pairs:
three and 33 calls, escaping/non-escaping, precise GC, closure disabled/enabled
and forced exits zero/one/two. Source coordinates, logical bytes, closure hits,
completion/recovery counts and closure-only savings agree between index modes.
The full sequence matrix across both profiles and GC modes remains required,
as do the budget, input-mutation and capture/suspend composition matrices.


The full sequence run passed all 1,280 processes and 640 directly compared pairs,
bound to the 304 source inputs at `1644bf5d`. It covers counting off/on in both
build profiles and GC modes, one/two/three/32/33 calls, escaping/non-escaping
closures and every supported forced exit. Every counting-enabled case records
positive counted indices while preserving logical bytes, closure savings and
recovery counters. `74ac-joint-sequence-full-audit.json` checks the exact source,
floor and installed artifacts, matrix coordinates and paired comparisons.
This completes sequence acceptance for that checkpoint; the later controls
descendant still requires its own combined production and installed gates.

## Joint GC budgets and V2 controls

The controls descendant extends the test-only floor diagnostics with a cumulative
counted-index event counter. The budget probe reports both pending counted indices
and the events during the measured invocation. The former explains physical
savings; the latter proves execution even when collection has cleared the pending
ledger. The V2 probe records events before the actual capture/suspend pause and
after resumption. Neither counter introduces Id allocation, roots, callbacks,
public ABI fields, or production-floor instrumentation.

Two explicit source fixtures exercise a copy PC between numeric operations. The
V2 fixture retains the original expected result 42 and all fresh-result, alias,
Frame and native-pin checks. The changed-input fixture retains shared capture and
argument identity checks and the six observing replacement-addition effects. Its
second operation reuses the current argument: the changed 40/2 inputs therefore
produce 44. The initial fixture mistakenly expected 42; that failed receipt is
retained alongside the corrected run. The runtime required no arithmetic change.

`74ac-controls-focused-audit.json` binds 284 successful release cases to the current
Rust sources and diagnostic floor: 40 V2 cases, 20 changed-input cases, and 224
quick-budget cases. Both runtime unit suites pass 220 tests with 15 ignored. The
budget subset covers precise GC, three/32 calls, escaping/non-escaping closures,
all fixed budget gaps and baseline-derived extent minus/equal/plus eight bytes.
It is not the complete two-GC production matrix. The original budget generator's
176 checked coordinates and the original numeric fixtures remain unchanged.

The full production gate now additionally requires counting off/on in both build
profiles for sequence, budget, V2-control and changed-input matrices. This adds
1,280 sequence, 896 budget, 80 control and 40 input processes (2,296 total) to the
existing matrices. It checks exact coordinate coverage, current generated bodies
and fixtures, common diagnostic floors and available installed-artifact bindings.
It compares every accounting/GC/recovery field across counting modes after
subtracting only independently reported pending-index savings. V2 output is
normalized only after checking its raw counter line and positive work before the
pause. Missing budget prefixes, GC modes or actual counted work reject admission.

The source drivers retain their individual identity, effects, allocation and
ownership assertions. Their existing numeric-only matrices are still mandatory.
`74ac-joint-admission-checks.json` records 22 checker exercises, including an
explicitly synthetic budget extension used only to test the validator. It is not
runtime evidence for conservative GC. Six subsequent artifact-binding checks are
recorded in `74ac-joint-artifact-admission-checks.json`. Full production acceptance,
a fresh combined installation, installed checks and profitability work remain
required; all four optimization families remain incomplete and defaults remain
unchanged.
