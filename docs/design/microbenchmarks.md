# Short paired performance measurements

Use `scripts/microbench.py` for performance development while continuing to use
the computer. It runs one worker at reduced scheduling priority, has no idle-host
wait/retry loop, and measures for at most ten seconds. Compilation and bootstrap
are separate, reported setup costs. It does not rebuild startup/tool images or
modify the installed prefix.

Prepare a diagnostic prefix from a compatible existing installation once:

```sh
python3 scripts/microbench.py --prepare-from /path/to/birnam-prefix --seconds 6
```

The prepared copy is under `.cargo-target/microbench/prefix`. Its copied install
receipt is stale; `microbench-prefix.json` identifies it as diagnostic tooling.
Subsequent runs incrementally rebuild and refresh the eight changed release owners
and the runner, reusing `.cargo-target/decomposition-install/target`:

```sh
python3 scripts/microbench.py --case word-locals --seconds 2
python3 scripts/microbench.py --case integer-add --seconds 2 --gc normal
python3 scripts/microbench.py --case integer-multiply --seconds 2 --gc off
python3 scripts/microbench.py --case helper --profile candidate --seconds 3
```

`--prefix` selects another compatible prefix, which is used read-only unless it
has the diagnostic ownership marker or `--prepare-from` is explicitly supplied.
Preparation refuses an existing destination without that marker. Re-run
`--prepare-from` when the underlying installation/artifact set changes.
`--output PATH` selects the JSON record; the default is
`.cargo-target/microbench/latest.json`. Profiles use the same basename with
`.sample.txt`. Workload errors and assertions are retained in `.stderr.log`.

## Fixtures and controls

The fixtures are `word-nested`, `word-locals`, `abi`, `helper`, `integer-add` and
`integer-multiply`, plus `abi-captured` and `public-integer-loop`. `all` shares
the total measurement budget among the eight.
Every operation runs through Id. Results are checked before measurement and
at batch boundaries; assertions and allocation census are outside timed batches.
Both variants share one process and rooted inputs, and their order alternates.

NativeBackend exposes explicit compiler options on its existing program/factory
messages: `emit:source:filename:persistent:synchronous:optimizations:` and
`factory:source:dispositions:optimizations:`. The final argument is an Integer
bitmask: 1 enables Word expression/local fusion, 2 ABI literal elimination and
4 authored Word helper candidates; 8 enables guarded owner-batched root cleanup.
Existing messages retain all four by default (mask 15).
The runner temporarily forwards compiler requests to these options interfaces
while compiling/loading fixtures, then restores the original compiler methods
before measurement. Runtime lookup and allocation methods are not replaced.
Typed bodies use the factory route as well as the initial program emitter, so
both routes are controlled. Unit tests exercise all sixteen mask combinations.

Each fixture uses the same source body for baseline and candidate. Integer
fixtures derive their arithmetic policy from `modules/integer.birnam`; the
harness moves helpers to separate private factories and adapts the explicit left
operand and native constructor receiver. This isolates the variants without
changing global Integer arithmetic between samples. Module names are unique per
fixture/variant to prevent admission reuse. These private-factory fixtures are
not a measurement of the complete public `Integer +` dispatch path.

`public-integer-loop` performs sixteen dependent public `+` sends in a compiled
body, repeatedly invoked by the driver. Two copies of the actual Integer source
policy are compiled with passes disabled/enabled and their opaque Method/Unit
objects retained. Between batches, Id installs the corresponding source methods
on Integer; eight untimed invocations warm guards after the epoch change. No
methods are switched inside a timed batch. The reporter records CPU time per
invocation and per public addition. This does not reconstruct baseline Frames or
reimplement arithmetic in Rust.

Untimed eight-operation inventories assert that the controls reduce the relevant
Word/Vector allocations. A fixture with accidentally identical implementations
fails during setup, rather than reporting a misleading comparison. Counts are
physical allocations grouped by family plus GC charge, measured with collection
disabled. They are distinct from timing with normal GC.

## Timers, limits and interpretation

Thread CPU time uses `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` (Darwin clock 16,
Linux clock 3), with no wall-time fallback. A regression checks that sleeping is
excluded. Elapsed time is recorded alongside it to expose scheduling delays.
CPU time still reflects cache contention, processor frequency and thermal state.

Batches calibrate toward 25 ms of CPU work for the slower variant, capped at
8192 operations. Calibration occurs once per case; the operation count then stays
fixed. Recalibration after every pair can oscillate across GC thresholds and
produce a spurious order-dependent timing pattern. Deadline checks occur every
eight operations. GC is collected
and the chosen budget restored between batches; eight calls warm both paths.
With normal GC, collections caused by the timed workload are included. Warmup,
inter-batch collection and guard preparation consume the measurement budget but
are not included in a batch's CPU time. One operation/collection can overrun the
soft budget; the parent kills a stuck worker eight seconds after the requested
budget. Setup has a separate 60-second deadline, configurable up to 120 seconds.
Each incremental build has a 180-second timeout. No deadline retries itself.

The reporter normalizes CPU time by completed operation count and compares
paired candidate/baseline ratios. It excludes sub-millisecond calibration
samples, requires eight pairs, and reports a deterministic bootstrap interval
for the median log ratio. A 3% margin and a maximum interval width reject weak
claims. Scheduling-delay pairs remain visible. “Faster”, “slower”, “no meaningful
difference” and “inconclusive” are within-run judgments, not release gates or
claims about other machines. A short run may legitimately produce too few pairs.

JSON preserves raw samples, work counts, actual fixture source, pass masks,
revision/dirty state, source digest, runner/library hashes and build/setup time.
Source/library changes during a run invalidate its timing conclusion. Profile
mode waits at a ready barrier for macOS `sample`, then runs only the selected
variant; profiling timings are not reported as comparison evidence.

Optional untimed message counting is now available:

```sh
python3 scripts/microbench.py --case public-integer-loop --counts-only
```

It builds Id with `dispatch-metrics` into a separate target and copies it into
an isolated counts prefix. The normal timing prefix retains its uninstrumented
Id. Counters record selectors inside the native Id owner without replacing
lookup/allocation methods, so optimization admission remains valid. The run
reports allocation/message counts and no timing verdict. Detailed lookup/guard/
conversion counts remain future additions; timing and instrumented work-count
runs remain separate. The native borrowed census is copied before any callback.


## Acceptance evidence

The first complete run prepared all six fixtures in 1.26 seconds and measured
for 6.01 seconds. It detected isolated Word/helper improvements and reported
Integer addition as inconclusive and multiplication as no meaningful difference.
Allocation witnesses distinguish the variants. A separate two-second helper
run produced a macOS sampling profile. These are harness acceptance results,
not a new startup measurement or a full runtime speedup claim.

Logs and JSON are under `/tmp/birnam-microbench/`. Reporter tests cover scheduling
wait, different iteration counts, noise and short-run refusal. Backend tests
cover pass controls; the thread-clock test covers sleep. Production compilation
and the focused compiler/ABI/Word guard regressions remain the compatibility checks.


## First optimization slice

Word expression evaluation now uses bounded stack buffers for plan bytes (100),
inputs (32), roots/tokens (33) and unsigned scratch values (64). ABI borrows are
still copied before callbacks, and root retain/release order is preserved. The
maximum-size plan is covered by a collecting runtime regression. Byte arguments
and results still have owned copies across callbacks.

Vector caches only its constructor/add/read meanings after a complete positive
Id behavior-epoch fence. It still checks current default allocation and both
Vector/backing-node allocation permissions on every attempt. A provider that
changes permission without changing the method epoch forces ordinary execution.
Id behavior reassociation and dictionary changes invalidate these meaning proofs.

Execution uses stack operands for calls of up to eight arguments, with a dynamic
path for larger calls. Its compiler ABI-literal path uses bounded Object/signature
buffers. Byte-copy storage is allocated only for actual Byte arguments. Tests
cover replaced/mutating signature accessors, GC, incoming carriers and a nine-
argument ordinary call. All target calls and dependency checks remain Id sends.

The early ABI measurements exposed the calibration/GC-threshold oscillation.
Those adaptive results are diagnostic only. With the corrected fixed-count
runner, captured-signature ABI elimination measured candidate/baseline CPU ratios
of 0.438 and 0.431 in two-second old/new-library runs. Their candidate CPU costs
were 12.25 and 12.21 us: this does not establish an additional speedup from this
slice's buffer/cache changes. The source-policy/public-operator fixture measured
a ratio of 0.968, with an interval crossing the 3% benefit margin; it remains
inconclusive. It removes six Vectors and three Words per addition, with charge
falling from 4712 to 2000 bytes, but branch allocations and dispatch still matter.

Raw samples and two-second profiles are under `/tmp/birnam-perf-slice/`. These
measurements do not qualify startup or claim native arithmetic parity.


Continuous profiling now collects/initializes once per selected case and omits
between-batch setup collections and method switches. Normal-GC workload
collections are included. This corrects profiles that previously attributed
untimed benchmark setup collection to the arithmetic path.

`integer-add-core` isolates the actual checked-add/result-construction source
body with Word operands. It is a diagnostic cost probe, not complete Integer
semantics. See [public-path investigation](integer-performance-investigation.md).

## Formal-performance controls

`public-integer-roots` compares masks 7 and 15 on the same public sixteen-addition
chain. Physical allocations must match; counts-only verifies fewer individual
release messages. `public-integer-shape` and `block-shape-large` compile both
variants with mask 15, then toggle Execution `programShapeBorrowing:` outside
timed batches. Baseline copies canonical bytes before pure inspection; candidate
inspects the rooted native borrow and keeps only scalar counts before the next
send. The large fixture constructs a Block containing 256 let statements without
invoking its body. This isolates construction work, not body execution.

```sh
python3 scripts/microbench.py --case public-integer-roots --seconds 4
python3 scripts/microbench.py --case public-integer-roots --counts-only
python3 scripts/microbench.py --case block-shape-large --seconds 4
```

`gc_microbench` is a separate short release example: 512 zero-payload objects,
four explicit root registrations each, three warm precise collections and six
100 ms batches. It requires `BIRNAM_TEST_MODULE_DIRECTORY`, checks that measured
collections reclaim zero objects, and emits thread CPU plus wall samples. It is
useful for comparing compatible collector dylibs in separate diagnostic prefixes;
separate-process ratios are descriptive, not the paired reporter's confidence
verdict. No startup images are involved.
