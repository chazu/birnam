# 68 — Store snapshot scaling

Status: investigation and implementation plan complete, 2026-09-05.
Ticket: `birnam-store-snapshot-plan-1nj`. Production implementation not started.

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
