# HAMT and RRB

The two persistent data structures from the [concept page](16-persistent-structures.md),
at the implementation level. `src/kernel/hamt.zig` (~571 LOC) is the map/index
workhorse; `src/kernel/rrb.zig` (~382 LOC) is the sequence workhorse.

> Implementation: `src/kernel/hamt.zig`, `src/kernel/rrb.zig`. Concept:
> [Persistent Data Structures](16-persistent-structures.md).

## HAMT — `hamt.zig`

A Hash Array Mapped Trie in the CHAMP shape (Steindorfer & Vinju 2015).

```zig
pub fn Hamt(comptime K: type, comptime V: type, comptime Ctx: type) ...
```

Generic over key, value, and a `Ctx` providing `hash(K) u64` and `eql(K, K)
bool`. The fact store instantiates it many times — e.g. `Hamt(u64, ValueCell,
U64Ctx)` and the nested index maps in [The Fact Store](22-fact-store.md).

### Node layout (CHAMP)

```zig
const Node = struct {
    data_map: u32,                 // bitmap: which slots hold inline KV pairs
    node_map: u32,                 // bitmap: which slots hold child subnodes
    kvs: []KV,                     // dense array of present KV pairs
    subnodes: []*const Node,       // dense array of present children
    collision: bool,               // true => collision bucket
};
const KV = struct { key: K, value: V, hash: u64 };
```

- **32-way branching** — 5 bits of hash per level.
- **Two disjoint bitmaps** (the CHAMP refinement): a slot is *either* an inline KV
  *or* a child pointer, never both. `data_map` and `node_map` are disjoint.
- **Popcount compression** — the array index for a slot is
  `popcount(bitmap & (slot_bit - 1))`, so only present entries occupy space.
- **`MAX_DEPTH = 13`** levels of 5-bit branching (the last level consuming the
  partial bits of a 64-bit hash); once the hash is exhausted, collision buckets
  handle full-hash collisions with a linear scan.

### Public API

```zig
empty(allocator, ctx) Self
get(key) ?V
assoc(key, value) !Self        // insert/update → new root
dissoc(key) !Self              // remove → new root
iter() Iterator                // depth-first
count() usize
```

`assoc`/`dissoc` path-copy: each node from root to the touched leaf is freshly
allocated (`dupKvs`/`dupSubnodes`), while every unchanged sibling subtree is
reused by pointer. The old root stays valid — this is the structural sharing the
whole substrate depends on.

## RRB — `rrb.zig`

A Relaxed Radix Balanced tree (Bagwell & Rompf 2011).

```zig
pub fn Rrb(comptime V: type) ...   // generic over element type only
```

Used for vectors, source ropes, the image log, and every fact cell's history
vector.

### Node layout

```zig
const Node = struct {
    is_leaf: bool,
    values: []V,                   // used iff is_leaf
    children: []*const Node,       // used iff !is_leaf
};
```

- `BRANCH = 32`, `LOG_BRANCH = 5`, `MASK = 31`.
- A `shift` field tracks the bit offset for radix descent (`shift = 0` → root is a
  leaf; `shift = 5` → children are leaves; etc.).
- **Current state: strict radix**, not yet fully relaxed — all leaves at the same
  depth, non-rightmost branches full. Rightmost-partial branching makes append
  efficient.

### Public API

```zig
empty(allocator) Self
nth(idx) V                     // O(log32 N) radix descent
set(idx, value) !Self          // O(log32 N) update via path-copy
push(value) !Self              // amortized O(1) append
pop() !Self                    // O(log32 N)
concat(other) !Self            // currently naive O(N) — relaxed-radix TODO
splitAt(idx) !struct{Self,Self}// currently naive O(N)
iter() Iterator
count() usize
```

`nth` extracts `(idx >> shift) & MASK` at each level and descends. `set`/`push`
path-copy the spine, sharing untouched children. `concat`/`split` are O(N) for
now (the relaxed-radix optimization from Bagwell & Rompf is deferred until a
workload demands it).

## Structural sharing in one sentence

Both structures, on any update, copy only the nodes on the root-to-leaf path and
reuse everything else by pointer — so the previous version remains a valid,
zero-copy snapshot. That single property is what makes
[time-travel](15-time-travel.md), [image save](2e-images-aot.md), and cheap
[history](22-fact-store.md) all fall out of the substrate rather than being built
separately.

## Where to go next

- What the HAMT indexes store: [The Fact Store](22-fact-store.md).
- What the RRB sequences become at the surface: [Collections](17-collections.md).
- How abandoned spine nodes are reclaimed: [Memory Management](2f-memory.md).
