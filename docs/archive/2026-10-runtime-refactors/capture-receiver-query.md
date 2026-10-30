# Query capture borrowing on the receiver

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Follow-up: [fixed names and local protection](fixed-names-and-local-protection.md)
reduces successful addition further to 514.375 sends. Measurements below describe
the preceding receiver-query slice.

Implemented 2026-10-08. Querying the receiver directly removes the ordinary
Namespace/Block factory lookup from generated synchronous capture loading.
Successful Integer addition falls from 717.375 to 626.375 sends, a 12.7% reduction.

## Change and contract

The compiler sends `captureReadMode` to the Block it is about to read. Execution
installs this owner query on the common Object behavior: Blocks inherit it and
other ordinary Objects answer mode 2 without invoking DNU or interpreting any
foreign representation. Execution checks its own initialized portable Block
record, then asks Id's atomic
`nativeBindingsMatch:` query to verify both the current mode query and current
`captureWordAt:` reader, including the canonical query services and the binding
check's own entry/context. A forwarding query wrapper therefore declines borrowing.
The entered query already carries its exact owner-local context; it uses that
identity as its witness instead of scanning all installed operations to find it.
The witness is compared inside Id, never called as a saved foreign executable.

Mode 1 means callback-free reads of immutable captured references covered by the
live receiver. Mode 0 means ordinary current reads with protection for each result,
including fresh replacement results. Actual reads remain Id sends. A positive
grant is local to one capture-loading batch; it is not cached across invocations
or callbacks. No foreign representation read or saved executable is introduced.

Mode 2 uses owner inspection for nonportable receivers. When the receiver query
returns native UNBOUND, generated code resolves the current Block factory and
sends `captureReadModeOf:`. That retained query is a real type/inspection fallback:
it still checks the current direct reader for portable Blocks, so removing a
direct query cannot bypass a replaced reader. Factory lookup happens before
admission, and its Object stays protected through the fallback operation.
Other query failures propagate; invalid native carrier/mode replies are rejected.

The factory query remains for this live fallback, not solely for older artifacts.
Factory-query replacements no longer intercept the normal Block path. Direct
query/reader replacements do. This is an explicit compiler/library interface
simplification, not a transparent omission of sends from unchanged source.

Rust needed for cold bootstrap remains. Image startup continues mapping supplied
machine code; explicit artifact rebuilding is separate from image loading.

## Formal correspondence

`formal/rooting/lean/Capture.lean` proves that a transitively rooted receiver
covers its capture edges and keeps a pure read batch alive without separate
result or factory roots. Its admission lemma requires current reader, query and
query-service meanings. Reachability, stable edges/scope and pure current reads
are explicit premises, not inferred from a native pointer or nonescape alone.

`CaptureReads.tla` includes direct and factory-query fallback routes for portable
Blocks. It does not model non-Block inspection or its general dispatch. The
direct route allows collection of the unused factory; fallback roots it through
the query.
Two invocations change reader/query meanings before the second admission. The
safe model survives; `CaptureCached` must expose an unrooted fresh result when a
previous grant is reused. Existing bad-grant, fresh-unrooted, early-exit and
mutated-edge cases remain required counterexamples. The Lean checker also rejects
a mutation that ignores the query's binding.

These are abstract lifetime/admission proofs and bounded transition checks. Id
dispatch, native return handoff, DNU/custom lookup and arbitrary provider code
are not fully verified by them. Production tests supply the correspondence:
pure reads under failed allocation admission; replaced readers allocating fresh
Objects and collecting on the next read; query/proof wrappers replacing readers;
unbound-query fallback without reader bypass; malformed replies and exact
failures; receiver-only capture reachability; and non-Block Method invocation.

## Measurement and validation

[Receipt](../../measurements/capture-receiver-query-20261008.json) records source,
library, trace, timing and validation hashes. Successful public Integer addition:

| Per addition | Before | After |
| --- | ---: | ---: |
| All sends, including amortized fixture work | 717.375 | 626.375 |
| Capture access | 174 | 94 |
| Other lifetime/cleanup | 93.25 | 82.25 |
| Closure construction | 160 | 160 |
| Logical root registrations | 112.0625 | 96.0625 |
| Id Objects allocated | 20 | 20 |
| Branch Blocks constructed / invoked | 8 / 4 | 8 / 4 |

The 91-send saving is 80 factory-lookup/preparation sends plus eleven cleanup
sends. Sixteen redundant factory-root registrations disappear; tracked tokens
return to zero. This is **12.7% fewer total sends** and **46.0% fewer capture-access
sends**, not the theoretical minimum or a CPU-share claim. Actual capture reads
remain 62/addition, with sixteen query/admission pairs.

Four-second CPU sessions observed median per-addition costs of 72.56 -> 69.70 us
and 76.28 -> 67.39 us, approximately 3.9% and 11.6% lower. These are separate-version
observations on an active host, not within-process paired version controls or a
confidence-qualified speedup. Each version's two default variants have the same
compiler mask; the harness's within-run verdict compares those equal controls,
not before versus after. All final timing/census input checks were stable.
The two-second baseline profile points to Id lookup, owner hashing and root
bookkeeping; its leaf samples do not provide disjoint closure CPU shares.

Early authored artifacts were explicitly rebuilt with the retained Rust grammar
seed in a temporary release installation. The final receiver-query placement and
context-witness cleanup then refreshed only the native Execution owner in the
marked diagnostic prefix: compiler output and early artifacts were already current.
The published user installation was not replaced. The temporary mixed-artifact
census is diagnostic only and is excluded from these before/after numbers.

Passed 29 Execution/backend Rust tests and seven native integration binaries:
capture access, closure construction, Integer policy, Word helpers, invocation
inputs, incoming ABI and materialization. The independent signed Integer oracle
covers bounds/errors/mixed Float arithmetic and reload/collection behavior.
The small task probe passes fourteen assertions at quanta 128 and 1, preserving
captured identity/shared Cells across wait/resume, GC and cancellation/cleanup.
It runs from the diagnostic prefix with the saved bootstrap/tool images present.

The formal checker verifies 104 Lean theorems, 45 TLC configurations and all
required mutation/counterexample rejections. No new whole-runtime, startup-speed,
full test-suite or cold-cache build qualification is claimed by this slice.

## Next target

This focused performance round is complete; small-module migrations can resume.
The next performance target is the remaining branch construction. Four
of eight branch Blocks are uninvoked on successful addition. Their identities,
private vtables, replacement constructor effects, shared Cells and supported
Frame/continuation observation must be accounted for before deferring construction.
The count is an opportunity, not an allocation-elimination proof.
