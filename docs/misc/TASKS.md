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
74H is committed as `5e242e70`. The follow-up
[Design 74I](../design/74i-public-api-guard-boundary.md)
(`birnam-deopt-api-boundary-75r`) confirms eight guard-boundary cases in twelve
fresh C-client processes against ordinary debug/release floor builds. Vtable
identity, overwrite callbacks and reflective lookup do not substitute for
the private guard authority. The original stop was superseded on 2026-09-11:
the user permits runtime-internal changes while preserving API/ABI, full
malleability and expressibility in Id semantics.
[Design 74J](../design/74j-internal-guarded-machine-sends.md)
(`birnam-internal-guarded-sends-xpn`) implements guarded canonical Machine access
inside the existing send implementation. Its two longer E1 timing attempts were
rejected by host-load gates. [Design 74K](../design/74k-runtime-owned-native-tier.md)
(`birnam-runtime-owned-native-tier-nqd`) implements native PC entries for eligible
portable Blocks through existing staging and registration. Twelve paired
debug/release cases prove activation and same-Frame recovery, including mutation,
precise GC, nested entry, active replacement and executable-owner eviction.
[Design 74L](../design/74l-guarded-default-lookup.md)
(`birnam-guarded-default-lookup-d2e`) preserves current method dictionaries and
custom lookup effects while optimizing canonical lookup. Full verification and
compatibility gates pass. Seven controlled comparison groups qualify a 17.3%
combined improvement on the smaller warm portable fixture, without an
above-spread regression; the larger, fallback and control rows and incremental
74K/74L gains remain inconclusive. The first production same-Frame tier is
implemented; general inlining, unboxing, read caching and multicore work remain
outside this slice. The floor gains about 603 KB from the native compiler.
[Design 74M](../design/74m-native-tier-performance-isolation.md)
(`birnam-native-tier-performance-onx`) isolates the tier with identical host/module
binaries. Seven controlled pairs find no independent speedup beyond variation,
including sustained native execution; the smaller chain's warmup costs 420 us
more. Ordinary compiler/collection probes never enter the tier. Diagnostics
confirm that interning a new unrelated symbol permanently withdraws a warmed
candidate, and cold bodies pay for candidate C generation they never compile.
[Design 74N](../design/74n-native-tier-dependency-revalidation.md)
(`birnam-native-tier-revalidation-7qd`) implements precise dependency revalidation.
Eighteen paired cases, production verification and coverage diagnostics pass.
Seven controlled timing groups pass after an approved temporary `rk` pause and
verified resume. No execution or memory difference exceeds variation; both
chain warmups cost about 0.30–0.35 ms more with the tier enabled. Native-tier
performance remains unproven. Deferring unused candidate preparation until a
body becomes hot is the next proposed bounded experiment.
[Design 74O](../design/74o-deoptimization-architectural-fit.md)
(`birnam-deopt-architectural-fit-zq0`) completes the subsequent user-requested
architectural-fit investigation. Five test-only probes pass in debug/release;
full verification passes with 453 Rust tests and 11 diagnostic tests ignored.
Missing-callee reconstruction and an Id-authored recovery policy work within
the tested bounds. Identity/effect counterexamples impose semantic limits;
[Design 60P](../design/60p-v2-control-completion.md) (`birnam-3jm`) completes the
subsequently requested V2 suspend/capture/resume implementation and removes Frame
layout V1. General inlining and hotness tuning remain separate follow-ups.
[Design 74P](../design/74p-production-recovery-and-leaf-inlining.md)
(`birnam-06p`) implements production recovery and its first consumer: guarded
projection inlining. Implementation and production verification are complete;
qualified timing is deferred by user. Later exploratory timings suggest a local
projection-call benefit; they are not qualified performance acceptance.
[Design 74Q](../design/74q-optimization-roadmap.md) proposes the next optimization
roadmap, starting with compiler-generated recovery for one small callee. It does
not implement or promote the proposed passes.
[Design 74R](../design/74r-small-callee-recovery-plans.md) (`birnam-svi`)
implements its first bounded batch: compiler-generated small-callee recovery
plans and alias inlining. All 463 workspace tests and full production gates pass;
qualified timing now supports the alias fixture benefit. Remaining small-cost
acceptance is tracked by `birnam-bew`; receipts and limits are in that document.
[Design 74S](../design/74s-three-optimization-families.md) (`birnam-kwr`)
continues with inherited-dispatch specialization and guarded native Frame regions.
The third family adds selective native copy propagation and grouped stores.
Per-family measurements are recorded there; combined production performance
acceptance remains in progress.
[Design 74T](../design/74t-representation-and-allocation-optimizations.md)
(`birnam-sik`) implements the four remaining roadmap families: unboxed arithmetic,
loop optimization, temporary-object elimination and closure/capture elimination.
Identity-graph recovery, bounded numeric-chain fusion, tail-loop entry transfer,
discarded-clone elimination and live aggregate field replacement are implemented.
These bounded consumers are enabled, with correctness and qualified fixture
measurements recorded in 74T. Live-field admission requires three current declared
projections and uses untraced scratch plus bounded refusal retries; its enabled
source passes 517 tests and the production gates. The next closure compiler
checkpoint adds source capture/argument provenance and literal-aware PC emission,
with 522 tests and production gates passing. Exact warm constructor declarations
and floor admission also pass debug/release/installed checks, including changed
API and ownership and an absent Block family. An experimental closure consumer
now eliminates source-created Block allocation and substitutes captures, with
escaping/forced recovery, method replacement and shared Cell checks. It remains
disabled by default. Collecting successor and nested-entry tests now require
actual optimization hits under precise and conservative GC, preserving closure
aliases, capture/argument identities and eviction survival. Its source passes
525 tests and all production gates. An explicit host-owned Machine probe also
passes 24 baseline/optimized/forced capture and suspension executions across
debug/release and both GC modes, including paused collection, supplied results,
closure identity, exact effects and generation pins. The bridge is test-only;
instrumentation/refusal coverage and candidate/resource costs remain next.
Constructor refusal now preserves exact capability callback order, including
instrumented loader operations, and disabled mode skips closure-plan preparation.
This checkpoint passes 526 workspace tests and full production gates. Enabled
preparation costs and performance acceptance remain. Live API replacement now
passes 32 differential groups across debug/release and both GC modes: callback
traces match baseline, relevant replacements refuse, and restoring the API
restores optimization. The full gate still passes 526 workspace tests, plus
the live-API and continuation matrices. Preparation/execution/refusal cost
measurement now has a verified source harness. Its first retained matrix is
unqualified on battery and shows a large admission regression; profiling points
to repeated loader discovery. An exact-constructor declaration cache retains
a native-image pin and rechecks current semantics on every use. Correctness
verification passes all 61 commands. Retiming remains unqualified on battery:
after early native-callee refusal, closure execution overhead is about 19–25%
and repeated refusal about 20%.
[74U](../design/74u-repeated-closure-invocation-regions.md) now implements up to
32 immediate invocations sharing one virtual closure. The full gate passes
528 workspace tests, 640 sequence comparison executions and strengthened
collecting/capture/suspension cases. Later-target mutation now passes 32 source
processes across both GC modes and debug/release, with exact shorter prefixes,
collecting recovery and immediate restored admission. Full verification remains
passed at 528 workspace tests and all 61 commands. Automatic-collection budget
testing also passes 384 source processes with matching callback effects,
Frame PCs and allocation counters through refused/partial/full admission.
The full receipt retains 1,056 sequence/mutation/budget process rows and their
floor hashes. Source finalizer/journal-lifetime checks now pass another 144
processes, including live native roots, exact reclamation, the 4,096-entry
journal limit and immediate admission after collection. Full verification
retains all 1,200 source-process rows and passes all 61 commands at 528 workspace
tests. Repeated-use measurement checks now add 48 executions covering one,
three and 32 calls across all four shapes. The new full receipt retains ten
matrices and 1,248 process rows, passes all 61 commands and 528 workspace tests,
and matches all 244 source hashes. The first 96-process repeated-use timing
matrix remains unqualified and shows about 11-12% successful-path overhead at
32 uses. Profiles identify per-call developer recovery-control environment
reads as the largest sampled optimizer cost; reading them once per admitted
region is next, followed by forced-position verification and retiming.
Broader preparation/metadata/resource acceptance and repeated-use
profitability remain open; the
consumer stays disabled by default. See 74T for the full remaining scope.
Broader closure/capture coverage and performance acceptance
and broader numeric/loop/aggregate/application acceptance remain open; none of
the four full families is complete.
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
