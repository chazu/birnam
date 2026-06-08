# Compiler, Bytecode, and VM

How a typed AST becomes executable bytecode and runs. `src/kernel/bytecode.zig`
defines the instruction set, `src/kernel/compiler.zig` lowers AST to bytecode and
stores it as facts, `src/kernel/vm.zig` holds the execution state, and
`src/kernel/closure.zig` defines closures and block captures. (Top-level forms
are tree-walked by the lang module; *methods* run on this stack VM — see
[The PEG Parser and lang frontend](28-peg.md).)

> Implementation: `bytecode.zig`, `compiler.zig`, `vm.zig`, `closure.zig`,
> `dispatch.zig`. Concept: [Reification](12-reification.md) — bytecode is facts.

## The instruction set — `bytecode.zig`

`FORMAT_VERSION = 1` (recorded in the [image](2e-images-aot.md) manifest).
Opcodes are one byte; operands are little-endian:

| Opcode | Operands | Effect |
|---|---|---|
| `PUSH_CONST` | u16 pool-idx | push constant pool entry |
| `PUSH_SMALLINT_I32` | i32 | push a tagged SmallInt (no pool entry) |
| `PUSH_LOCAL` | u8 slot | push frame local |
| `STORE_LOCAL` | u8 slot | pop → frame local |
| `POP` | — | discard top |
| `LOAD_SELF` | — | push the frame's receiver |
| `LOAD_CONST_TRUE/FALSE/NIL` | — | push the singleton |
| `SEND_0` | u32 sel, u16 site | unary send |
| `SEND_1` | u32 sel, u16 site | binary send (1 arg) |
| `SEND_N` | u32 sel, u8 argc, u16 site | keyword/n-ary send |
| `JUMP` | i16 offset | unconditional pc-relative jump |
| `BRANCH_IF_FALSE` | i16 offset | pop; jump if NIL or FALSE |
| `MAKE_BLOCK` | u16 pool-idx | allocate a block closure |
| `INVOKE_BLOCK` | u8 argc, u16 site | invoke a block on the stack |
| `RETURN` | — | return top of stack |
| `RETURN_FROM_METHOD` | u8 depth | non-local return, unwind `depth` frames |

The `site` operands index call sites for mcache sizing. The constant pool is an
RRB vector of Refs (literals, block-method refs, symbols), deduplicated by linear
scan at compile time.

## The compiler — `compiler.zig`

Input is an AST root eid (from the lang desugar pass); output is a
`CompiledMethod` carrying `bytecode`, `constant_pool`, `arg_count`,
`local_count`, `send_site_count`, nested `block_children`, and `captures`.

`compileExpr` dispatches on the AST node's vtable (`AST_LITERAL_VT`,
`AST_KEYWORD_SEND_VT`, `AST_BLOCK_VT`, `AST_LET_VT`, `AST_ASSIGN_VT`,
`AST_VAR_REF_VT`, `AST_RETURN_VT`, `AST_STMT_SEQ_VT`, …). Notable lowerings:

- **Blocks** compile to a separate `CompiledMethod`; the parent reserves a pool
  slot for the block-method eid and emits `MAKE_BLOCK`.
- **Statement sequences** pop each non-final result (`POP`); the last value
  remains on the stack.
- **Variable references / assignment** resolve a name to a local slot via
  `resolveVarSlot`, which walks the enclosing `parent_state` chain on a miss,
  allocating a fresh local and recording a `CaptureSpec(parent_slot → local_slot)`
  at each level it crosses.

`materialize` mints the method eid and asserts the [reified](12-reification.md)
method facts:

```
[method :vtable CLOSURE_VT]
[method :bytecode-stream <string>]
[method :constant-pool <rrb>]          (if non-empty)
[method :method-body-ast <ast-eid>]
[method :method-arg-count <n>]
[method :method-local-count <n>]
[method :method-send-site-count <n>]
[method :block-parent-method <eid>]    (blocks only)
[method :method-captures-ptr <ptr>]    (if captures.len > 0)
```

The bytecode bytes and the closure wrapper are routed to the swept
[arenas](2f-memory.md).

## Execution state — `vm.zig`

```zig
const Vm = struct {
    frames: std.ArrayList(Frame),
    pc: usize,
    bytecode: ?[*]const u8,          // current frame's bytecode bytes
    current_self: Ref,
    unwinding: bool,                 // non-local return in flight
    unwind_value: Ref,
    unwind_target_remaining: u8,
    bump_arena_slot: ?*anyopaque,
    next_frame_id: u64,
    recursion_guard: u32,            // for bind (Phase 7.5)
    ffi_pause: u32,                  // blocks promotion/demotion during FFI
};

const Frame = extern struct {
    bytecode: ?[*]const u8, pc: usize, method_eid: u64, self_ref: Ref,
    bp: usize, local_count: u8, _pad: [7]u8,   // align struct size to 8
    parent_method_frame: ?*Frame, frame_id: u64,
};
```

Frame capacity is reserved up front so frame pointers stay stable during the
dispatch loop. `recursion_guard` and `ffi_pause` live here (migrated from kernel
globals, per-thread for the Datomic-shape future).

## Closures and captures — `closure.zig`

```zig
const Closure = extern struct {
    method: *const anyopaque,   // Zig fn ptr, cast to Imp at the call site
    data: Ref,                  // captured state; NIL for plain kernel methods
};
```

Block closures carry a `BytecodeClosure` payload (`method_eid`,
`parent_self_ref`, `parent_frame_id`, and an inline capture tail). A `CaptureSpec`
is `(parent_slot, local_slot)`:

- At compile time, `resolveVarSlot` emits a `CaptureSpec` for each captured outer
  variable.
- At `MAKE_BLOCK`, each spec copies `locals[parent_slot]` into the block's
  capture tail.
- At `runMethod` entry, each spec copies `captures[i]` into `locals[local_slot]`.

`parent_self_ref` is the enclosing method's receiver, so `self` inside a block is
the Smalltalk `self`. `parent_frame_id` plus the `RETURN_FROM_METHOD` depth
operand target the right frame for a non-local return even if the stack churned
between `MAKE_BLOCK` and the `^`.

## Non-local return

> `runMethod`, `handleSendUnwind`, and `reifyStack` (below) live in
> `dispatch.zig`, not `vm.zig` — `vm.zig` defines the structs and opcode
> semantics; `dispatch.zig` drives the frame stack around each invocation.

`RETURN_FROM_METHOD` sets `vm.unwinding = true`, stores the value, and sets
`unwind_target_remaining = depth`. Each `SEND_*` checks `handleSendUnwind` and
each `INVOKE_BLOCK` decrements the remaining depth; when it reaches zero the
catching frame clears the flag and returns the unwind value as its result.

## Reified stack frames

`reifyStack` (Phase 11.6) materializes the live frame chain as transient
fact-entities (`:frame-pc`, `:frame-method`, `:frame-self`, `:frame-parent`),
so the call stack is itself queryable — reflection over execution state, in the
same fact mechanism as everything else.

## Where to go next

- How these methods are found and invoked: [Dispatch](26-dispatch.md).
- Where the AST comes from: [The PEG Parser and lang frontend](28-peg.md).
- How bytecode survives a save/load: [Images, Freeze, and AOT](2e-images-aot.md).
