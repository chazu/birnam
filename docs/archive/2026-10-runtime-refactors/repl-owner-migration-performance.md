# REPL ownership migration: qualified

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-19, baseline `3d45e084`. The user approved the one-time initialization remediation below; it is implemented and qualified.

## Ownership and behavior

REPL session state is an ordinary shared Cell containing borrowed input/error
streams and an owned read buffer. Teardown conditionally retires that group before
freeing the buffer. Clones share the owner; reentry cannot free it twice and
cleanup replacements survive. Owner-level image projection clears process-local
values while preserving owner aliases. A separately retained live stream still
refuses image saving.

The prior code called `fdopen(0, ...)` and `fdopen(2, ...)` every session without
closing the resulting FILE objects. The draft instead reads the process's current
stdio pointers through ordinary Alien calls: `dlopen(NULL, RTLD_LAZY)`, `dlsym`,
`peekPtrAt:`, then `dlclose`. Darwin's `__stdinp`/`__stderrp` names are tried first,
with `stdin`/`stderr` fallbacks. The target actually tested is macOS; the fallback
is not a Linux qualification claim. No new native code or private payload access
is added.

The REPL owns only its read buffer and never closes standard streams/descriptors.
`clearerr` resets EOF/error flags at session start. libc buffering is retained:
read-ahead input after one session remains available to a subsequent session,
instead of being stranded in the former abandoned FILE object.

Passed checks:

- Actual REPL initializer and message-based buffer cleanup with Store, Relational,
  Bootstrap, Alien, Image and Isolate libraries physically absent. This component
  test does not claim real stdio can operate without the Alien module.
- Real stream descriptors stay 0 and 2 through cleanup; repeated initialization
  retires the prior buffer; aliases/reentry free it once; replacements survive.
- Repeated sessions read two supplied lines in order, retain unread input, and
  handle EOF; failed setup leaves no partially installed session.
- Mapped/rebuilt images preserve owner aliases with reset state, then open a fresh
  console. Retained-stream refusal preserves an existing destination.
- Installed tooling/image/interaction checks, persistent TCP sessions, bind
  failure, published project tty/socket execution and main-once behavior.
- The Host test that forbids compilation/parsing during installed console startup.

The first build caught a shadowed local variable in the new symbol helper. That
implementation error was fixed before the successful build and checks above.

## Regression and isolation

Three interleaved standard-release observations, with normal GC:

| Twenty session setups/teardowns | Baseline | Draft |
| --- | ---: | ---: |
| CPU median | 4.902 s | 7.824 s |
| Retained facts added | 3 | 0 |

That is **59.6% more setup CPU**. Each session calls `wrapStdio`, reinstalling all
wrappers. The new implementation has seven wrappers rather than four, amplifying
existing repeated setup work. This is not repeated C compilation: the installed
no-compilation gate passes.

A separate diagnostic first installs the real wrappers, then overrides only a
cloned session's `wrapStdio` method with a no-op. All real stream lookup,
allocation and cleanup remain active. The same batch takes **15.30 ms before and
31.91 ms after**. Thus repeated wrapper setup dominates both ordinary timings.
The new borrowed-stream lookup has a small absolute cost of its own; the
experiment does not claim it became free or that a production fix is delivered.

## Implemented remediation

Initialize wrappers once per shared REPL setup state, using an ordinary owned
readiness value. The compiled startup image already prepares these wrappers;
a source-only first session would initialize them when the value is false.
Keep explicit `wrapStdio` available for deliberate reinstallation, and mark setup
ready only on successful initialization. Share readiness across clones and retain
it through images whose wrapper code is already restored.

Subsequent sessions should use the currently installed methods. Do not compare
method identities, cache native entries, or silently reinstall methods after a
user replaces/removes them. This also stops automatic session setup from undoing
intentional user edits. It is an explicit library initialization-policy change,
not a compiler assumption that methods cannot change.

Qualify source-first initialization, installed/image startup, shared clones,
failed setup, explicit reinstallation, replaced wrappers surviving new sessions,
existing interaction behavior, and cold/warm timings. Commit only after those
checks pass without a significant regression.

Evidence and exact source hashes are in the
[receipt](../../measurements/repl-owner-regression-20261019.json).


## Recovery finding during remediation

A new replacement test reproduced an existing Image defect: after installing the
saved authored method bindings, recovery replayed every Alien recipe, overwriting
an authored replacement of the same target/selector. Readiness itself restored
correctly, but the saved replacement was lost before the next console session.

The fix excludes recipes shadowed by an actual saved authored binding. This is
snapshot reconstruction, not a runtime method-identity guard or automatic repair.
Untouched foreign wrappers still restore; deliberately reinstalled wrappers must
also survive. Mapped, rebuilt and baked recovery checks cover those cases.

The upcoming Alien slice must distinguish cached function/signature object code
from active target/selector registrations. The existing recipe catalog also keeps
multiple different functions registered to the same target/selector and has no
explicit deletion representation. Audit latest-wrapper selection and removed
wrappers during that slice; this finding does not establish that those cases are
already correct.


## Final qualification

Three interleaved standard-release observations, normal GC, against `3d45e084`:

| Measurement | Baseline median | Final median | Change |
| --- | ---: | ---: | ---: |
| CPU for 20 session setups/teardowns | 4,984.07 ms | 33.84 ms | -99.3% (147x faster) |
| Retained facts added by that batch | 3 | 0 | removed |
| Fresh-process console startup wall | 2.839 s | 2.526 s | -11.0% |
| Fresh-process source-file startup wall | 2.469 s | 2.540 s | +2.9% (71 ms) |

These are local macOS samples, not a general language speedup claim. The small
ordinary startup overhead is retained and reported; the repeated-session regression
is resolved. The initial borrowed-stream diagnostic's extra per-session cost is
still present, but wrapper reinstallation no longer dominates ordinary sessions.

All 12 setup checks pass: compiled readiness, actual source initializer, shared
clone readiness, failure on the third wrapper with no later calls, retry, current
replacement dispatch and explicit reinstallation. Resource tests cover borrowed
descriptors, unread input/EOF, repeated setup, aliases, collecting/reentrant cleanup,
replacement preservation, failed stream lookup and live-stream refusal. Mapped and
rebuilt images retain both readiness and an authored wrapper replacement.

The Alien image regression fails on the baseline and passes with the correction,
including untouched wrappers and explicit foreign reinstallation through mapped,
rebuilt and baked recovery. The native image suite passes round-trip/GC checks,
20 alternating mapped/rebuilt resaves with a bounded live code set, partial-load
conflicts and snapshot mutation. The native suite requires resource-metrics TCC;
the initial standard-library fixture failure and corrected run are both retained
in the receipt. Installed interaction and no-compilation startup checks also pass.

The unchanged ResourceOwner/ImagePayload formal contracts cover owner retirement
and image projection; this slice does not claim a new formal run. Actual REPL
initialization/cleanup passes with Store/Relational absent, but complete Store-free
I/O, Host and images still await Alien/Image/composition work.

Exact measurements, source hashes, setup source and test logs are in the
[qualified receipt](../../measurements/repl-owned-resources-20261019.json).
