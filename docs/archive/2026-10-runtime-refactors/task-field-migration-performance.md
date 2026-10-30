# Task field migration: regression and approved remediation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-19. Baseline `2dc87f5a`. The initial owned-field prototype regressed
performance and was held for approval. The approved remediation was subsequently
qualified and committed as `9e7c91ed`; the original failed measurements below are
retained as diagnostic history.

## Initial prototype and its qualification gap

TaskControl, TaskExecutor and Worker now have ordinary named reference slots.
`TaskField` is a fresh, ordinary owner/slot view; handles share the actual mutable
location and retain their owner. RemoteFailure owns its outcome directly. The
stackless-control module has no remaining Store/db calls, field schemas or
attribute-name synthesis. Worker cleanup retains compare-and-clear behavior so
an intervening replacement handle is not overwritten.

Executor and Worker image projections preserve the prior transient-field reset
policy; task handles use the already-qualified process-local marker. At that stage these field
moves still needed dedicated alias, GC, image, worker and genuine initializer-absence
tests. The subsequent qualification is recorded below.

The private field helper now addresses declared slots and answers `unknownSlot`
for an unknown name. It no longer doubles as an arbitrary fact-backed property
bag. Actual storage and methods remain reflectable and replaceable. No native
field registry or foreign layout access was added.

## Measured regression

Three interleaved observations per variant, ordinary installed runtimes, normal
GC. Times are medians for each whole batch, including its authored loop.

| Batch | Before | Owned-field prototype | Change |
| --- | ---: | ---: | ---: |
| 500 `TaskExecutor current` reads | 217.34 ms | 300.93 ms | +38.5% |
| 500 quantum-field writes | 212.98 ms | 296.57 ms | +39.2% |
| 31 immediate task starts/completions | 829.37 ms | 2,602.53 ms | 3.14× |

The task batch adds **562 retained facts before and zero after**. That ownership
improvement does not justify the CPU regression.

A separate 30-task diagnostic with quantum raised from 128 to 1,000,000 remains
about 3.16× slower. More frequent scheduling is therefore not the explanation for
the large regression. This diagnostic changed only the benchmark world's policy;
the implementation and default quantum were not changed.

## Cause and a bounded alternative

The prototype preserved the old database-shaped internal access route:

```text
field name -> resolve slot -> validate index -> allocate field view
           -> read the view's owner and index -> read/write the owner's slot
```

The view is useful when the caller asks to retain a field handle. It is unnecessary
work for the scheduler's own immediate reads and writes. The old Store Cell could
read its private native record directly; its authored replacement now traverses
several generic message/ABI and rooting paths.

An untimed Id message census of the real installed policies confirms this:

| One outward operation | Before | Prototype |
| --- | ---: | ---: |
| `TaskExecutor current` | 2,261 sends | 4,887 sends |
| Reading an already-retained field handle | 6 sends | 1,555 sends |
| Requesting a field handle | 2,077 sends | 3,199 sends |
| Starting/completing a task | 409,837 sends | 1,051,743 sends |

Most extra messages are namespace lookup, binding/representation admission,
rooting and allocation work from the extra authored helper calls. These counts
are diagnostic, not CPU timings; no dispatch or lookup implementation was replaced
by the census.

An ordinary `ObjectModel slot:in:` read through Id takes **14 sends** in the same
candidate runtime. A separate source caller using that operation with captured
owner/index values takes **118.41 ms for 500 reads**, near the old cached-field
loop's 120.39 ms. This is a lower-level diagnostic, not a measured implementation
of the proposed fix or a promised end-to-end task speedup.

## Approved next implementation

Use ordinary owner getters/setters for the scheduler's internal field accesses.
Their default implementations should read/write the known owned slot directly.
Keep `field:` for explicit handle requests, preserving shared mutation and owner
retention. For example, a normal replaceable getter can be:

```text
[[| let currentSlot := 3;
  [Reflect defineMethod: #current on: TaskExecutor as: [:self |
    [ObjectModel slot: currentSlot in: self]
  ]]
] value]
```

This removes a disposable intermediate object. It adds no native storage service,
raw payload reads, cached method pointers or authored monkeypatch checks. Calls
still use current Id dispatch. The control unit must remain resumable; changing
calling/suspension behavior is not an acceptable shortcut.

The implementation should:

1. Audit internal reads/writes and setter return values. Preserve existing return
   contracts wherever they matter, rather than silently substituting a different
   result object.
2. Make named owner accessors or a small owner read/write protocol the replaceable
   internal policy surface. Internal operations will no longer promise to call
   `field:` merely to manufacture a handle; this is an explicit implementation/API
   change, not a compiler optimization assuming that `field:` cannot be replaced.
3. Keep actual handle aliases, conditional worker cleanup, scheduling transitions,
   cancellation/shielding and image reset/rejection behavior. Verify these with the
   real task/worker suites, targeted alias/GC/image tests and the relevant formal
   models, then prove initializer operation without Store/Relational.
4. Repeat the paired field/task and application measurements. Commit the runtime
   slice only after those gates pass without a significant regression.

The broader completion and release goal remains unchanged. Evidence and exact
source hashes are in the [measurement receipt](../../measurements/task-field-regression-20261019.json).
The user approved this fix on 2026-10-19. Implementation and qualification are
in progress; the failed prototype measurements above remain the historical baseline.

## Remediation implementation and qualification

The scheduler now sends ordinary named getters/setters to its owners. Those
methods capture the ObjectModel receiver and immutable index values at
installation, following Globals and Trace; each operation still dispatches the
current method on that receiver through Id. They do not cache native entries or
validate method identities. `pollOutcome` remains distinct from the waiting
`outcome` operation; `quantum` still initializes restored executor state lazily.

Explicit `field:` requests create ordinary owner/index views. The view factory
and ObjectModel/Reflect dependencies are ordinary lexical captures, not repeated
global lookups for every handle operation. Their methods remain replaceable, as
do the owner accessors and `field:` itself. Reloading the source captures the
then-current dependency objects. There is no promise that an internal state read
will invoke `field:` or re-resolve a captured global binding.

`watch:` retains its field-handle result. The new internal setters answer the
owner; scheduler call sites discard those setter results. Worker shutdown still
conditionally clears the exact stopped handle, so a replacement survives.

The actual compiled task initializer has now run on a floor with no Store,
Relational, Bootstrap, Image, Alien or Isolate libraries. Local wait/resume,
cancellation/cleanup and GC owner-retention/release checks passed. This is
component evidence: complete Host/worker/image independence is still unfinished.
The 177-check task suite and targeted field tests (including mapped and rebuilt
images in the existing Store-containing composition) passed. Final measurements and broader qualification are recorded below.

Review also caught an ownership-conversion edge case: ordinary `clone` copies
slots, whereas the former fresh fact entity had no request fields. `prepare:parent:`
now explicitly resets `requestWorker`, `requestId` and `externalResult` along with
all other task state. A regression test prepares a task from a factory containing
stale request values and requires all three new fields to be nil. A completed
task retained only through a field view is also tested as an image refusal, with
the existing destination left intact.


## Accepted measurements

Final standard release artifact, three interleaved runs per variant:

| Batch | Committed baseline | Owned fields/accessors | Change |
| --- | ---: | ---: | ---: |
| 31 immediate tasks | 839.13 ms | 166.49 ms | 80.2% less CPU |
| 500 current-task reads | 218.02 ms | 138.07 ms | 36.7% less CPU |
| 500 requested-handle writes | 214.19 ms | 221.90 ms | 3.6% more CPU |
| 500 retained-handle reads | 117.92 ms | 125.33 ms | 6.3% more CPU |
| 500 handle constructions | 197.96 ms | 186.11 ms | 6.0% less CPU |
| Logue quiet-turn median | 26.309 ms | 26.350 ms | 0.2% more CPU |
| Startup wall median | 2.799 s | 2.905 s | 3.8% longer |

Startup CPU rises from 2.780 s to 2.899 s (4.2%). These are mixed small costs,
not a claim that every operation became faster. The large task regression is
resolved, with a substantial net improvement for task execution and no material
Logue regression. Retained views still pay for their authored owner/index reads;
internal scheduler accesses do not. Default GC and quantum policy are unchanged.
The same task batch adds 562 facts before and **zero** after.

The untimed send census falls from 409,837 to 83,116 sends per task and from
2,261 to 165 per `current` read. Retained field reads take 289 sends versus the
native Store Cell's 6; explicit handle construction falls from 2,077 to 1,977.
These figures explain the remaining view cost; they are not CPU measurements.

Qualification: 847 language checks; 177 task checks; installed task-image,
19 Concurrency service, 27 multicore and conservative/precise closure checks;
owner/alias/GC/image/refusal tests; actual initializer absence with GC reclamation;
396 Lean theorems, 125 rejected mutations and 272 expected TLC outcomes.
The formal checks cover the existing abstract contracts; runtime acceptance
establishes this implementation's behavior.

The final review correction adds three nil writes in fresh-task preparation.
The final source task suite and final-artifact field/image/absence checks include
that correction. Broad Host/language checks preceded only those three writes.
See the [accepted receipt](../../measurements/task-owned-fields-20261019.json) for
source/artifact hashes, exact runs, timings and qualification boundaries.


The later Concurrency ownership slice removes the unused `fieldIndex:` forwarding
methods from the field owners: `field:` now reads its captured Map directly.
The measurements above describe the `9e7c91ed` artifact, not that subsequent
simplification; its evidence is in the
[Concurrency receipt](../../measurements/concurrency-owned-fields-20261019.json).
