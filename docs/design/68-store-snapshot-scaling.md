# 68 — Store snapshot scaling

Status: transient snapshot indexing implemented and correctness-verified.
Ticket: `birnam-store-snapshot-plan-1nj` (implementation authorized after planning).

## Evidence

`collect_live_triples` in the Rust Store first deduplicates visible (entity,
attribute) pairs with a linear search, then searches the fact vector separately
for each pair and each pair's cardinality. `current_value_raw_as_of` searches
backwards. `compact_state` independently performs repeated suffix/prefix scans.

The ignored `snapshot_scaling_investigation` test calls the actual snapshot
implementation on N distinct entities sharing an attribute with default
cardinality-one, one assertion per entity. Five release samples, median:

| N | Snapshot time | Pair comparisons | Value-search fact visits | Schema-search fact visits |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 11,375 ns | 4,950 | 5,050 | 10,000 |
| 1,000 | 1,047,083 ns | 499,500 | 500,500 | 1,000,000 |
| 5,000 | 25,480,917 ns | 12,497,500 | 12,502,500 | 25,000,000 |

Visit counts are exact source-derived counts for this fixture, not profiler
counters. Times are local diagnostic evidence, not host-gated release results.
The test checks output cardinality and ordering. Reproduce:

```sh
CARGO_TARGET_DIR=.cargo-target cargo test -p birnam-store --release --locked --offline snapshot_scaling_investigation -- --ignored --nocapture
```

## Selected design: transient snapshot index

Keep Store as sole fact authority (Design 42). Build query-local indices from
visible facts and discard them before callbacks; no cached Oop roots or image
format changes. Keep ordered vectors for output and hash tables only for lookup.

1. Scan facts in physical order, selecting `tx < hi` when an upper bound exists.
   Record each pair once in first-visible occurrence order. Record visible fact
   indices per pair and the latest visible schema fact per attribute.
2. Resolve cardinality at the requested upper basis once per attribute. An
   absent/retracted schema value defaults to one, matching current behavior.
3. For one, take the last visible fact in that pair. For many, visit that pair's
   indices backwards, deduplicating values by Oop identity. The latest operation
   wins even when it retracts the value or falls below the lower bound.
4. Emit assertions whose winning fact has `tx >= lo` when a lower bound exists.
   Retain pair order and reverse-last-occurrence value order exactly.

Expected O(F + output) work and O(F) temporary storage for F visible facts,
subject to ordinary hash-table collision behavior. Prefer a simpler per-pair
index to a compact clever one-pass representation until differential tests pass.
Check memory overhead as well as time; this does not bound total query results.

## Contracts and traps

- Do not sort facts by transaction: non-historied projections update in place.
  Current code selects the last matching physical fact satisfying the watermark.
- `since:` and `between:` filter the winning current/window projection; they
  are not a stream of every historical assertion in the interval.
- Preserve identity equality, cardinality changes, assertion/retraction shadows,
  empty windows and the existing retention-floor errors. Do not silently repair
  unrelated temporal behavior in an indexing patch.
- Design 60J retains prospective history and explicit unavailable intervals.
  Compaction must keep baselines, schema transitions and necessary tombstones;
  allocation failure leaves the original vector and floor authoritative.
- Finish snapshot materialization before any user callback, as today. No Rust
  borrow or index spans a send that can re-enter or mutate Store.
- Derived indices must not retain dropped Oops after compaction or image restore.

## Independently verifiable implementation sequence

1. **Snapshot only:** retain the old implementation as a test oracle; implement
   transient lookup without changing the four public live-triple selectors.
   Differentially test random assert/retract/schema histories, one/many,
   current/asOf/since/between, in-place updates and exact output ordering.
   Include callbacks that mutate Store and force GC; preserve frozen snapshot
   behavior and rooting. Test before/after compaction and image restoration.
2. **Measure:** repeat this size matrix plus many-valued, long-history and small
   stores. Measure peak scratch memory and full queries/docgen with standard
   host gates. Accept no weakened comparator thresholds.
3. **Current lookup, separately:** only after profiling, consider a maintained
   (entity,attribute)-to-latest index. Audit append, in-place projection, mirror
   overwrite/removal, image load and compaction mutation paths. Publish rebuilt
   indices with replacement state; do not reuse stale vector offsets.
4. **Compaction, separately:** share concepts, not query lifetimes. Precompute
   current/floor cardinalities and stream endpoints, replacing repeated scans
   while preserving keep flags, recovery intervals and rollback reservations.
   Keep allocation-failure atomicity; test all retention policies and limits.

Each production slice requires focused Store tests, full `mise run verify`,
`mise run test`, image/retirement and bounded-churn coverage. Permanent indexes
and compaction are explicitly not prerequisites for snapshot optimization.
No user-facing semantic change is required for the first slice.

## Implementation

`collect_live_triples` now groups visible facts by identity-keyed pairs in
physical order and resolves cardinality through the same transient lookup.
Groups retain ordered borrowed fact references; many-valued output uses a
temporary identity set. All scratch is dropped before callbacks. A consecutive
pair hint avoids hashing each entry of a contiguous history. There is no
permanent index, mutation hook, additional GC root or image representation.

The old scan remains a test oracle. Generated histories exercise 64 seeds,
36 lower/upper-bound combinations, before and after compaction (4,608 exact
ordered-output comparisons), including in-place timestamp changes. A real
callback probe retracts returned facts and forces GC in AOT and forced-source
modes. Input values remain explicitly rooted by the fixture; this checks the
existing callback seam, not a new ownership guarantee for unrooted snapshots.

The size matrix caught an initial history-only regression from repeated hashing;
the consecutive-pair hint reduced the 5,000-fact case from about 54 us to 6.8 us,
against a 6.2 us scan. Small inputs still pay indexing setup overhead (sub-us in
the isolated 10-fact cases); do not describe this as universally faster.
The 5,000-distinct-pair diagnostic fell from about 25 ms to 0.36 ms. These are
local pure-kernel measurements, not release-qualified end-to-end results.

The permanent current-value index and compactor rewrite above remain separately
profile-gated follow-ups, not part of the selected transient-snapshot delivery.

Validation: focused Rust differential tests, full `mise run verify`, all 611
language tests, AOT/forced-source callback mutation and GC, and the runtime
bounded-churn save/load/resave gate passed using the private release prefix.
This includes the production image, retirement and relocation gates; no user
installation or running runtime was replaced.

## Performance acceptance — closed 2026-09-09

The 2026-09-06 comparison left bare startup (+11.8%), source startup (+7.6%)
and a missing after-docgen open. Its retained prefixes no longer existed on
2026-09-09, so the repeat rebuilt release prefixes from the recorded commits
(before `f15b8fbe`, Store/PEG `3bff9357`, the later head `21cf6d7b`, and the
fixed `f24ff9b2`) and ran the unchanged `scripts/bench.sh` measurement and
host-gate functions on a quiet AC-powered host, with two runs from the before
prefix bounding same-binary drift. The record, including the driver delta and
prefix provenance, is
[`as7-acceptance-comparison.json`](../measurements/as7-acceptance-comparison.json).

Startup (gated three-rep medians): dylib 107.0/107.1 ms before, 115.2 ms
Store/PEG; source 270.7/269.5 ms before, 280.7 ms Store/PEG. The regression
reproduced as a fixed ~10 ms per stdlib boot. The bare-boot flag did not:
9.7/10.1 vs 11.0 ms on three reps, and 6.84 vs 6.87 ms over 15 interleaved
reps. Bisecting `3bff9357` by file attributed the whole cost to the PEG change
(Store-only build 105.0 ms, PEG-only 112.4 ms): the snapshot index costs
nothing at boot. Its diagnosis and fix are recorded in
[Design 71](71-peg-matching-allocation.md).

Docgen: 39.6/39.1 s before, 33.8 s Store/PEG (-14.6%), peak RSS 322/319 to
317 MB. The earlier dylib-startup RSS growth (70.9 to 81.7 MB) did not
reproduce: 65.5 MB on every run. Queries: single-clause 598 to 43 ms, three-way
join 666 to 117 ms, negation 592 to 46 ms, nested pull 556 to 4.7 ms,
assert-10k flat (+1.5%).

Against the fixed tree the same cases measure dylib boot 86.0 ms (-19.6% from
before), source boot 241.5 ms (-10.7%) and docgen 29.4 s (-25.7%). The
unchanged comparator still exits 1 on before-to-fixed for single-process micro
cases (`ffi.peek-poke`, `map.at-*`, `error.propagate-5-deep`, several
`vector.*`) whose flags also appear between the two runs of the before binary,
and for `boot.bare` at +5.8% on three reps against +3.7% same-binary drift and
no difference over 25 interleaved reps. Those flags are retained, not waived;
release qualification remains the multi-process release gate. This design's
performance acceptance is closed on the startup, docgen, RSS and query evidence.
