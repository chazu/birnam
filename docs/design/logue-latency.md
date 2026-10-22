# Logue interactive latency diagnosis — 2026-10-22

Two costs reproduce the reported symptoms: full-map rendering takes seconds,
and synchronous automatic collection occasionally adds roughly half a second
to a turn. This diagnostic pass changed no game or runtime code. These
measurements precede the [runtime lookup improvement](id-default-lookup-fact.md).

[Evidence](../measurements/logue-latency-20261022.json) includes raw timings,
allocation counters, exact probe sources, experiment driver, source/runtime
hashes, and a native sample summary. The checkout was `9b145eee`; execution used
the installed `/Users/chazu/.local/opt/birnam/bin/birnam` on macOS arm64.

## Reproduction

The probe uses the actual Logue functions and Termbox backend in a drained
100×32 controlling PTY. It creates the normal fixed-seed fighter dungeon,
increases HP to survive, measures three full-map draws, traverses the first two
floors three times, then measures 80 ordinary turns and redraws. A separate run
executes 240 alternating `h`/`l` actions through the real `act` function and
renderer. Every action in that run advances the turn counter.

These are operation-to-Termbox timings, not physical display latency or timed
OS keyboard polling. Loading/compilation is outside the phase timers. Automatic
GC remains enabled except in the explicitly named diagnostic control. Result
recording allocates outside each timer and can shift collection boundaries.

| Original implementation, first PTY run | Median wall time | Maximum |
| --- | ---: | ---: |
| Descend logic | 16.55 ms | 18.09 ms |
| Descend redraw | 2,709.75 ms | 2,725.08 ms |
| Ascend logic | 11.04 ms | 13.24 ms |
| Ascend redraw | 2,745.46 ms | 2,768.04 ms |
| Full static map alone | 2,689.21 ms | 2,749.95 ms |
| Ordinary turn logic, 80 turns | 13.04 ms | 14.24 ms |
| Ordinary redraw, 80 turns | 37.89 ms | 628.68 ms |
| Terminal `present`, three isolated calls | 0.03 ms | 0.10 ms |

## 1. Floor changes recompute every tile's appearance

[`drawFullMap`](../../examples/logue/logue.birnam) walks all 60×18 cells,
looks up each tile, and computes its glyph and color through nested language
conditionals. `draw` calls this whenever depth changes. All five levels are
already generated at game start; descending does not generate a dungeon.

[`symEq`](../../examples/logue/logue-core.birnam) implements symbol equality as
`[[a printString] = [b printString]]`. The normal first floor contains 796 walls,
282 floor cells, and two stair cells. Its glyph/color selection therefore makes
4,885 `symEq` calls, or **9,770 symbol `printString` calls**, per full redraw.
This count follows the source branches and independent seeded dungeon model.

Serial, disposable-copy experiments distinguish the costs:

| Experiment | Full-map median wall time |
| --- | ---: |
| Original | 2,689 ms |
| Replace `putCell` body with `nil` | 2,692 ms |
| Replace `symEq` with symbol identity (`==`) | 1,585 ms |
| Return constant tile glyph/color | 1,301 ms |
| Disable automatic GC | 2,209 ms |

Removing the native cell writes barely changes the result. Much of the work
happens in language-level tile lookup, appearance computation and iteration.
Identity comparison cuts substantial work; that experiment produces
byte-identical terminal output for this complete fixture. The constant-appearance
variant deliberately renders the wrong map and serves only to isolate cost.
Neither experiment is a delivered fix or a general equivalence proof.

A three-second native sample also shows dispatch binding, symbol lookup,
allocation guards and memory copying among its prominent leaves. It does not
isolate all remaining redraw cost to one runtime component. The sampled run is
excluded from quiet timing comparisons.

## 2. Automatic GC produces intermittent input stalls

In the counter-instrumented run, otherwise roughly 13 ms turn logic takes
588 ms and 553 ms while allocation charge drops from near 16 MiB to near zero.
The separate movement run has five large stalls, **634–811 ms**, and each also
coincides with a charge reset. They occur in both action processing and redraw.
Movement logic and redraw have medians of 19.89 ms and 39.56 ms respectively.
That run also experienced early host contention: its initial generation/draw
and some early turns have inflated CPU/wall times. Use the original quiet run
for floor-change estimates, not that initial movement frame.

The collector's [default allocation budget](../../rust/birnam-collect/src/lib.rs)
is fixed at 16 MiB. Collection runs synchronously at allocation admission. The
[game loop](../../examples/logue/logue.birnam) completes actions and drawing
before polling the next event, so either phase can delay the next key.

With automatic collection disabled, the 80-turn diagnostic run has no
half-second stalls: maximum logic is 16 ms, maximum redraw 117 ms. This is not
a remedy: its final explicit collection takes **5.87 seconds**. Some shorter
latency variation remains even without automatic GC.

Allocation-only interpretation of that control gives approximately **11.8 MB
of charged allocation per full map**, and roughly **0.32 MB per ordinary turn
plus redraw** in this three-monster fixture. These are allocation charges,
not live heap or RSS. They explain frequent crossings of a 16 MiB budget;
they do not establish a memory leak or identify every retained root.

## 3. Crowded turns add steady game-logic cost

The existing headless runner passes all 34 game assertions and all 13 requested
oracle checkpoints. One reset sample per case, four 16-turn batches for arenas:

| Arena | Median CPU per turn, excluding display |
| --- | ---: |
| No monsters | 1.60 ms |
| Two monsters | 9.80 ms |
| Eight monsters | 45.18 ms |

`monsterChase` may try three candidate positions, each checking occupancy;
`monsterAt` searches a vector, and `updateMonster` rebuilds that vector. These
retain quadratic worst-case work with monster count. Current code already uses
terminating `detect:` and the message log is already bounded: the older reports'
unbounded log and `select:` lookup findings are not current defects.

The fresh synthetic cases establish monster-dependent cost, not a percentage
attribution to each helper or a typical-session average. Seed-12345 five-level
generation separately measures 2.56 seconds CPU; it affects starting a game.

## Priority of follow-up work

1. Simplify canonical symbol comparisons and tile appearance lookup. Then cache
   static map appearance or draw prepared rows/runs, with invalidation for tile
   and style changes. Re-measure actual floor transitions and terminal output.
2. Reduce rendering allocation, then profile collector inventory/trace/retirement
   time and retained roots. Merely postponing collection does not remove its cost.
3. Profile occupancy checks and monster-vector updates before choosing an index
   or alternate update representation. Preserve sequential monster-turn rules.
4. Include interactive redraw and individual-turn tail latency in the performance
   workload: the existing headless batch benchmark cannot see these display
   costs and intentionally collects outside its timed batches.

The experiments are single-process-per-variant diagnostics, not an ABBA release
benchmark. No optimization was integrated, installed, committed or pushed during
this diagnostic pass; the linked follow-up records subsequent runtime work.
Temporary probes and full terminal/sample logs remain under
`/tmp/logue-diagnose-20261022`; exact generated sources are embedded in the
evidence record. The headless run is reproducible with:

```sh
python3 scripts/logue-bench.py --prefix /Users/chazu/.local/opt/birnam \
  --work /tmp/logue-diagnose-20261022-baseline \
  --cases quiet,sparse,crowded,generation --samples 1 --timeout 180
```
