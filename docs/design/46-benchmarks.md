# The Benchmark Suite

> **Status:** plan of record, 2026-07-11, rev. 2 after adversarial review.
> Not yet built. Motivated by [44 — Toward Self-Hosting](44-toward-self-hosting.md),
> which gates every migration on a measured send constant and explicitly parks
> the relational Tier-B lift "behind a benchmark" that does not exist. Builds
> on the harness patterns already proven here: the `Test` module +
> `run-tests.birnam` for birnam-level suites, and `scripts/repl-golden.sh`
> for scripted whole-process runs.
>
> **Rev. 2 changes:** the relational suite is process-isolated (the store is
> append-only with no in-process reset — rep N was measuring store growth,
> not code); the standard timed loop is native `to:do:` (a cell-state
> `whileTrue:` loop *is* a cells benchmark); the ~1.5–2µs send constant is
> flagged as a Debug-era number to be re-baselined under ReleaseSafe; GC
> discipline specified; empty-loop subtraction bounded by unrolling; `-e`
> replaced with a stub script (no such flag); `timesRepeat:` correctly
> classified as derived; monotonic timer; Peg + define-method cases added.

## Purpose and decisions

A simple, relatively comprehensive suite we can run **during development**
(fast, before/after a perf-relevant change) and **after cutting a release**
(a snapshot checked in per tagged version), tracking improvements and
regressions across the system's perf-sensitive surfaces.

Decisions already made:

- **ReleaseSafe always.** Benchmarks run against a ReleaseSafe build — the
  same optimization mode `dist.sh` ships. Debug numbers are not tracked.
  Nuance the suite must respect: `-Doptimize` affects only the **Zig floor**
  (dispatch, natives, GC, store); TCC-emitted birnam method bodies are
  unchanged by it. So ReleaseSafe shifts every native/derived ratio in the
  natives' favor relative to the Debug-mode numbers recorded in docs/44 —
  those ratios get **re-baselined**, not assumed.
- **Per-tagged-version history.** Each release's snapshot is checked in as
  `bench/results/v<X.Y.Z>.json` and kept forever. Between-version dev runs
  are disposable: they land in an untracked `bench/results/dev/` and are
  thrown away when we're done comparing.
- **~5% run-to-run variability is accepted.** This runs on one personal
  machine, not a lab. Median-of-R and asymmetric thresholds absorb normal
  drift; hard sub-5% claims always need a deliberate same-boot A/B.
- **Names are a contract.** Every case has a stable `group.case` identifier
  that is never renamed, so v0.8.0's results file stays comparable at
  v0.12.0. Retiring a case is allowed (recorded in the retired-case list in
  `bench-compare`); renaming is not — a rename is a retire + a new case.
  `bench-compare` fails on an asymmetric case set unless the missing case is
  in that list.

## Architecture: three layers, one result file

The suite mirrors how the system is built:

1. **Micro** (birnam-level, in-process): the send constant and the
   native-vs-derived boundaries that gate self-hosting migrations (44 §7).
2. **Relational** (birnam-level, **process-isolated** — see below): the
   datalog read/write path — the numbers the Tier-B surface lift must not
   regress.
3. **Macro** (bash-level, whole-process): boot paths, compile times, docgen,
   script workloads — wall clock and peak RSS.

One driver (`scripts/bench.sh`) builds, runs all three, and merges everything
into a single JSON file stamped with git sha, `VERSION`, date, machine, and
build mode.

## Infrastructure

### Timer: `[Time monotonicMicros]`

`modules/time.birnam`'s `nowMillis` rides `gettimeofday` — wall clock,
non-monotonic (an NTP step mid-run produces garbage deltas). The benchmark
timer is a **new wrap of `clock_gettime(CLOCK_MONOTONIC)`** in the same
style (`struct timespec` = two longs; peek seconds + nanos, fold to µs).
Same pattern as `fillTimeval:`, still no new Zig. Lives in `time.birnam`
beside `nowMillis`.

### Harness: `modules/bench.birnam`

Same shape as `test.birnam`: a `Bench` singleton, suites register cases, a
runner executes and reports. The runner's contract:

- **The timed-loop shape is native `to:do:`.** Its counter lives in the Zig
  frame — zero store traffic. Cells are **banned inside timed regions**
  (except in the `cells.*` cases, where they are the subject): birnam has no
  local re-assignment, so a cell-state `whileTrue:` loop pays a historied
  store write per iteration and becomes a cells benchmark wearing a loop
  costume.
- **Unrolled bodies.** Each `to:do:` iteration performs **K=8 copies** of
  the op under test, so the loop's own overhead is ≤ ~15% of the raw time
  and the baseline subtraction (next point) stays well-conditioned. Without
  unrolling, net = raw − baseline is the difference of two comparably-sized
  noisy numbers and the ~5% machine noise amplifies to the compare
  threshold.
- **Empty-loop baseline subtraction.** The runner times an empty `to:do:`
  loop at the same iteration count and subtracts. Micro results are
  **net ns/op** (per single op, i.e. ÷ K); the raw number is also recorded
  so the subtraction is auditable. Exception: the `loops.*` cases (whose
  subject *is* the loop) report **raw only**.
- **GC discipline.** `[System autoGc: false]` at suite start (restored at
  suite end — safe to run from a live REPL); `[System gc]` immediately
  before each timed rep — collections never land inside a timed window by
  default. Cases where allocation-with-collection is the subject (`gc.*`)
  opt back in per-case.
- **Failure containment.** The warmup doubles as an error gate: an erroring
  case body (an error aborts its `to:do:` at iteration 1) is reported as a
  `FAIL` line and counted on `[Bench failures]` — the suite continues, and
  entry scripts gate their exit code on failures + a verified write.
  Calibration is also ceilinged (2^30 iterations) so a pathological
  zero-cost body terminates with visible garbage instead of spiraling.
- **Calibration.** Auto-double the iteration count until one timed run
  exceeds a floor (~100ms), so cases need no hand-tuned counts and stay
  meaningful as the system speeds up or slows down. (Safe for micro cases,
  which don't grow the store; see the relational section for why it is NOT
  safe there.)
- **Repetitions.** R timed runs per case (default 5) after one untimed
  warmup; report **median** (the tracked number) and **min** (the noise
  floor). Quick mode (R=2, lower floor) for the dev loop, selected via a
  libc `getenv` wrap (`BENCH_QUICK=1`) — one more Alien wrap in
  bench.birnam, same precedent as lsp.birnam's `exit`.
- **Determinism.** Any case needing randomness pregenerates its inputs
  outside the timed region from a fixed `Random seed:` (e.g.
  `vector.at-random` draws its index vector up front — a libc FFI call per
  draw inside the loop would dominate the trie `at:` under test).
- **Output.** JSON assembled with `StringBuilder` (stable field/case order —
  `Json stringify:` over a HAMT Map would order keys arbitrarily, which is
  hostile to diffing), written with `File write:contents:`: one object per
  case — `{name, ns_per_op, ns_per_op_raw, iters, reps, median_us, min_us}`.
  With an even rep count (quick mode's R=2) the "median" is the upper-middle
  element — conservative, and irrelevant at the tracked R=5.

### The relational suite is process-isolated

The store is **append-only and historied** — asserts land permanently,
retraction appends, and there is no in-process reset. So a calibrating,
repeating harness in one process would measure store growth, not code:
rep 5 of `store.assert-10k` writes into a store several times larger than
rep 1, and every query after it reads a polluted base.

The design that stays honest:

- One relational process = **seed once, read many**. The timed seeding pass
  (10k entities, fixed seed) *is* the `store.assert-10k` measurement — one
  number per process, no calibration. The query cases then run against that
  now-stationary base and may calibrate/repeat freely in-process (reads
  don't mutate).
- `scripts/bench.sh` runs the whole relational suite as **R=3 fresh
  processes** and takes per-case medians across them. 10k asserts and
  multi-clause queries sit far above µs resolution, so cross-process ms
  timing loses nothing.

### Driver: `scripts/bench.sh`

- Builds ReleaseSafe itself (`zig build install stage1
  -Doptimize=ReleaseSafe`) — zig's cache makes this cheap when current, and
  building in-driver is what guarantees the binary matches the mode (the
  binary does not expose its optimize mode, so there is nothing to "check").
- Runs the micro suite in-process, the relational suite as R fresh
  processes, and the macro cases itself: wall time from the shell, peak RSS
  via `/usr/bin/time -l` (macOS).
- Merges all three into `bench/results/dev/<git-sha>.json` (or
  `bench/results/v<X.Y.Z>.json` with `--release`), stamped with sha,
  version, date, `sysctl hw.model`, and build mode.

### Compare: `scripts/bench-compare.sh a.json b.json`

Dogfooded in birnam (`Json` parse + `StringBuilder` table): prints per-case
deltas, exits non-zero if any regression exceeds the threshold — **10% for
micro** (noisier), **5% for macro** — or if a case is missing without an
entry in the retired-case list. Improvements are listed, never fatal.
Thresholds and the retired list live at the top of the script.
Bootstrap caveat: the compare tool runs on a birnam binary; when the build
under investigation is itself broken, run compare under the **last installed
release binary** (`~/.local/bin/birnam`) — the script prefers it when
present, falling back to `zig-out`.

### Wiring

- `mise run bench` → `scripts/bench.sh` (dev mode).
- `mise run bench --release` → the checked-in snapshot, run **after**
  `release.sh` on the tagged commit; committing the result is part of the
  release procedure (documented in this doc and in `mise.toml`'s comment
  header). Deliberately **not** inside `release.sh`: bench noise must not be
  able to block a release, and the snapshot deserves a quiet machine.
- Optional later sugar: a `zig build bench` step. Not required — `bench.sh`
  is the tool.

## The inventory

### Micro — `bench/micro.birnam`

| group.case | pins |
|---|---|
| `sends.unary` | the send constant itself |
| `sends.keyword3` | arity cost |
| `sends.dnu-handler` | the DNU/`doesNotUnderstand:` path |
| `blocks.value` | closure invocation |
| `loops.to-do` | the native loop (counter in the Zig frame) — **raw only** |
| `loops.whileTrue` | the s13 native loop prim; its condition state necessarily rides a cell, so this number *includes* cell traffic — **raw only**, noted as such |
| `loops.timesRepeat` | the derived wrapper over `to:do:` (stdlib birnam code) — the native/derived loop pair |
| `string.indexOf-native` / `string.split-native` | the perf-retained native scanners over ~100KB |
| `string.scan-derived` | the same scan as a derived per-byte loop — tracks the native/derived ratio (re-baselined under ReleaseSafe; 44's ~10× was Debug-mode) |
| `stringbuilder.append-10k` | the Phase-5 StringBuilder |
| `vector.push-10k` / `vector.at-random` / `vector.fold-sum` | persistent trie + the shared native fold (indices pregenerated, fixed seed) |
| `map.put-10k` / `map.at-hit` / `map.at-miss` | HAMT |
| `cells.write` / `cells.read` | `[db cellFor:attr:]` — object state rides this; the write is historied and this case owns that cost |
| `error.raise-handle` / `error.propagate-5-deep` | the (now birnam) Result-style error path |
| `parse.json-100kb` | the PEG engine under `Json parse:` — the LSP's per-message hot path and a past regression site |
| `codegen.define-method` | `defineMethod:` → codegen → TCC install — the REPL's interactive latency |
| `ffi.wrapped-call` | Alien call overhead (a `gettimeofday` loop) |
| `ffi.peek-poke` | raw C memory |

### Relational — `bench/relational.birnam` (process-isolated, see above)

| group.case | pins |
|---|---|
| `store.assert-10k` | the timed seeding pass — the write path (the 100–1000× budget) |
| `query.single-clause` | the base read (the 10–50× budget) |
| `query.join-3way` | the native nested-loop join |
| `query.negation` / `query.aggregate` | per-feature read costs |
| `query.rule-recursive` | semi-naive fixpoint (ancestors-shaped) |
| `query.asOf` | time-travel on a history-on attribute |
| `pull.nested` | pull |

These are the acceptance numbers for the Tier-B lift: run before, lift,
run after, compare.

### Macro — cases in `scripts/bench.sh`

Wall ms + peak RSS each. Boot cases run a checked-in one-line stub
(`bench/nil.birnam`, containing `nil`) — there is no `-e` flag, and the stub
avoids adding one.

| case | pins |
|---|---|
| `boot.bare` / `boot.stdlib-dylib` / `boot.stdlib-source` | boot paths via the stub; dylib-vs-source is the stage1 payoff. Forcing the source path per-run: point `BIRNAM_LIB_DIR` at a decoy dir (hiding `libbirnam_stdlib.dylib` et al.) while `BIRNAM_ID_DYLIB` keeps the id floor findable — `loadStdlib` prefers the dylib and falls back to embedded source only when it is absent |
| `image.save` | save cost on a warm heap: boot, build a known graph, save — the image's `main` is `[OS exit: 0]` |
| `boot.image` | boot the image `image.save` just produced (its exiting `main` terminates the run) — the docs/40 payoff |
| `compile.stage1-stdlib` | `--compile-module` on `stdlib.birnam` |
| `script.primes` / `script.word-frequency` | end-to-end language throughput on existing examples |
| `docgen.full` | the known heavyweight workload — the canary for string-protocol regressions |
| `gc.collect-known-heap` | build a known graph, force `[System gc]`, time it (allocation + collection are the subject here — the autoGc-off harness rule is opted out) |

## Build phases (one commit each)

1. **Timer + harness.** `Time monotonicMicros` (clock_gettime wrap);
   `modules/bench.birnam` (to:do: runner, unrolling, subtraction, GC
   discipline, getenv quick-mode); a smoke case; the `bench/` layout +
   `.gitignore` for `bench/results/dev/`.
2. **Micro suite.** Acceptance check: **re-measure the send constant by
   hand under ReleaseSafe first** (a one-line loop timed in the REPL) and
   record it here — the harness must then reproduce *that* number. The
   ~1.5–2µs in docs/44 is Debug-mode; ReleaseSafe rebuilds the Zig floor
   (dispatch is Zig code) and can move it substantially, so "reproduce
   1.5–2µs" would wrongly fail a correct harness.
3. **Relational suite** (the process-isolated protocol above).
4. **Macro cases + `scripts/bench.sh`** (build, run, merge, stamp).
5. **`bench-compare` + `mise` task + a short `docs/misc/BENCHMARKS.md`**
   (methodology, thresholds, retired-case list mechanics, run discipline:
   same machine, on AC power, nothing heavy running).
6. **Baseline v0.8.0.** Run against the tagged release build; check in
   `bench/results/v0.8.0.json`; add the post-release snapshot step to the
   release procedure.

Total new surface: one birnam module, two suite files + a one-line stub,
two shell scripts, one mise task. **No new Zig** (the monotonic timer and
`getenv` are Alien wraps over libc, per the standing modules principle),
no new build stage.

## Non-goals

- **Not CI.** There is no CI machine with stable performance; the suite is
  a personal-machine instrument. (GitHub Actions runs `verify`, never
  `bench`.)
- **Not statistical rigor.** Median-of-5 with warmup, on an accepted-noise
  machine. No confidence intervals, no isolcpus theatrics.
- **Not a profiler.** When a case regresses, the suite tells you *where to
  look*, not *why* — Instruments takes over from there.

## Retired cases

(None yet. Retirements are recorded here and in `bench-compare`'s list.)
