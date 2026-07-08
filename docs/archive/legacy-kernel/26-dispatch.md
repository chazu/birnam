# Dispatch

How a message becomes a method call. `src/kernel/send.zig` resolves a
`(receiver, selector)` to a closure and invokes it; `src/kernel/mcache.zig` is
the method cache that keeps the hot path fast; `src/kernel/dispatch.zig` runs
bytecode methods once bound. This is the implementation of the
[object model's two-tier dispatch](13-object-model.md).

> Implementation: `src/kernel/send.zig`, `src/kernel/mcache.zig`,
> `src/kernel/dispatch.zig`. Eids: `src/kernel/kernel_eids.zig`. Concept:
> [The Object Model](13-object-model.md).

## The Send frame and entry points

```zig
const Send = extern struct {
    selector: u64,        // selector eid
    receiver: Ref,
    n_args: u8,
    state: Ref,           // current "self" (differs in super-sends)
    closure: ?*Closure,   // filled by bind
};

const Imp = *const fn (*Send, [*]const Ref, usize) callconv(.c) Ref;
```

Every method has the same C variadic shape: a `Send` frame, a flat argument
buffer, and a count (locked in ADR-10-Send-Arity). `send0`/`send1`/`send2`/`send3`
are thin packers over `sendN`, which binds then invokes the resolved closure with
`args[0..n_args]`.

## vtableEidOf — receiver → vtable

`vtableEidOf(r)` resolves any receiver Ref to a vtable eid by tag:

| Tag | Resolution |
|---|---|
| **pointer** | read `obj[-1]`, untag as eid |
| **eid** | if materialized, read the struct's `obj[-1]`; else cold EAVT query `[r :vtable ?vt]`. Bumps hotness and calls `maybeAutoPromote` |
| **smallint** | `SMALLINT_VT` |
| **string** | `STRING_VT` |
| **keyword / reserved** | null (no kernel vtable) |

The eid case is where [promotion](25-promotion.md) is driven — every cold
dispatch counts toward the hotness threshold.

## bind — the resolution algorithm

`bind(s: *Send) ?*Closure` fills `s.closure`/`s.state` and returns the closure,
or null for "method not found." It is guarded by `vm.recursion_guard`.

1. **Hot path.** `mcache.lookup(vt_eid, selector)` — on a hit, return the cached
   closure. O(1).
2. **Bind override (Phase 7.5).** Walk the vtable's delegate chain for a `:bind`
   fact (`ATTR_BIND`); if present, invoke that Birnam-level override, which
   returns a closure-eid Ref or NIL (defer). This is metacircular dispatch — you
   can replace `bind` from inside the system. Skipped past a small recursion
   limit. Result is cached.
3. **Cold path.** Walk `cur_vt` and its `:delegate` chain; for each, query EAVT
   `[cur_vt selector ?method_ref]`. The method ref is `mkEid(closure_eid)`;
   dereference it through the object table to a `*Closure`. Cache on hit; return
   null when the chain reaches NIL.

If `bind` returns null and the selector wasn't already `doesNotUnderstand:`,
`sendN` rebinds `SEL_DNU` and invokes it with the missing selector as an eid
`Ref` arg (`mkEid(sel)`) — the [DNU](13-object-model.md) reflective hook. If DNU itself isn't bound, it
returns NIL.

## The method cache — `mcache.zig`

```zig
const SIZE = 1024;                  // direct-mapped, single-threaded
const Entry = struct { vt_eid: u64, sel_eid: u64, closure: ?*Closure, occupied: bool };
fn slot(vt_eid, sel_eid) usize      // vt ^ (sel *% 0x9E3779B97F4A7C15), spread, & 1023
lookup(vt_eid, sel_eid) ?*Closure
store(vt_eid, sel_eid, closure) void
flushSel(sel_eid) void              // O(SIZE): clear entries for one selector
clearAll() void
```

A 1024-entry direct-mapped table keyed `(vt_eid, sel_eid)`. Invalidation is
driven by the fact store: `onAttrMutate(a)` (wired as `on_attr_mutate`) calls
`flushSel(a)` whenever a fact on selector `a` changes — so installing or
replacing a method is immediately visible. A mutation on `ATTR_BIND` flushes the
whole cache (a bind override affects every `(vt, sel)`). Measured: the mcache
probe is effectively free; warm dispatch is ~8 ns dominated by `vtableEidOf`'s
object-table HAMT get. See [Performance](18-performance-model.md).

## dispatch.zig — running a bytecode method

Once `bind` resolves to a bytecode method, `dispatch.runMethod` executes it:

```zig
pub fn runMethod(allocator, store, receiver, method_eid, args, captures) RunError!Ref
```

It loads the method's `:bytecode-stream` and `:constant-pool` facts, seeds locals
with args and copied captures, pushes a `Frame` onto the `Vm` frame stack, and
runs a `while + switch` loop over the opcodes (the eventual shape is tail-call
dispatch per ADR-11). `SEND_*` opcodes call back into `send.zig` (hot path
through the mcache). `bytecodeDispatcherImpl` is the closure body installed for
every compiled method — it recovers `(method_eid, self, captures)` from the
closure data and calls `runMethod`. The opcode set, frames, blocks, and non-local
return are documented in [Compiler, Bytecode, and VM](27-compiler-vm.md).

## Where to go next

- What gets dispatched: [Compiler, Bytecode, and VM](27-compiler-vm.md).
- The receiver→vtable promotion it drives: [Promotion](25-promotion.md).
- The measured cost and the measurement lesson: [Performance](18-performance-model.md).
