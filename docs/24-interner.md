# Interner

`src/kernel/interner.zig` is the bidirectional string ↔ eid table. It is what
makes selectors integers (so dispatch compares `u64`s), and what gives the same
name the same identity across a session and across an [image reload](2e-images-aot.md).

> Implementation: `src/kernel/interner.zig`. Related:
> [Object Table](25-promotion.md) (the eid allocator), [The Fact Model](11-fact-model.md).

## What it does

The interner maps strings (selector names, symbols, attribute names) to eids and
back. Interning the same byte sequence twice returns the same eid; a fresh string
gets a user-range eid from the object table, recorded both directions, plus a
`:name` fact so the name is itself queryable.

```zig
pub const Interner = struct {
    allocator: std.mem.Allocator,          // backs the maps + key dups (never reclaimed)
    string_allocator: std.mem.Allocator,   // backs string boxes (the strings arena, Phase 15.1b)
    str_to_eid: Hamt([]const u8, u64, ...), // forward
    eid_to_str: Hamt(u64, []const u8, ...), // reverse
    writer_lock: std.Thread.Mutex,         // single-writer; readers walk lock-free
};
```

Both maps are persistent HAMTs, so reads are lock-free over an atomic root and
only writes take `writer_lock` (forward-compat with the Datomic-shape reader
threads).

## Public API

```zig
init(allocator) Interner
setStringAllocator(a) void                 // point string boxes at the arena, before any intern/seed
seed(store, eid, name) !void               // pre-wire a well-known eid (bootstrap)
intern(store, table, name) !u64            // lock-free probe → recheck under lock → mint
nameOf(eid) ?[]const u8                     // reverse lookup (lock-free)
count() usize
restoreEntry(eid, name) !void              // image load: restore without asserting a fact
```

- **`seed`** is the bootstrap path: it pre-binds the fixed kernel selectors
  (`SEL_LOOKUP`, `SEL_INIT`, …) to their hardcoded eids and asserts their `:name`
  facts, so a later `intern("lookup:")` returns `SEL_LOOKUP` rather than minting a
  duplicate.
- **`intern`** has a lock-free fast path (probe the forward map); on a miss it
  takes the lock, re-checks (guarding against a racing intern), dups the string,
  mints an eid via `table.alloc()`, records both directions, boxes the name with
  `mkString`, and asserts the `:name` fact.
- **`restoreEntry`** is the [image](2e-images-aot.md)-load path: it rebuilds the
  name↔eid bijection *without* asserting a fact (the facts are replayed
  separately), and guards against a restored user-eid shadowing a kernel
  selector.

## Why bidirectional, and why it persists across reload

Two directions are needed because both questions are hot: "what eid is this
name?" (the compiler, every time it emits a `SEND`) and "what name is this eid?"
(the REPL printer, reflection, error messages).

Persisting the table in the image (the v5 `.bim` interner section) is what keeps
*identity* stable across a save/load: a method installed under selector
`#squared` must resolve to the same eid after reload, or every stored method fact
would dangle. `restoreEntry` re-establishes exactly that mapping before the facts
replay. See [Images, Freeze, and AOT](2e-images-aot.md).

## Where to go next

- The eid allocator the interner mints from: [Object Table](25-promotion.md).
- Why selectors-as-eids makes dispatch fast: [Dispatch](26-dispatch.md).
- How the table survives a reload: [Images, Freeze, and AOT](2e-images-aot.md).
