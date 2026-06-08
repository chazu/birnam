# Persistent Data Structures

Two persistent (immutable, structurally-shared) data structures form the entire
kernel substrate: the **HAMT** for maps/sets/indexes, and the **RRB-Tree** for
sequences. No bespoke structures exist above them. This page explains what they
are, why immutability with structural sharing is the foundation that makes
[snapshots](15-time-travel.md) and [history](11-fact-model.md) cheap, and why
exactly these two.

> Implementation: `src/kernel/hamt.zig`, `src/kernel/rrb.zig`. Design:
> `VISION.md` §Persistent Collections.

## Why persistent (and what that word means)

"Persistent" here is the data-structures sense, not the disk sense: an update
produces a *new* version while the old version remains valid and untouched. The
two versions share all the structure they have in common — only the path from the
root to the changed leaf is copied ("path copying"). The rest is pointer-shared.

This single property is what the whole substrate is built to exploit:

- **A snapshot is a root pointer.** Because mutating an index copies only the
  changed path, the previous root still describes the entire store as it was.
  Holding that root *is* holding a zero-copy snapshot — the basis of
  [time-travel](15-time-travel.md) and image save.
- **Diffs are structural.** Two versions differ exactly in the nodes that were
  path-copied; everything else is identical by pointer.
- **History is cheap to keep.** Old roots cost only the nodes that later updates
  replaced — not a full copy per transaction.

The cost side is real and is treated honestly in
[The Performance Model](18-performance-model.md): path-copying five indexes on
every write is *why* a fact write is ~100× a native field write. Persistence
buys cheap snapshots and pays for it at write time.

## HAMT — the map/index workhorse

**Hash Array Mapped Trie** (Bagwell 2000, in the CHAMP shape of Steindorfer).
Used for *all* fact indexes (EAVT/AEVT/AVET/VAET/MEAT), all maps and sets, and
the object table.

- **32-way branching.** Each node holds up to 32 children, indexed by 5 bits of
  the key's hash at that depth.
- **Popcount-bitmap-compressed nodes (CHAMP).** A node stores *two* 32-bit
  bitmaps — `data_map` (slots holding inline key/value entries) and `node_map`
  (slots holding child subnodes) — plus dense arrays of only the present entries
  and children, so there is no wasted space for empty slots. The dense index is
  `popcount(bitmap & (slot_bit - 1))`. Separating data from nodes is what makes
  CHAMP's iteration and equality faster than a single-bitmap HAMT.
- **O(log₃₂ n)** lookup/insert/delete — effectively constant for any realistic
  size (depth ≤ ~7 for billions of keys).
- **Path copying on insert.** Only the nodes from root to the touched leaf are
  reallocated; siblings are shared with the prior version.

The object table is HAMT-shaped too: a stable-sparse `u64 → Ref` map with the
same shape as a page table, which is what lets eids stay stable while the
underlying storage compacts and relocates.

## RRB-Tree — the sequence workhorse

**Radix Balanced persistent vector** (Bagwell & Rompf 2011) — the structure
family behind Clojure's, Scala's, and Immer's persistent vectors. Today's
implementation (`rrb.zig`) is the **strict** radix variant (Phase 1 spike): no
tail optimization, and relaxed-node balancing is deferred. It backs vectors and
the random-access history log inside each fact cell. (Ropes, buffers, and
source-text are intended future users, not shipped.)

- **O(log₃₂)** lookup, update, and push/pop — effectively constant for any
  realistic size.
- **`concat` and `splitAt` are O(N) today** — correct-but-naive (repeated push).
  The relaxed-radix balancing that would make them O(log₃₂) is a documented
  future optimization, not yet implemented.
- Same structural-sharing property: updating index `i` copies one root-to-leaf
  path and shares the rest.

The fact-cell history vectors (`ValueHistVec`, `PresenceHistVec` in `fact.zig`)
are RRB-Trees, which is why appending a history entry on every write is
affordable and why random-access `asOf:` over deep history stays logarithmic.

## Why only these two

The vision is deliberate about not inventing structures (`VISION.md`): no Ents,
no enfilades, nothing bespoke. Two well-understood, battle-tested persistent
structures cover every need — maps/sets/indexes on the HAMT, sequences on the
RRB-Tree — at roughly ~500 LOC each in Zig. Everything above the kernel composes
from these two. Constraining the substrate to two primitives keeps the frozen
layer small and auditable, which is the whole point of a minimal-kernel design.

## Memory: epochs and reclamation

Persistence means old spine nodes are abandoned on every mutation. Reclaiming
them is *plumbing*, not garbage collection of user values:

- Index nodes allocate into an **epoch arena** (Phase 15.I). A `compact()`
  rebuild swaps to a fresh epoch and frees the old one *wholesale* — you never
  reason about per-node structural sharing.
- The free is gated on the **oldest retained basis-T**, so an epoch is freed only
  when no held [snapshot](15-time-travel.md) still references it. Today
  (current-only) that reduces to "free on compact," but the gate is what keeps
  cheap time-travel from being silently foreclosed.

This is distinct from value-heap GC (which reclaims language values — boxed
floats, strings, closures) and from fact/history *retention* (a temporal policy,
not a reachability question). The three concerns are kept separate on purpose;
see [Memory Management](2f-memory.md) (Part 2).

## Where to go next

- What gets stored in these structures: [The Fact Model](11-fact-model.md).
- Why snapshots are free and writes are not:
  [Time-Travel](15-time-travel.md) and [Performance](18-performance-model.md).
- The collection types users actually touch: [Collections](17-collections.md).
