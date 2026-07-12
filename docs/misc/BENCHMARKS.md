# Benchmarks — how they work and how to run them

The operational guide. Design rationale, the measured constants, and every
methodology decision live in [docs/design/46-benchmarks.md](../design/46-benchmarks.md);
this page is what you need day to day.

## TL;DR

```
BENCH_QUICK=1 ./zig-out/bin/birnam bench/micro.birnam   # tight dev loop, ~5s
mise run bench                                          # full instrument -> bench/results/dev/
mise run bench-release                                  # tracked snapshot -> bench/results/v<X.Y.Z>.json
mise run bench-compare bench/results/v0.8.0.json bench/results/dev/bench-<sha>.json
```

Everything runs **ReleaseSafe** (the driver builds it; Debug numbers are not
comparable and not tracked). Run from the repo root, on AC power, with
nothing heavy running. ~5% run-to-run drift is normal on this machine;
sub-5% claims need a deliberate same-boot A/B.

## When to run what

- **Touching perf-relevant code** (dispatch, collections, strings, store,
  codegen): run the affected suite directly before and after —
  `BENCH_QUICK=1 ./zig-out/bin/birnam bench/micro.birnam` — and compare the
  printed ns/op by eye. Quick mode (R=2, low calibration floor) is for
  iteration; drop `BENCH_QUICK` for numbers you'll quote.
- **Touching relational/store code**: `./zig-out/bin/birnam
  bench/relational.birnam` — it takes minutes (queries are ~seconds each
  until the store grows indexes; that's the recorded finding, not a bug),
  which is why it's not part of the tight loop.
- **Before merging / when you want the full picture**: `mise run bench`,
  then `mise run bench-compare <last-release-snapshot> <your-dev-result>`.
- **After cutting a release** (part of the release process): on the tagged
  commit, on a quiet machine, `mise run bench-release`, then **commit
  `bench/results/v<X.Y.Z>.json`**. That file is the baseline every future
  run is compared against, and it is kept forever. This step is deliberately
  NOT inside `release.sh`: bench noise must never block a release, and the
  snapshot deserves a deliberate, quiet run.

## The layout

```
bench/
  micro.birnam        24 in-process cases: sends, blocks, loops, string,
                      vector, map, cells, errors, codegen, ffi
  parse.birnam        the PEG/Json case (own process: loading json shifts
                      send cost for everything sharing the process)
  relational.birnam   datalog over a fixed 10k-entity base; gates on known
                      result cardinalities
  gc.birnam           one timed collect over a verified live+garbage heap
  image.birnam        one timed [System saveImage:]; its artifact is what
                      the driver's boot.image case boots
  nil.birnam          the boot-path stub
  compare.birnam      the diff tool (run via scripts/bench-compare.sh)
  results/v*.json     per-release snapshots — committed, kept forever
  results/dev/        disposable between-version runs — gitignored
modules/bench.birnam  the harness (calibration, GC discipline, baseline
                      subtraction, JSON)
scripts/bench.sh      the driver; scripts/bench-compare.sh the diff
```

## Reading the numbers

- Suite cases report **net ns/op** (median of R GC-preceded runs, empty-loop
  baseline subtracted, ÷ iterations×K unrolled ops). `ns_per_op_raw` and the
  unclamped `net_us` are in the JSON for auditing; a negative `net_us` means
  the case is ill-conditioned (raise its `opsPerIter`).
- Macro cases report **wall µs** (median + min) and peak RSS. Subprocess
  spawn overhead is ~1.2ms and rides inside every macro number — fine for
  deltas, don't read `boot.bare` as an absolute.
- `loops.*` cases are raw-only (their subject IS the loop); `loops.whileTrue`
  deliberately includes its cell-state store traffic.
- Send-path numbers (`sends.*`) vary ±8% **between processes** and shift
  with the loaded-module footprint — a sends regression means "check module
  sizes first, dispatch second". Release snapshots run micro, relational,
  gc, and image as 3 fresh processes each (gc/image are one-shot-per-process,
  so replication is their only noise defense); the compare tool medians
  across them.

## The compare contract

`scripts/bench-compare.sh old.json new.json` exits non-zero if any case
regresses past its threshold (**10% micro, 5% everything else** — set at the
top of `bench/compare.birnam`) or disappears without being listed in the
`RETIRED` vector there. **Case names are a contract**: never rename one —
retire it (add to `RETIRED` + the docs/design/46 retired list) and add a new
name. Improvements and new cases are informational, never fatal.

## Adding a case

1. Append it at the END of its suite (case order is load-bearing for the
   store-backed cases — earlier cases shape the fact log later ones scan).
2. Setup outside the timed loop; the loop body is one native `to:do:` with
   K unrolled ops (`opsPerIter: K`); no cells inside timed regions unless
   cells are the subject.
3. Ops that grow process state (store appends) use `fixedCase:iters:do:` —
   calibration would make runs incomparable. Ops that can run only once per
   process (seeding, image save, gc) time by hand and use
   `[Bench record:us:ops:]`.
4. Update the suite's expected-count exit gate, and the cardinality/heap
   gates if the case has verifiable answers — a benchmark that can silently
   measure the wrong thing will (both empty-set queries and a live "garbage"
   pile shipped in first drafts here; the gates are why they can't again).
5. Record the name in docs/design/46's inventory.
