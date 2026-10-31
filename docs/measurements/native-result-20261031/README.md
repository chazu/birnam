# Native argument conversion and the ordinary result proposal

Baseline `0745e1bf`, 2026-10-31. This work has two separate outcomes: a buffer
cleanup that preserves current behavior, and an ordinary-return proposal awaiting
a decision about an observable reentry difference. The latter is not applied to
production source.

The [current-runtime requalification](../native-result-current-20261031/README.md)
updates the unapplied proposal after later runtime changes. This original receipt
retains its historical baseline and results.

## Implemented buffer cleanup

Incoming native adapters previously built a receiver-plus-arguments Vec for every
bound invocation, a byte-snapshot bookkeeping Vec even when no argument contained
bytes, and a normalized argument Vec that grew from its initial receiver slot.

The implementation now keeps the receiver separate from the existing argument
slice, allocates byte-snapshot bookkeeping only for signatures containing byte
arguments, and sizes the normalized argument buffer once. It still snapshots all
byte inputs before conversion factories run. The same ordered messages perform
normalization, body invocation, result conversion, and failure observation.

Bound and unbound arity rules, nullable inputs, input roots, and context getter
order remain unchanged. No calling convention, continuation policy, native ABI,
language representation, or public message changes. The borrowed slice is the
existing native call's readable argument area; this creates no borrowing rule for
Birnam objects or programs.

The existing [probe](../method-source-investigation-20261030/run.py) was run in
baseline/candidate/candidate/baseline order, with builds outside the timers.
Wrapped-addition medians were 47.78 versus 46.57 microseconds with automatic
collection enabled, and 45.11 versus 44.92 with it disabled diagnostically.
Both variants allocate 50 managed objects per call. These small differences do
not establish a substantial CPU speedup; the concrete simplification is removal
of unnecessary native buffers and reference copying.

Nine focused integration targets pass: native incoming conversion, native
boundaries, fixed ABI behavior, Integer policy and collecting operation
replacement, continuation transparency, Execution methods, root batches, and
root reuse. Added cases explicitly cover unbound adapters' receiver prefix,
missing arguments, and nullable versus non-null arguments. Existing cases check
byte snapshot order across mutating factories, GC, failures, deferred suspension,
and mapped/rebuilt images. This is not full release qualification; the previously
recorded full-image refusal assertion remains unresolved.

## Unapplied ordinary return proposal

The [candidate patch](ordinary-result-proposal.patch), based on `0745e1bf`, gives
ordinary native results a synchronous entry while retaining continuation entry
for deferred branches. It keeps body invocation and conversion messages intact.
Its focused echo test reduces six managed allocations to four, while preserving
Method-as-data returns, arity failures, failure observation during GC, and the
body's suspension barrier. Arithmetic measured 40 rather than 50 allocations in
the exploratory run.

The reentry probe found a meaningful difference before this was accepted:

| Native conversion callback | Baseline and buffer cleanup | Ordinary-return proposal |
| --- | --- | --- |
| Attempts an independent Id drive from a native host call | `driveActive`, because the adapter itself has a private drive | Allowed, because no drive is active |
| Attempts another drive while a real drive is active | `driveActive` | `driveActive` |

The callback is a replacement for Word's `fromBits:` operation. It attempts an
empty drive through the existing public native control interface, records the
outcome, and delegates to the original constructor. A replacement could branch
on this outcome, so the difference cannot be dismissed as merely a timing or
allocation-count observation.

The open question is whether the first refusal is a required part of the native
control contract or an accidental restriction caused by the wrapper. Removing it
would make this case consistent with ordinary native calls, but preserving errors
and replacement effects requires resolving that specific case rather than silently
shipping the difference. The user has been asked; no answer is assumed. Keep the
proposal unapplied until that decision is supplied.

The patch includes the native fixture and Rust regression/probe. Replaying it on
its recorded baseline shows `root conversion drive state=1`; replaying with the
proposal shows `2`. Both enforce refusal of an actually overlapping drive. The
baseline fails only the proposed allocation ceiling after the behavior probes.
The separate buffer cleanup still reports state `1` and six echo allocations,
confirming it has not introduced the pending behavior change.

## Evidence and next work

[result.json](result.json) records summaries, source/library hashes, checks, and
local raw-evidence hashes. Logs and runnable prefixes remain under
`.cargo-target/measurements/native-result-20261031`. The candidate patch is
experimental evidence, not instructions to apply it as part of unrelated work.
It will need reconciliation with the independent buffer cleanup if accepted.

This cleanup is not a substitute for resolving the reentry contract or addressing
the larger arithmetic/control expansion. The overall simplification goal remains
active, and the authored inventory migration remains unqualified on performance.
