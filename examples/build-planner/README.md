# Headless build-planning experiment

A deterministic project dependency planner, implemented in Birnam twice at the
state layer: Store facts, and persistent Map/Vector snapshots. The graph policy
is shared. No compiler is invoked and no real project files are modified.

Run from the repository (macOS, installed Birnam 0.17.0 or later):

```sh
python3 scripts/build-planner-test.py
python3 scripts/build-planner.py \
  --prefix "$HOME/.local/opt/birnam" \
  --work /tmp/birnam-build-planner \
  --shapes chain,wide,shared --sizes 8,32,128 --samples 3
```

`results.json` records every checked plan and project state, phase timings,
post-collection object counts, native Store fact bytes, process peak RSS, fixture
hashes, source hashes, and installed binary/library/image hashes. Runs alternate
backend order. A timeout is an incomplete experiment, never a passing result.
Use a new work directory for an independent run; an existing directory must carry
this runner's ownership marker. Work directories retain generated manifests and
complete stdout logs.

For isolated history and scale experiments:

```sh
python3 scripts/build-planner.py --prefix "$HOME/.local/opt/birnam" \
  --work /tmp/planner-history --shapes shared --sizes 32 --history 100 --samples 3
python3 scripts/build-planner.py --prefix "$HOME/.local/opt/birnam" \
  --work /tmp/planner-scale --shapes shared --sizes 1024,10000 --samples 1 --timeout 180
```

## Workload and semantics

The generator emits explicit manifests with SHA-256 digests of deterministic
synthetic source text and declared, unordered dependency sets. `chain` has deep
invalidation; `wide` has one shared dependency; `shared` combines a chain with
additional shared dependencies. Size includes one initially absent module reserved
for addition. No arbitrary Birnam source dependency analysis is claimed.

Each sequence contains initial indexing, a no-op revision, a leaf source edit,
a root source edit, adding a module, changing dependencies, deleting a required
module, restoring an earlier source graph, introducing a cycle, and restoring the
initial sources. Additional revisions edit the leaf. Historical queries include
successful states, missing dependencies, and a cycle.

A source is a digest and a dependency set. A successful artifact records the
source description and the application revision that last built it. Artifacts are
simulated successful build records, not executable files. An unchanged retained
artifact may be reused after deletion and reintroduction of identical sources.

The planner computes directly changed modules, propagates invalidation to their
users, and produces a deterministic dependency-first build order. It checks the
whole active graph on each revision; **incremental refers to the resulting build
set, not an incrementally maintained graph index**. Its queue algorithm is linear
in nodes and edges apart from persistent collection/message costs and diagnostic
sorting. Missing dependencies or cycles reject the entire build plan. `blocked`
includes cyclic nodes and their downstream dependents, not just cycle members.

Checkpoints capture source state and existing artifacts **before** publishing a
successful build. This permits reproducing the exact historical plan. Failed
plans publish no artifacts. Restoration copies only source state into a new
revision, preserving the latest successful artifacts and all older checkpoints.
It does not undo compiler or filesystem effects, or erase history.

Store uses identity-keyed Symbol entities, historied `#digest`/`#artifact` facts,
and cardinality-many `#dependency` facts. It retains history from basis zero and
uses `liveTriplesDo:` / `liveTriplesAsOf:do:` to reconstruct graph views. Only
changed edges/digests are asserted/retracted. Application revisions group serial
Store operations; **this does not implement atomic multi-fact transactions**.

Map uses persistent source/artifact maps and adjacency vectors. A checkpoint
retains a map root. Restoring sources replaces that root while retaining current
artifacts. Deletion uses a nil tombstone. The common finite module inventory
includes future additions; this is not an unbounded module-discovery benchmark.

## Files and verification

- `planner.birnam`: source comparison, invalidation, topological planning, timers.
- `backends.birnam`: the two state representations and their checkpoint/restore operations.
- `run.birnam`: fixture normalization, revision execution, diagnostics and history queries.
- `scripts/build_planner_oracle.py`: independent Python reference using `graphlib`
  and copied dictionaries. It recomputes all answers without consulting Birnam.
- `scripts/build-planner-test.py`: hand-checked scenarios, all 512 directed
  three-node graphs (including self edges), dependency-closure and validation tests.

Every completed Birnam run is checked for complete source and artifact state as
well as plans. Historical states are checked again after all mutations. The
receipt checker rejects missing revisions, missing memory checkpoints, malformed
output, incorrect plans/state, and absent completion markers.

## Measurement boundaries

The runtime uses normal automatic GC, with explicit collection between revisions.
Each backend/sample runs in a fresh process; initial indexing is a first-use
measurement, and later phases are warmed by preceding work. There is no separate
steady-state claim for initialization.

CPU and wall clocks surround `edit_read`, `edit`, `read`, `plan`, `checkpoint`, and
`publish`. Restoration also measures `restore_read`. The two graph reads are
explicit: one supplies old state for edits, the other supplies the new planning
view. They expose this adapter's materialization cost, not a lower bound for all
possible Store-based planners. Map reads already hold graph views. Store could
trade additional cached projections/indexes for less reconstruction.

Startup, source compilation, JSON parsing, fixture normalization, validation-state
serialization, and explicit between-revision collection are outside these phase
timers. Timer overhead is included; tiny checkpoint/Map-read measurements are
primarily protocol overhead. Summed phase CPU is application work under these
boundaries, not process elapsed time.

Post-GC object counts include the runtime, prepared fixture, current backend,
checkpoints, and driver roots. Compare growth from each process's initial count.
Prepared fixture rows stay live in both runs, so counts are not standalone
production memory footprints. Store fact bytes are a separate native allocation
metric; they exclude spare vector capacity, hash tables and allocator metadata.
Do not add object counts to byte counts, or call GC's since-collection allocation
counter retained memory. Peak RSS includes loading/compilation and cannot attribute
retained application memory. All revisions are retained; no retention-window or
compaction-policy comparison is claimed.

This application changes no language, bootstrap, GC, or deoptimization behavior.
It exercises public late-bound messages. It does not use the Rust relational
query evaluator or settle that evaluator's equality semantics.
