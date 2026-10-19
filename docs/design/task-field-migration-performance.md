# Task field migration: performance hold

2026-10-19. Baseline `2dc87f5a`. The owned-field implementation is a working-tree
prototype, not a qualified or committed runtime change. The user explicitly
requires a hold for approval after a significant performance regression.

## What the prototype establishes

TaskControl, TaskExecutor and Worker now have ordinary named reference slots.
`TaskField` is a fresh, ordinary owner/slot view; handles share the actual mutable
location and retain their owner. RemoteFailure owns its outcome directly. The
stackless-control module has no remaining Store/db calls, field schemas or
attribute-name synthesis. Worker cleanup retains compare-and-clear behavior so
an intervening replacement handle is not overwritten.

Executor and Worker image projections preserve the prior transient-field reset
policy; task handles use the already-qualified process-local marker. These field
moves still need dedicated alias, GC, image, worker and genuine initializer-absence
tests. A successful installed build is not that qualification.

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

## Recommended next implementation, pending approval

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
source hashes are in the [measurement receipt](../measurements/task-field-regression-20261019.json).
The current prototype is held for human approval before implementing this fix.
