# Logue performance workload

The second application workload exercises stateful computation: seeded dungeon
generation followed by repeated monster movement, combat and player-state
updates. The original baseline below used the unchanged `logue-core.birnam`
and passed its 28-assertion headless regression. The subsequent
[bounded-message fix](../archive/2026-10-runtime-refactors/logue-message-log.md) expands that regression to 34 checks.

The [runner](../../scripts/logue-bench.py) drives the
[Birnam harness](../../examples/logue/logue-bench.birnam) and compares results
with an [independent Python model](../../scripts/logue_oracle.py). Full state is
checked at each batch boundary, not merely a checksum. The Python model uses
native dicts/lists and integers, and includes the exact RNG consumption rules.
The original oracle modeled the unbounded-message behavior recorded below;
the current oracle checks the corrected six-entry limit.
It is not an oracle for arbitrary player actions: existing game tests cover
pickup, consumables, class setup, combat, level transitions and winning.

## Workload and measurement contract

- Seeded generation builds the real five-level dungeon for seeds 12345 and 777.
  Checks include every tile, room, monster, item, stair position and final RNG.
- Controlled arenas use the real `endTurn`, monster and hunger code. The player
  waits; monsters approach, resolve occupied tiles and attack. The fixture gives
  the player enough health to survive all measured turns. Nothing in the timed
  game code has been bypassed to make survival cheap.
- Quiet, sparse and crowded cases share a 16×10 arena and 16-turn batches, varying
  only the monster count (0/2/8). A 32×10 arena changes traversal conditions;
  64-turn batches expose longer-lived costs. These are synthetic arenas, not
  claims about typical player sessions.
- Each performance case warms once. Three samples reset the game independently;
  each arena sample contains four consecutive batches. Store history is not
  cleared. Per-batch medians distinguish approach from settled combat; an overall
  median alone can conceal that distinction.
- A correctness-only case replaces the live monster Vector's `do:`, collects,
  and checks that the next turn skips monster actions while advancing hunger and
  the turn counter. No replacement guards were added to the game.

Thread CPU and monotonic wall timers cover only the game operation plus its loop
invocation. Fixture creation, source loading, explicit GC before/after batches,
state snapshots and JSON reporting are outside those timers. Automatic GC runs
normally during timing. Separate allocation runs disable automatic collection
for one batch; their timing is not comparable to normal-GC results.

Post-GC object counts include the entire process and retained harness data.
The allocation-charge counter resets on collection: its raw before/after-GC
readings are diagnostics, not retained-byte estimates. Only the no-auto-GC
charge deltas estimate allocation during a batch; charge is not RSS. Process peak RSS includes startup, source compilation, reporting
and game execution. The snapshot check itself has costs; instrumented memory
trends need focused attribution before calling them game leaks.

All commands have deadlines. Sources, runtime libraries/images and generated
fixture wrappers are hashed. Results from timed-out processes are recorded as
incomplete and produce a failing runner exit status. This uses the existing
script loader rather than introducing a project/module migration in the same
change. No runtime, bootstrap, GC policy or language semantics were changed;
there is no new speculative optimization requiring a formal refinement proof.

## Baseline: 2026-10-12

The [evidence record](../measurements/logue-performance-20261012.json) binds the
source and installed runtime hashes, normal-GC measurements, separate allocation
runs, observed state fields and digests of the fully checked states. All 28
existing game assertions, five Python oracle/checker tests, 75 benchmark
checkpoints and the live replacement check pass. A forced one-second timeout
records an incomplete run and returns status 2. This is a local baseline, not a
before/after speedup or a release qualification.

| Case | Timed operation | Median thread CPU |
| --- | --- | ---: |
| Quiet, 0 monsters | 16 turns | 26.80 ms |
| Sparse, 2 monsters | 16 turns | 316.23 ms |
| Crowded, 8 monsters | 16 turns | 2,603.14 ms |
| Long, 2 monsters | 64 turns | 1,303.22 ms |
| Wide, 8 monsters | 16 turns | 1,737.21 ms |
| Generation, seed 12345 | Five levels | 2,524.86 ms |
| Generation, seed 777 | Five levels | 2,201.04 ms |

The wider arena delays contact, so its lower time does not mean larger worlds
are cheaper. Likewise, the first arena batch includes approach; later batches
include settled combat and different message volumes. Whole-process peak RSS
for these cases is roughly 1.07–1.09 GiB, dominated by a workload that also loads
and compiles the sources. It does not isolate game memory.

### Findings and next target

The crowded case's batch medians are **1.35, 2.08, 3.13 and 4.66 seconds**. Its
message counts are **9, 41, 73 and 105** after 16, 32, 48 and 64 turns. The
commented six-message contract is violated: `msgTrim` drops only one message per
turn, while multiple monsters can append messages. In addition, `msg` performs
an identity `collect:` and an always-true `select:` whenever the log exceeds six.
These are concrete application-level causes of growing work, not evidence yet
that method dispatch or GC is the primary bottleneck. The oracle explicitly
preserves this current behavior; passing it does not endorse the six-message
contract as implemented correctly.

**First fix the bounded message log and remove its redundant traversals**, retaining
current sends and ordinary replaceability. Keep this baseline for a paired
comparison and update the oracle to the intended bounded-log rule. Then profile
monster lookup and update: `monsterAt` filters the whole Vector to find one
occupant, and `updateMonster` rebuilds the Vector for a replacement. Those are
plausible collection/iteration targets, but need profiling after the message
problem is removed.

The first 16-turn allocation-only batch charges about **0.11 MiB / 5,888 objects**
for the quiet case, **1.05 MiB / 47,976 objects** for sparse, and **6.88 MiB /
326,918 objects** for crowded. These include the measured loop/instrumentation
and are not retained memory. Quiet post-GC object counts also rise by 36 per
checkpoint from harness retention, so raw growth in another case cannot be
attributed wholly to the game or Store history. Retention needs a separate
controlled investigation if it remains significant after fixing the log.

This original baseline included no game or language optimization. The
[follow-up](../archive/2026-10-runtime-refactors/logue-message-log.md) records the bounded-message correction and
post-fix profiling, separately from these historical measurements.
