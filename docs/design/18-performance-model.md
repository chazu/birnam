# The Performance Model

This page is the honest accounting of what Birnam's substrate costs and why. It
is the reader-facing summary of **ADR-perf-1** (see `ROADMAP.md` for the decision
record). The central idea: the fact store is *both* the durable/queryable/
time-traveled projection *and* the runtime object representation — and that dual
role is the source of both Birnam's power and its performance ceiling.

> Measured 2026-06-07, ReleaseFast on arm64, GPA + index arena (= production
> config). Harnesses: `src/kernel/kernel_bench.zig`,
> `src/kernel/kernel_write_bench.zig`. See [Benchmarking](37-benchmarking.md).

## The foundational tension

In a normal language, an object's fields are memory and the database (if any) is
elsewhere. In Birnam they are the *same thing*: an object's state is facts, and
facts are the durable, queryable, historied store. Promotion to a materialized
struct is a read-through cache; a write is, by default, write-through to all
indexes plus history.

That conflation is what gives you free reflection, free history, free snapshots,
and a uniformly queryable image. It is *also* why mutation is expensive: every
write does database work. The performance model is the story of where that cost
falls and what the tiered representation does about it.

## The measured cost bands

| Operation | Measured | vs. native | Verdict |
|---|---|---|---|
| **Warm dispatch** (`send0` to a trivial method) | ~8 ns/op | ~6.4× a native indirect call | **Fine** — Smalltalk ballpark |
| **Field read** (`currentValue(e,a)`) | ~20 ns/op | ~10.3× a native hashmap get | Acceptable; the real read lever |
| **Field write** (`assert`) | ~1.8 µs/op (~10 KB/fact across 5 indexes) | ~100–360× a native write | The architectural cost |
| **Field `:=`** (retract + assert) | ~3.0 µs/op | — | Two store ops |

**A write costs roughly 100 reads.** That confirms the ADR's 100–1000× write
band, and it is the number that drives every representation decision below.

Dispatch breakdown (so optimization targets the real hotspot): `vtableEidOf`
5.9 ns (of which the object-table HAMT get is 4.6 ns), `mcache.lookup` ≈ free,
full `bind()` 7.8 ns.

## What's inherent vs. fixable

- **Dispatch (~6×).** Fine as-is; further wins come from Phase-18 JIT, not from
  hand-tuning. (See the course-correction below.)
- **Field read (~10×).** Fixable *iff* materialization sticks — a shape/hidden-
  class with inline slot offsets turns a `currentValue` HAMT walk (~20 ns) into a
  fixed-offset slot load (~1–2 ns). This is the V8 "objects-are-not-dicts" move.
  It couples to the materialized-struct tier, not a standalone cache.
- **Field write (~100–1000×).** Only fixable by *not making write-through
  mandatory* — i.e. transients, escape analysis, and a write-back tier. No JIT
  removes "every write updates five indexes plus history" if every write is a
  fact.

## The three-tier representation model (ADR-perf-1 decision)

The resolution is to stop treating "all state is always a write-through fact" as
mandatory, and instead tier the representation:

- **T1 — heap / transient.** No eid, no facts: collections, scalars, method
  locals (frame slots), non-escaping objects. Native speed.
- **T2 — materialized write-back** *(default for escaped user objects)*. The
  struct is the runtime truth; facts are a projection reconciled at a boundary
  (query / tx-commit / save). Per-assignment cost becomes per-*commit* cost.
  `asOf:`/`history` are empty/error here.
- **T3 — temporal write-through** *(opt-in)*. Historied; gets `asOf:`/`history`.
  [Reference cells](2a-cells.md) are the shipped instance of this tier.

Transitions: T1→T2 on escape; T2↔T3 by declaration. **Meta-level facts (vtables,
methods, AST, bytecode) stay write-through** — they're written once and `bind`
dispatches by reading them, so they can't be naïvely write-back. "Default T2"
means *base-level user objects only*. (Write-through ≠ historied: AST and bytecode
are a shipped **no-history** code tier — `asOf:` over them returns null — while
vtables and methods keep history. See [Code-Fact Tiers](19a-code-fact-tiers-plan.md).)
See
[Time-Travel](15-time-travel.md) for the opt-in API (`beTemporal`, prototype
stamping, `db cell:`).

## What has shipped

- **perf-1.A — materialized struct is a mutable read cache.** A same-shape value
  write patches the struct slot in place (attribute binary-searched in the
  layout) instead of demoting. Only a true shape change (attribute added,
  retracted-to-absent, `:vtable` change, whole-entity clear) demotes. This stops
  cells/counters being evicted on every assignment — a down payment on write-back.
- **perf-1.B — demote keeps hotness.** A shape-changed hot entity re-promotes on
  its next dispatch instead of going fully cold, saving the threshold's worth of
  slow cold dispatches per shape change.

Both shipped in commit `edb7572`. The design boundary (perf-1.3) is resolved:
time-travel is opt-in, three tiers as above.

## What is open (deliberately)

- **perf-1.4 — write-back tier.** Hot slots dirty-marked in the struct, reconciled
  to facts at a tx/query/save boundary. This is the lever that moves mutation off
  the database-write ceiling.
- **perf-1.5 — escape analysis / transients.** Locals and non-escaping objects
  never become facts at all (the T1 path).
- **perf-1.6 — folded.** Originally "dispatch inline caches"; *dropped* after
  measurement (see below). The useful remnant — shape/hidden-class for field
  *offsets* — folds into perf-1.4 (needs a stable struct layout) and Phase-18.

These are flagged in the ADR so the projection boundary is decided deliberately
rather than accreted. They are not built yet, and the recommendation is to defer
them until a real mutable-object workload exists to measure against.

## A measurement lesson

Dispatch was first *guessed* at 2–5×, then **mis-measured** at 22× — an artifact
of benchmarking `SEL_VTABLE`, whose method body re-resolves the vtable via
`currentValue` (a nested HAMT walk), so the bench was timing a field read inside
the method, not dispatch. Re-running against a trivial-body method gave the true
~8 ns / 6.4×. The lesson, baked into the harnesses: **isolate the primitive from
method-body work before concluding** — a bad bench-method choice will send you
optimizing a non-problem. (That is why perf-1.6's dispatch inline cache was
dropped: it fixed something that wasn't broken.)

## A safety note that is also a finding

The first benchmarking attempt crashed the machine: a million-iteration
fact-mutation loop exhausted memory, because writes grow memory without bound
(never-freed history + five-index path-copy). That is not just a benchmarking
hazard — it is **ADR-perf-1 finding #1**, and the reason sustained mutation must
be bounded with prune-in-loop plus the index epoch arena. The benchmarking
harnesses are built to make that failure impossible (in-process memory caps);
see [Benchmarking](37-benchmarking.md).

## Where to go next

- Why loading a large program is memory-heavy, and the two levers that bound it:
  [Load-Time Memory](19-load-memory.md).
- The opt-in temporal API this model justifies: [Time-Travel](15-time-travel.md).
- The materialization machinery: [Promotion](25-promotion.md) (Part 2).
- The safe harnesses behind every number here: [Benchmarking](37-benchmarking.md) (Part 3).
