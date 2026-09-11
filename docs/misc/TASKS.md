# Birnam work tracker

The repository-local **Beads** tracker (`.beads/`, prefix `birnam`) is the
authoritative active backlog. Run commands from this checkout:

```sh
br where
br ready
br list
br show <id>
br sync --flush-only
```

Commit `.beads/issues.jsonl` with related work; the SQLite database is local.
Before changing issues, `br where` must resolve this repository's `.beads/`,
not the unrelated home-directory Maggie tracker.

## Selected sequence — 2026-09-06

The current inquiry is [Design 74](../design/74-malleable-multicore-execution-and-optimization.md),
malleable multicore execution, guarded optimization/deoptimization, and a bounded
execution experiment. On 2026-09-09 the user authorized its E0 legality proof
(delivered as [Design 74A](../design/74a-e0-guarded-slot-access-legality.md)
and accepted the same day), then authorized E1, delivered as
[Design 74B](../design/74b-e1-guarded-slot-access-implementation.md)
(`birnam-e1-hyz`): a feature-gated dev prefix, explicit emission selection,
fixture and driver. E2 and E3 followed the same day as
[Design 74C](../design/74c-e2-matrix-and-e3-measurement.md): E2 identical in
every row; E3's frozen 20% criterion not met (13–20% real gain), after which
the user withdrew percentage thresholds for proven-equivalent, non-regressing
changes, so E3 is met as amended. The next hypothesis, slot-index box elision
on the guard's hit path, was authorized, reviewed, implemented and measured as
[Design 74D](../design/74d-e4-slot-index-box-elision.md) (`birnam-e4-gvm`,
closed): about 50% faster than the baseline on the fixture, 34–43% faster
than E1, every access hitting, E2 identical. The production slice,
[Design 74E](../design/74e-production-slot-access-slice.md), was landed and
reverted the same day: it needs a runtime ABI cut (two new shim names), which
the user declined. The dev-feature form stays available. On 2026-09-10 the user
authorized an ABI-preserving same-frame recovery experiment, delivered as
[Design 74F](../design/74f-same-frame-deoptimization-experiment.md)
(`birnam-deopt-same-frame-738`): ten real-driver baseline/candidate cases and
256 repeated trials pass in debug and release. It uses test-only static native
entries with existing descriptor-v2 Frames; no production tier or speedup is
claimed. The user then authorized compiler-generated recovery, delivered as
[Design 74G](../design/74g-generated-same-frame-deoptimization.md)
(`birnam-generated-deopt-g6l`): twelve paired cases, sixteen repeated native
load/retire cycles, precise GC and real unload checks pass in debug and release.
Both forms use one canonical CodeProgram and descriptor; candidate entries
omit PC decoding. The compiler feature and test-only driver retain the current
ABI/image contract. Full repository verification passes, including native-prefix,
cold-archive and relocation checks; no production speedup is claimed.
After committing 74G as `a10d1b55`, the user authorized
[Design 74H](../design/74h-guarded-frame-read-reuse.md)
(`birnam-read-reuse-2ce`): guarded reuse of repeated Frame reads in one PC.
The 36 baseline/control/candidate observations and 320 additional calls agree
in debug/release, including GC slot mutation without dictionary invalidation.
Seven host-gated timing pairs are inconclusive in every row; normal-path
integration is deferred by the experiment's predeclared criterion. Full
repository verification passes; ABI/image authorities remain unchanged.
The prior implementation
sequence is paused while this direction is examined; existing Beads dependencies
and pending timing acceptance are unchanged.

Epic `birnam-epic-as7` owns performance preparation for further self-hosting and
Linux support. The dependency graph and acceptance state live in Beads; the
execution map is [Self-hosting performance work](SELF-HOSTING-PERFORMANCE.md).

1. ~~Close Store/PEG startup and docgen performance acceptance.~~ Closed
   2026-09-09: the startup regression reproduced, was attributed to PEG hashing
   and per-vtable indexes, and was fixed in `f24ff9b2`; evidence in
   [as7-acceptance-comparison.json](../measurements/as7-acceptance-comparison.json).
2. ~~Implement validated CodeProgram metadata reuse, indexed global imports and
   indexed method dictionaries within the existing contracts.~~ Implemented
   2026-09-06; timing acceptance closed 2026-09-09 (see
   [Self-hosting performance work](SELF-HOSTING-PERFORMANCE.md)).
3. ~~Review [Design 73](../design/73-scalable-ordering-contract.md), the proposed
   sorting/String contract.~~ Accepted 2026-09-09; `birnam-epic-as7.5` and
   `birnam-epic-as7.6` are implementable in that order.
4. After explicit decisions, implement the remaining collection, Store,
   allocation and Linux stories in dependency order.

Design tickets remain open until their decision is accepted; writing a proposal
alone does not unblock implementation. Preserve the independent Rust cold
bootstrap and existing benchmark thresholds.

## Code-quality review proposals — 2026-09-06

- [Design 75](../design/75-code-quality-and-responsibility.md) covers shared
  language rules, exact-floor symbol lookup, Codegen and host responsibilities,
  relational adapters, and migration-era lint/comment cleanup.
- [Design 76](../design/76-documentation-cleanup-and-archives.md) covers current
  documentation corrections, surviving-contract ownership, and archival moves.

Both are proposed for review. Writing them does not begin implementation or
archiving, change existing Beads dependencies, or close timing acceptance.

## Cleanup decision

The [previous tracker](../archive/TASKS-2026-09-05.md) is archived intact for
provenance. Its outstanding Zig shim deduplication is obsolete after the Rust
cutover. Old Zig-specific JSON, process, networking, module override, and REPL
packaging follow-ups are removed from the active backlog. Broader YAML,
concurrency, and networking ideas remain unselected, not declared implemented.
Completed historical work belongs in the changelog/design history rather than
the active queue.
