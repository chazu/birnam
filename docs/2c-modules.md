# Modules

Birnam assembles itself at runtime from module dylibs. `src/kernel/module.zig` is
the loader; `src/kernel/id_api.zig` and `src/kernel/module_abi.zig` define the
ABI a module uses to talk to the kernel. The language, the query engine, and the
PEG façade are all modules — the kernel knows nothing about them until they load.

> Implementation: `src/kernel/module.zig`, `src/kernel/id_api.zig`,
> `src/kernel/module_abi.zig`; module units in `src/modules/*`. Concept:
> [The Vision](10-vision.md) tenet 3.

## The loader — `module.zig`

```zig
const InitFn = *const fn (*ShellContext, *const IdApi, u64) callconv(.c) void;

const Manager = struct { allocator, modules: ArrayList(Module), api: IdApi, shell_ctx };
const Module  = struct { eid: u64, name, lib: std.DynLib, init_fn: InitFn };
```

`Manager.load(path)`:

1. `std.DynLib.open(path)` — dlopen the dylib.
2. `lib.lookup(InitFn, "birnam_module_init")` — dlsym the required export.
3. `table.alloc()` — mint the module's eid.
4. Call `init_fn(shell_ctx, &api, eid)` — the module receives its eid here, and
   uses the `IdApi` to install methods and register a REPL handler.
5. Record the `Module`.

`unload` retracts every fact tagged with the module's eid (via
`factsOfModule` → `retractAllOf`), dlcloses, and drops the registry entry.
`reload` does both in sequence. This is the [vision's](10-vision.md) "module load
= merge facts, unload = retract" made concrete — the `m` column on every fact
([The Fact Model](11-fact-model.md)) is the unit of ownership.

## The contract: `birnam_module_init`

Every module dylib exports exactly one symbol:

```zig
export fn birnam_module_init(ctx: *ShellContext, api: *const IdApi, module_eid: u64) callconv(.c) void
```

`ShellContext` carries two callbacks — `log` and `set_repl_handler`. The module
caches `api`, installs its methods (asserting `[vt sel closure]` facts under its
own `module_eid`), and optionally registers a REPL handler. That is the whole
surface between the kernel and a module.

## The IdApi — `id_api.zig` / `module_abi.zig`

`IdApi` is a C-compatible extern struct of function pointers and well-known eid
constants — the kernel's entire exported surface. `module_abi.zig` is a mirror of
the same struct that modules `@import` (so a freeze-generated wrapper compiles
without the kernel source). `populate()` (after `setInterner`) wires every
function pointer; `setRegistry` installs the rule registry.

The major API groups:

- **Identity / allocation** — `alloc_eid`, `promote`/`demote`,
  `alloc_object(vt, n_slots)`, `make_closure(fn, data)`, `object_to_ref`.
- **Strings / interning** — `intern(name, len)`, `make_string`, `symbol_name`.
- **Fact store** — `assert_fact`, `retract_fact`, `query_eavt(e, a)`,
  `facts_of_module`.
- **Send / dispatch** — `send0`/`send1`/`send2`/`send3`/`sendN`.
- **Object slots** — `slot_at`, `set_slot`, `object_at_eid`.
- **Query** — the full builder (`make_query`, `query_add_triple`,
  `query_add_predicate`, `query_add_find`, the negation/disjunction begin/end
  pairs, `query_add_find_aggregate`, the `asof`/`since`/`between`/`in_module`
  setters), `query_execute`, the `result_*` accessors, `save_query`/`run_query`,
  `register_pred`/`register_fn`. See [Datalog](29-datalog.md).
- **Promotion / FFI** — `promote_eid`, `demote_eid`, `hotness_of`,
  `is_materialized`, `ffi_enter`/`ffi_exit`.
- **PEG** — `peg_install_send_api` and the pattern constructors.
- **Boxed values** — `make_float`, `make_rrb`/`rrb_with`/`rrb_len`/`rrb_nth`, the
  map constructors (`make_map`/`map_put`/`map_get`/…), `make_reduced`,
  `make_range`, the string-op batch.
- **Snapshots (15.R)** — `snapshot_now`, `snapshot_as_of`, `snapshot_get`,
  `snapshot_release`, `set_retention`, `excise_entity`.
- **Temporal (16 Tier E, appended)** — `current_value_as_of`, `cell_history`,
  `basis_now`. See [Reference Cells](2a-cells.md).
- **Bytecode** — `compile_block_as_method`.

## The append-only ABI discipline

The single most important rule for the module ABI: **new `IdApi` fields are
appended at the end of the struct, never inserted mid-struct.** A dylib compiled
against an older `IdApi` reads the struct by offset; inserting a field mid-struct
shifts every later offset and silently breaks already-built modules (and images
that name module deps). This is the same append-only discipline that governs the
[kernel eid block](11-fact-model.md), and it was learned the hard way — a
mid-struct insert in an early phase corrupted dispatch. New capability ⇒ append.
The Phase 16 temporal functions and the Phase 17 string batch both sit at the
struct's end for exactly this reason.

## The standard modules

- `libbirnam_peg` — the PEG engine façade.
- `libbirnam_lang` — the grammar, desugar, compiler, evaluator, REPL handler, and
  the `Module`/`db`/`language` prototypes. See [The PEG Parser and lang frontend](28-peg.md).
- `libbirnam_query` — the Datalog surface. See [Datalog](29-datalog.md).
- `libbirnam_hello` — a smoke-test module.

## Where to go next

- The kernel side these modules call into: [Kernel Overview](20-kernel-overview.md).
- One module's surface in depth: [Datalog](29-datalog.md).
- How module deps are recorded for reload/AOT: [Images, Freeze, and AOT](2e-images-aot.md).
