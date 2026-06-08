# Object Table, Promotion, and Demotion

This page covers the read-through caching layer that makes object access fast:
`src/kernel/object_table.zig` (the eid → materialized-struct map and hotness
counters) and `src/kernel/promotion.zig` (materializing a hot entity into a
struct, patching it in place, and demoting it back). This is where the
[performance model's](18-performance-model.md) tier-2 representation lives.

> Implementation: `src/kernel/object_table.zig`, `src/kernel/promotion.zig`.
> Concepts: [Refs / the -1 trick](14-refs.md), [Performance](18-performance-model.md).

## The object table

```zig
pub const ObjectTable = struct {
    allocator: std.mem.Allocator,
    map: Hamt(u64, Ref, U64Ctx),       // eid → materialized Ref (raw pointer)
    next_eid: std.atomic.Value(u64),   // monotonic, starts at USER_EID_START
    hotness: std.AutoHashMap(u64, u32),// per-eid dispatch counter (transient)
};
```

It is **sparse**: an entry exists only for a *materialized* entity. Absence of an
eid means "this object is cold — its content lives in the fact store." Whether an
entity *exists* is a fact-store question, not an object-table one.

```zig
alloc() u64                 // mint a fresh eid (does not touch the map)
get(eid) ?Ref               // materialized Ref, or null if cold
put(eid, r) void            // register a materialized pointer
clear(eid) void             // drop the materialized entry (entity unchanged in store)
iterator()                  // walk live slots (the GC marks through this)
bumpHotness(eid) / hotnessOf(eid) / resetHotness(eid)
```

`alloc` only mints eids; asserting the initial facts is a higher layer's job. The
hotness map is transient (never snapshotted) and saturates at `u32` max.

## Materialized layout

A promoted object is a `[vt_ref][slot_0 … slot_{N-1}]` allocation with the
vtable Ref at offset `-1` (the [-1 trick](14-refs.md)). The object table stores a
pointer Ref to slot 0. Slot order is attributes sorted ascending by eid, so the
layout is derivable from facts with no per-instance metadata — `slotIndexOf(attrs,
a)` binary-searches the sorted attribute list to find a slot.

## Promotion

```zig
pub fn promote(eid: u64) anyerror!void
```

Materializes a cold entity into a struct (no-op if already materialized):

1. Read the entity's `:vtable` fact (error if absent / not an eid).
2. Collect its non-vtable `(a, v)` pairs from `attributesOf`.
3. Sort by attribute eid (the locked deterministic order).
4. Allocate `sizeOf(Ref) * (1 + N)` bytes, aligned; write the vt Ref to slot
   `-1` and each value into slots `0..N-1`.
5. Save the `Layout` (the owned, sorted `attrs` slice) for size-aware free.
6. `table.put` the new pointer.

Promotion is triggered automatically: `maybeAutoPromote(eid)` runs on the cold
dispatch path after bumping hotness, and promotes once the hotness crosses
`g_threshold` (default 100, cached from the `[M_BOOTSTRAP :promote-threshold]`
fact, re-read on mutate) — provided the entity isn't already materialized and no
FFI call is in flight.

## Demotion and in-place patching

The materialized struct is a **read cache that is kept coherent with the facts.**
The fact store fires `on_entity_mutate(e, a)` after every write; `promotion`'s
hook decides between two outcomes:

```zig
fn onEntityMutate(e: u64, a: u64) callconv(.c) void  // the hook
fn tryPatchSlot(e: u64, a: u64) bool                 // in-place same-shape write
pub fn demote(eid: u64) void                         // free struct, re-cold the eid
```

- **Same-shape value write → `tryPatchSlot` (perf-1.A).** If the attribute is
  already in the layout and still present, binary-search its slot and
  `object.setSlot` the new value in place. The entity stays materialized. This is
  what stops cells and counters from being evicted on every assignment.
- **Shape change → `demote`.** A demote fires only on a true shape change: a new
  attribute (struct must grow), an attribute retracted to absent (must shrink),
  the `:vtable` changing, or a whole-entity clear (`a == 0`). Demote frees the
  struct allocation and the layout, and `table.clear`s the eid.

Two guards matter:

- **FFI pin.** Demote is a no-op while `vm.ffi_pause > 0` (`ffiEnter`/`ffiExit`
  bracket every FFI call), so an alien callee never sees a freed pointer. See
  [FFI](2d-ffi.md).
- **Keep hotness on demote (perf-1.B).** Demote does *not* reset the hotness
  counter. A shape-changed entity that was hot is still hot, so its next dispatch
  re-promotes it with the new shape instead of re-earning the threshold.

Both perf-1.A and perf-1.B are the shipped down payment on the write-back tier;
the full argument is in [The Performance Model](18-performance-model.md).

## Where to go next

- The hook's other consumer: [Dispatch](26-dispatch.md) (the mcache).
- Why this caching is bounded by the write-through model:
  [The Performance Model](18-performance-model.md).
- The Ref layout it materializes into: [Ref Encoding Internals](21-ref-impl.md).
