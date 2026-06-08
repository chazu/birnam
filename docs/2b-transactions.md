# Transactions

`src/kernel/tx_engine.zig` is the consolidated commit path for fact mutations.
Today it is deliberately thin; its shape exists to make the future
single-transactor (Datomic-shape) concurrency model a localized change rather
than a rewrite.

> Implementation: `src/kernel/tx_engine.zig`, `src/kernel/fact_store.zig`.
> Design: `VISION.md` §Non-Goals (ADR-11 concurrency target). Surface:
> [Reference Cells](2a-cells.md) (`db transact:`).

## The commit path

```zig
const OpKind = enum(u8) { assert, retract };
const Op = struct { kind: OpKind, e: u64, a: u64, v: Ref, m: u64 };

pub fn commit(store: *FactStore, op: Op) !u64 {
    return switch (op.kind) {
        .assert  => store.assert(op.e, op.a, op.v, op.m),
        .retract => store.retract(op.e, op.a, op.v, op.m),
    };
}

pub inline fn opAssert(e, a, v, m) Op
pub inline fn opRetract(e, a, v, m) Op
```

`commit` is a single funnel that today calls straight through to
[`FactStore.assert`/`retract`](22-fact-store.md). The `Op` struct and the
`opAssert`/`opRetract` builders give callers a uniform way to describe a mutation
without knowing whether it currently runs inline or, in future, gets queued.

## Atomicity model

Birnam is **single-process, single-writer** ([vision](10-vision.md) §Non-Goals).
All transactions serialize through one channel. Two consequences:

- The `next_tx` and `basis_t` counters are atomic (`std.atomic.Value(u64)`) — not
  because there are concurrent writers today, but so reader paths stay correct
  when the transactor thread lands.
- **Language-level atomicity is by construction.** `db transact: aBlock`
  (see [Reference Cells](2a-cells.md)) simply runs the block; because no other
  writer can interleave, the block's sequence of `set:`/`assert` operations
  commits indivisibly. There is no rollback machinery — there is nothing to roll
  back against, since the single writer never observes a partial state from
  another.

## The forward-compatible shape (ADR-11)

The locked future target is the Datomic shape: one designated transactor thread
owns the tx engine; reader threads access the persistent indexes lock-free; each
thread carries its own `Vm`. Under that model, `commit` becomes a channel send to
the transactor's queue instead of a direct call — and that is the *only* change
the call sites see, because they already speak in `Op`s. The image manifest
already carries a `thread_mode: single | datomic` tag (see
[Images, Freeze, and AOT](2e-images-aot.md)) so the format survives the
transition. Phase 11 shipped single-threaded with the seven "keep-viable" items
(per-thread `Vm`, atomic counters, atomic mcache writes, single-writer interner
with atomic root, etc.) already in place.

## Where to go next

- What a commit actually does to the indexes: [The Fact Store](22-fact-store.md).
- The atomic surface for users: [Reference Cells](2a-cells.md) (`db transact:`).
- The thread-mode tag in the image: [Images, Freeze, and AOT](2e-images-aot.md).
