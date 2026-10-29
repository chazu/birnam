# Ordinary-slot and String ordering costs

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

2026-10-14. Follow-up to [standalone Cells](standalone-cells.md).

The standalone-Cell step exposed two different costs: String ordering used two
mutable Cells even for tiny comparisons, and each ordinary slot operation crossed
unnecessary temporary-root and name-interning boundaries. This slice addresses
both without adding native Cell policy or changing compiler admission.

## String ordering

`String <` retains its common-prefix bound calculation and delegates to the
ordinary `byteOrderBefore:from:limit:` method. That method carries the index in an
argument, reads left then right byte, sends current equality, and either tail-sends
the next index or returns current byte ordering. Exhaustion compares current sizes.
There are no mutable Cells, repeated index reads, verdict tests or suffix reads.
The bound remains the initial common-prefix bound; it was not changed into a
new mutable-length contract.

This is an authored library algorithm change. It intentionally removes its former
internal Cell sends and their allocations. It does not authorize compiler removal
of arbitrary user sends. Byte access, equality, order, helper dispatch and control
remain ordinary replaceable sends. The helper uses normal resumable compilation;
callbacks can suspend, collect, change subsequent methods or be cancelled. There
are no new replacement checks and no synchronous-only shortcut.

## Ordinary-slot operations

ObjectModel already had a GC-visible, nested temporary-reference inventory for
allocation validation. Its private protection helper now uses that same inventory
for ordinary operations, including slot adaptation, writes, cloning and image
storage. It appends every input before callbacks and removes only its own suffix
on success or error. Root enumeration takes a copied snapshot before invoking a
visitor. No callback occurs between removing a returned reference from the scope
and returning it to its caller.

This removes per-reference Root token acquisition/release and token-vector
allocation. The boxed slot adapter also uses fixed stack arrays instead of two
short heap vectors. ObjectModel retains fixed Symbol identities for `Integer`,
`nil`, `at:` and `wordOf:`; it still resolves the current namespace binding and
sends current numeric conversion and raw slot methods. It does not cache their
values, methods or results, and does not inspect Integer payloads.

The change follows the existing [owner-local lifetime contract](helper-lifetime-and-boxing.md).
Private implementation token inventories and retain/release callback counts are
not preserved; explicit program Root sends keep their meaning. A replaced owner
root provider still has to report its live references, just as before. Allocation
identity, layout/bounds validation and mutation-phase admission remain in place.
No ABI, image representation, bootstrap ordering or GC threshold changes.

## Verification

`StringOrder.lean` proves equivalence of results, callback state and ordered byte
steps between the Cell loop and direct recursion, under the stated canonical
control/index contracts. It specifically covers a decisive false result and
continuing with updated callback state. Negative mutations replay a byte or drop
callback effects. It abstracts the byte step and compiler rather than proving
arbitrary replaced controls equivalent.

Existing owner-root Lean proofs and the six NativeOwnerScopes TLA+ cases cover
reported reachability, nested exit, leaks and immediate return handoff. No new
TLA+ state machine is necessary for the unchanged lifetime protocol.

Executable tests cover current numeric binding/conversion, replaced raw slot
access, precise GC during conversion, recursive entry, exact scope restoration on
success and failure, and reclamation after exit. String checks cover unsigned
UTF-8 ordering, left/right read order, early termination, changed equality/order,
helper replacement, wait/resume with replacement and GC, cancellation, one-step
budgets and 10,000-byte equal/shared prefixes. Existing language, image and native
owner tests provide adjacent coverage.

## Measurement method

`scripts/local-cell-bench.py --baseline-local --extended` keeps the original two
workloads and adds generic slot reads/writes, empty strings, first-byte mismatches
with 128-byte suffixes, 128-byte shared prefixes and equal strings. Every answer
and zero fact growth is checked. Ten warm-up operations and explicit GC precede
timing. The existing timer is Darwin clock ID 16 (thread CPU time, as declared
in the platform SDK), not process CPU time as older reports called it. The
language work here runs on the owner thread. Startup, preparation and post-batch explicit GC are outside the timer;
automatic GC remains enabled. Retained-object counts do not measure allocation
churn or GC time inside the operation.

Diagnostic ablations hold the authored runtime fixed while substituting only the
ObjectModel dylib: baseline plus the new owner measures slot changes alone;
new authored runtime plus the old owner measures String changes alone. The final
candidate uses both and is built normally. Ablation prefixes are measurement
controls, not released installations. Timing runs are sequential and begin only
after validation workloads finish.

## Bootstrap metadata correction found during qualification

The broad native gate exposed an earlier standalone-Cell migration defect, also
reproduced with its baseline runtime: a second literal Module declaration for
`#cell` in `modules.birnam` invalidated source revision registration and changed
the manager artifact identity to `#cell`. Early Cell registration now uses an
ordinary manager binding, leaving exactly one source declaration (`#modules`).
No parser rule was relaxed. The source-loader regression checks recorded source,
the actual compiled manager artifact identity, and Cell's loaded/selfHosted state.
This initializer correction is outside benchmark timers and does not change the
Cell, slot or String operation bodies.

## Qualification boundary

The broad run passed 737 language assertions plus scratch, Reflect, inspector,
Cell retention and Cell-owner fixtures; 26 REPL goldens; and mapped/rebuilt new
and legacy local-Cell images. All 67 native integration binaries passed across
the broad run and its follow-ups. The full native gate uses a diagnostic copy
with TCC resource metrics enabled, as its lifetime assertions require; all
performance prefixes are uninstrumented. An initial attempt with ordinary TCC
could not satisfy the gate's metrics precondition and supplies no acceptance
evidence.

After the Module metadata correction, a fresh production build and startup-image
probe passed, followed by all 22 Module assertions, the source-loader/artifact
identity regression, precise slot-scope tests and the actual authored Cell
programs running with Store absent. ObjectModel's binary and String's source
were unchanged by that correction; the broad language/image suites were not
repeated for the registration-only fix. Lean checked 229 theorems, rejected 51
mutations and obtained all 123 expected TLC outcomes. This is development
qualification using the existing Cargo cache, not clean-cache release approval.
The normal installed version remains unchanged.

## Results

[Source-bound receipt](../measurements/slot-and-string-cost-20261014.json).
Three fresh processes per variant and workload ran in alternating order, without
builds, tests or other profiling alongside the timers. All observations are kept.

| Language workload | Standalone Cells before this slice | Both changes | CPU reduction |
| --- | ---: | ---: | ---: |
| 1,000 Cell create/update/read operations | 822.64 ms | 813.83 ms | 1.1% |
| 1,000 generic slot update/read operations | 742.94 ms | 749.05 ms | -0.8% |
| 500 short String comparisons | 868.39 ms | 490.82 ms | 43.5% |
| 500 empty String comparisons | 535.10 ms | 387.24 ms | 27.6% |
| 500 first-byte mismatches (128-byte strings) | 675.34 ms | 397.98 ms | 41.1% |
| 20 long shared-prefix comparisons (128 bytes) | 1074.85 ms | 474.19 ms | 55.9% |
| 20 equal-string comparisons (128 bytes) | 1076.35 ms | 467.20 ms | 56.6% |

The Cell and generic-slot workload changes are within approximately 1% and their
sample ranges overlap. They establish no clear end-to-end CPU improvement.
The slot-only ablation reduces the old Cell-heavy String workload by 2.6%;
String-only reduces it by 43.4%. Most of the String gain is therefore the authored
algorithm, not the generic owner change.

The isolated adapter probe (`rust/birnam-siphon/examples/slot_cost.rs`) preboxes its
index and holds its inputs live, then sends the actual public `slot:in:` and
`slot:in:put:` messages in a native loop. Numeric conversion, namespace resolution,
raw slot dispatch, validation and owner protection still execute on every call.
It excludes authored caller/loop/frame overhead and must not be presented as a
language-wide or Cell-call speedup. Each process warms 1,000 operations and times
100,000 checked operations per case, using the same CPU clock.

| Isolated operation | Before | After | CPU reduction |
| --- | ---: | ---: | ---: |
| Boxed slot read | 943 ns | 391 ns | 58.6% |
| Boxed slot write | 1273 ns | 416 ns | 67.3% |

This demonstrates a substantially cheaper owner adapter, whose sub-microsecond
savings are small relative to the full authored workload. That makes small
accessor/call execution a remaining investigation target; this slice does not
implement compiler inlining or synchronous-only accessors.

A fresh comparison against the earlier native-local-Cell runtime shows short
String ordering at **729.86 -> 487.92 ms (33.1% less CPU)**, more than recovering
the standalone-Cell String regression. Cell create/update/read remains
**752.07 -> 812.38 ms (8.0% more CPU)** relative to native Cells. Ordinary Cells
retain their ownership and malleability benefits, but this work does not erase
that remaining cost.

All language benchmark runs produced the checked answer, zero new facts and a
22-object post-GC delta. These are bounded workload results, not an application
speedup claim. No user installation was replaced and no commit was made.
