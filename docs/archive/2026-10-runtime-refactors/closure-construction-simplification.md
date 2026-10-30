# Closure construction simplification

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

Follow-up: [capture access simplification](capture-access-simplification.md)
reduces addition further to 1,367.375 sends. The measurements below describe the
preceding construction slice.

Implemented 2026-10-07. CodeObject now answers checked parameter/capture counts
through native Word messages, and generated Block literals pass captures directly
to the current Block owner. All cross-owner operations continue through Id.

## Changes

- `CodeObject parameterCountOf:` and `captureCountOf:` read admitted immutable
  program state. No canonical export, Bytes object, parsing or dependency callback
  occurs inside their default implementations. Replacing either method is effective
  on subsequent constructor sends; wrong kinds/counts and failures are checked.
- `Block fromMethod:codeObject:captureOperands:` accepts native Object operands:
  Method, CodeObject, then zero or more ordered captures. The Id operand count
  determines the capture count. It copies references into Execution's owned Block
  storage, protecting all inputs before dependency callbacks.
- Both synchronous and resumable emitters use this constructor. The compiler no
  longer allocates ObjectModel capture containers, writes their slots or rereads
  them. The unused `allocateReferenceSlots:` allocator and `referenceSlots:` Block
  constructor are deleted.
- Live language/materialization callers retain the collection-based capture
  constructor; it uses the same compact shape queries and owned construction core.
  Shape/descriptor validation still precedes collection inspection.
- Pending image shells construct nil capture slots directly in Execution. The
  existing pending-state and capture-restoration protocol is retained.
- The obsolete shape parser, copy/borrow policy switch and corresponding historical
  microbenchmark cases are removed. Canonical export remains for compilation,
  persistence and other real byte consumers.

Blocks still get their existing separate behaviors and Method bindings. Capture
projection still uses current owner messages. Neither virtual Blocks nor shared
behaviors nor projection-root elimination is part of this change.

## Measured result

[Receipt](../../measurements/closure-construction-simplification-20261007.json) records
both sixteen-addition censuses, identities of sources/libraries, partitions and a
short CPU comparison. Counts use the metrics Id; timings use the ordinary Id.

| Per successful addition | Before | After |
| --- | ---: | ---: |
| All sends including amortized fixture work | 3,846.375 | 1,728.375 |
| Closure construction including preparation/roots | 2,241 | 272 |
| Allocated Id objects | 54 | 30 |
| New branch Blocks | 8 | 8 |
| Canonical program exports during addition | 8 | 0 |
| Temporary reference containers | 8 | 0 |

Total sends fell **55.1%**; construction sends fell **87.9%**. The default native
constructor subtree is 28 sends, independent of capture count. A warmed factory
lookup and returned-object roots make the measured constructed-Block cost 34 sends.
Captures are copied locally by their owner; none of these costs bypasses Id.

Separate two-second CPU probes on the active host observed medians of approximately
295 microseconds/addition before and 148 after, about **2× faster**. This is an
indicative before/after comparison, not a paired confidence interval, a startup
measurement or a requirement that the machine be idle. The fixture's within-run
baseline/candidate contrast toggles binding proofs; its near-unity ratio is not a
comparison of this old versus new construction protocol.

The new disjoint send partition identifies capture access as the largest remaining
bucket: 544/addition (31.5%). Boolean selection costs 293 (17.0%), ABI bridges 281
(16.3%), and construction 272 (15.7%). These are send shares, not CPU shares.

## Validation and concurrency

Passed:

- Workspace all-target compilation and the affected native-backend, Execution and
  PortableCode Rust tests.
- New direct-construction regression: zero, two and twelve captures; order and
  aliases; clone lifetime; wrong/missing carriers and excessive counts; native
  shape query replacement, wrong results and failure propagation; constructor
  replacement; precise collection during a shape callback after external capture
  roots are dropped. Both synchronous and resumable bodies are exercised.
- Focused pending-image-shell rejection/restoration through GC, including wrong
  restoration count and execution only after restoration.
- Existing materialization checks, including suspension/resumption, source-less
  code, collecting/replaced providers and native Unit lifetime; invocation-input
  lifetime, Word-helper replacement and PortableCode admission/fixture checks.
- Isolate value conversion through owner messages.
- Local task control: **171 checks, zero failures**, including quantum 128 and
  quantum 1 scheduling, wait/resume, cancellation, cleanup, joins and mutation.
  This integration test took 315 seconds; it was validation, not a benchmark.
- A smaller captured-Cell task probe at both quanta: captures observe shared Cell
  updates across wait/GC/resume and preserve cancellation/cleanup behavior. This
  is included in the new constructor regression for future focused checks.

No scheduler, task ownership, continuation format, native barrier or worker wire
protocol is changed. Capture operand arrays are borrowed only for the synchronous
native constructor call; the finished Block retains its own copied Object
references and existing GC/code ownership. Owner-thread constraints remain.
There is no observed concurrency regression in the checks above.

Limits: the broad image round-trip suite was stopped after exceeding the bounded
validation budget. Its focused Block-shell behavior passed separately. Complete
subprocess-worker startup from newly saved images and the full image suite are
not claimed as passing. A comparison run of the old task suite was stopped during
its slow quantum-1 section; no task-suite before/after speed claim is made.
No new Lean/TLA+ proof of the emitted implementation is claimed by these checks.

## Build and artifact boundary

The diagnostic prefix's fifteen authored bootstrap modules were explicitly rebuilt
with the Rust grammar seed and the new compiler, then the production Bootstrap
without the seed was restored and the seed dylib removed. Startup on that rebuilt
prefix and all recorded runtime checks succeeded. This retains the native cold
bootstrap foundation; it is not an independent fresh-cache cold-build audit.

Old native artifacts or saved worker/user images containing the removed constructor
call require explicit rebuilding. No compatibility shim is retained, no installed
prefix is published by this work, and machine-code-bearing images are not silently
recompiled on startup. New image shells use the new owner queries.

Large raw trace/timing files remain local at the receipt's hashed paths. Reproduce
counts on a rebuilt compatible diagnostic prefix with:

```sh
python3 scripts/microbench.py --case public-integer-bindings --counts-only --trace \
  --output /tmp/birnam-closure-after/final-trace.json
python3 scripts/analyze-send-tree.py /tmp/birnam-closure-after/final-trace.json \
  --output /tmp/birnam-closure-after/final-analysis.json
```
