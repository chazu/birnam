# Kernel Overview and Boot

Part 2 walks the implementation file by file. This page is the map: what the
frozen Zig kernel contains, how `bootstrap.boot` brings it up, and how the shell
executable wraps it. Read it first — the later Part 2 pages assume the topology
described here.

> Implementation: `src/kernel/bootstrap.zig`, `src/shell.zig`. Concepts:
> [The Vision](10-vision.md), [Reification](12-reification.md).

## The three structs that are the kernel

Everything in a running Birnam hangs off three kernel structs, all initialized
before bootstrap:

- **`FactStore`** (`fact_store.zig`) — the fact database: the five/six indexes,
  the tx and basis counters, history. See [The Fact Store](22-fact-store.md).
- **`ObjectTable`** (`object_table.zig`) — a sparse `u64 eid → Ref` map for
  materialized objects, vtables, and closures, plus per-eid hotness counters.
  Also the eid allocator (`next_eid`, starting at `USER_EID_START`). See
  [Object Table, Promotion, and Demotion](25-promotion.md).
- **`Interner`** (`interner.zig`) — the bidirectional string ↔ eid table for
  symbols and selectors. See [Interner](24-interner.md).

They are wired together at boot rather than holding each other statically: the
fact store learns the object table (for lazy fact-eid minting) and a mutate hook;
dispatch captures all three as globals.

## The boot sequence

`bootstrap.boot` has the signature:

```zig
pub fn boot(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !Kernel
```

It returns a `Kernel` struct holding Refs to the freshly-built kernel vtables.
The ordered stages (matching `VISION.md` §Bootstrap):

1. **Provision the rule registry** for the query/IdApi layer (Phase 7.1).
2. **Wire the mcache invalidation hook** — `store.on_attr_mutate = onAttrMutate`,
   so any fact change on a selector flushes the affected method-cache entries.
3. **Wire the eid allocator** — `store.setEidAllocTable(table)` for lazy fact
   reification.
4. **Allocate the kernel vtables.** For each of the ~12 kernel vtables
   (`VTABLE_VT`, `OBJECT_VT`, `SELECTOR_VT`, `CLOSURE_VT`, `VECTOR_VT`,
   `ASSOC_VT`, `NIL_VT`, `RRB_VT`, `FLOAT_VT`, `FFI_PTR_VT`, `MODULE_VT`,
   `ERROR_VT`): allocate a struct with zero user slots, set its `-1` slot to
   `mkEid(VTABLE_VT)`, and register it in the object table. `vtable_vt`'s own `-1`
   slot points at itself — the fixed point.
5. **Assert `:vtable` facts** so the cold dispatch path can resolve eid-tagged
   receivers to their vtable.
6. **Assert the `:delegate` chain.** `OBJECT_VT` is the top (no `:delegate`
   fact — absence = top, per Date discipline); everything else delegates to it;
   `TRUE_VT`/`FALSE_VT` delegate to `BOOLEAN_VT`.
7. **Assert `[NIL :vtable NIL_VT]`** so sending to `nil` is legal.
8. **Install the kernel methods.** For each `(vt, sel, fn)`: allocate a closure
   (`[vt_ref][method][data]`), mint an eid, register it, assert the method fact.
   The five bootstrap methods are `objectVtableImpl`, `objectInitImpl`,
   `vtableLookupImpl`, `vtableDelegatedImpl`, `vtableMethodAtPutWithImpl`.
9. **Seed selector and attribute names** into the interner, so user code can
   `intern("lookup:")` and get `SEL_LOOKUP` back.
10. **Enable dispatch** — `initDispatch(store, table, allocator)` sets the
    `send.zig` globals and clears the mcache. This must be last among the core
    stages: `bind` reads the facts asserted above.
11. **Phase 8+ tail.** Register built-in predicates, init promotion, install the
    Phase 9.5 module methods, the cardinality schema, the binary-format readers,
    and the Phase 10/10.1 language vtables (booleans, blocks, smallint, grammar,
    PEG, binding/scope, the AST kind vtables).

Boot order is load-bearing: the three structs and the index arena must exist
before any `assert`; `initDispatch` must come after the method facts; interner
seeding must precede dispatch so dylib-time `intern()` calls resolve to kernel
eids.

## The shell

`src/shell.zig` is the Mode-1 host executable. Its `main`:

1. Branches to the AOT driver if `--aot` is present (see
   [Images, Freeze, and AOT](2e-images-aot.md)).
2. Allocates the kernel infrastructure: `FactStore.init` + `enableIndexArena`,
   `ObjectTable.init`, `arenas.init` (the segmented value-heap arenas — see
   [Memory Management](2f-memory.md)), `Interner.init` pointed at the strings
   arena.
3. Calls `bootstrap.boot`.
4. Populates the `IdApi` export table (`id_api.populate()`, after
   `setInterner`) and creates the module `Manager`. See
   [Modules](2c-modules.md).
5. Loads each module dylib named on argv — `manager.load(path)` dlopens it, mints
   its eid, and calls its `birnam_module_init`. A module installs methods on
   kernel vtables and registers a REPL handler.
6. Runs the REPL loop: print `birnam>`, read a line, handle `:`-commands
   (`:save`, `:load`, `:prune`, `:gc`, `:in`, `:retain`, `:excise`, `:asof`,
   `:freeze`) directly, otherwise hand the line to the registered handler. At the
   line boundary it runs growth-gated GC and spills old retained history epochs.

### Kernel vs. shell vs. module — the division of labor

- **Kernel** (`bootstrap.boot`): wires dispatch, materializes the kernel
  vtables, installs the five bootstrap methods, seeds names, enables `send`.
- **Shell**: allocates infrastructure, owns the module manager (dlopen / eid
  assignment / unload), owns the REPL loop and the image `:`-commands.
- **Modules** (dylibs): register the REPL handler, install user-facing methods on
  kernel vtables, define rules and prototypes. The language itself
  (`libbirnam_lang`) is a module — the kernel knows nothing about Birnam syntax.

## Where to go next

- The value words everything passes around: [Ref Encoding Internals](21-ref-impl.md).
- The database under it all: [The Fact Store](22-fact-store.md).
- How a loaded module talks to the kernel: [Modules](2c-modules.md).
