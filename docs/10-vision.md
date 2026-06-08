# The Vision: A Relational Fact-Store Substrate

> Authoritative source: `VISION.md` (locked design). This page is the
> reader-facing summary of *why* Birnam is shaped the way it is. When this page
> and `VISION.md` disagree, `VISION.md` wins.

## The one idea

Birnam is a complete, malleable computing environment in the COLA / *idst* /
Piumarta tradition — a small kernel that brings up a self-describing object
system you can rewrite while it runs. The twist that defines the project:

> **The image is a database.** Code, data, history, modules, and schema all live
> as facts `[entity attribute value tx op module]` in persistent (HAMT / RRB)
> indexes.

Everything else follows from that single decision. History, reflection,
queries, modules, and snapshots are not separate subsystems bolted onto a
runtime — they are all the *same* mechanism (facts in indexes) viewed from
different angles.

## What "everything is a fact" buys you

In most languages these are five unrelated features built by five different
subsystems:

| Question | In Birnam it is… |
|---|---|
| "What changed between two points in time?" | a query over the `tx` column |
| "What methods does this object respond to?" | a query over vtable facts |
| "Who calls `foo:`?" | a query over method-body facts |
| "Save the whole running system to disk." | serialize the index roots |
| "Unload this module." | retract every fact whose `module` column matches |

Because they are one mechanism, they *compose*. Time-travel works on code as
well as data. Reflection works on history. A snapshot captures the compiler,
the grammar, and your objects in one move — because all three are just facts.

## The core tenets (condensed)

These are the load-bearing commitments. The full list lives in `VISION.md`.

0. **The image is a database.** Facts in HAMT/RRB indexes. Everything follows.
1. **Everything is an object.** Vtables, selectors, modules, source, bytecode,
   method caches, history records — all entities, all reachable via `send`.
   See [Reification](12-reification.md).
2. **Bootstrap is minimal and visible.** A small frozen Zig kernel (id model +
   HAMT) brings itself up; everything above loads as modules-of-facts.
3. **Modules assemble the system at runtime.** An image is a set of fact subsets
   tagged by module, not a monolith. Load = merge facts; unload = retract.
4. **Live mutation is first-class.** Adding or replacing a method is asserting a
   fact. The REPL is just an object whose method dispatches messages.
5. **History is free, prunable, and queryable.** Every assert/retract carries a
   `tx`. Time-travel is "as-of `tx`". Pruning is excision. No bolt-on log.
   See [Time, History, and Temporal Queries](15-time-travel.md).
6. **Reflection is query.** Introspection is Datalog, not bespoke walks.
7. **Three execution modes.** Live image, bytecode VM, native AOT — same source,
   same fact store, different switches.
8. **Alien code is absorbable.** Any `.so`/`.dylib` can be loaded, introspected,
   and incrementally wrapped. Wraps are facts: persistent, queryable, diffable.
   See [FFI](2d-ffi.md) (Part 2).

## The malleability boundary

Birnam draws one hard line, in the same spirit as Piumarta's C boundary — just
drawn higher up the stack.

- **Frozen layer (a few thousand LOC of Zig):** HAMT, RRB, the fact store, the
  indexes, the object table, the tx engine, tagging, allocation, the
  materialized-struct arena, dispatch mechanics, mcache, and the bytecode
  walker. Changing any of these means rebuilding the kernel.
- **Malleable tower (everything above, fact-shaped):** vtables, methods,
  *dispatch policy itself* (`bind` is a method), promotion heuristics, schema,
  the parser, the AST, the compiler, the VM-level method bodies, modules, the
  REPL, history policy, pruning policy.

The metacircular property holds: you can rewrite `bind` while the system runs,
replace the compiler with one written in Birnam, and edit the grammar that
parses your own source. The substrate boundary sits higher than Piumarta's, so
the malleable layer is correspondingly richer — relational and historical, not
merely structural.

## What Birnam is not (for now)

- No multi-writer / distributed images. Single-process, single-writer; all
  transactions serialized through one channel.
- No threads / actors yet. **Deferred, not refused** — the locked target is the
  Datomic shape (one transactor thread, lock-free reader threads). Nothing
  post-Phase-11 forecloses it.
- No sandboxing.
- Not trying to beat C. Smalltalk-VM ballpark is the explicit performance goal.
  See [The Performance Model](18-performance-model.md) for where that ballpark
  holds and where the substrate costs more.

## Where to go next

- The mechanics of a fact: [The Fact Model](11-fact-model.md).
- Why code itself is data: [Reification](12-reification.md).
- The object/message layer on top: [The Object Model](13-object-model.md).
