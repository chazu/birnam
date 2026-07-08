# Rung 3 — the front end (peg + lang), and the road to libtcc

> **Status:** Shipped (last updated 2026-06-30). 3a (peg) + 3b (grammar) + 3c
> (C codegen + libtcc) all **shipped**; the post-grammar runtime — stdlib, the full
> datalog surface, GC, error model — and the final codegen/runtime gaps (arithmetic
> dispatch, AOT native output) are **complete** (see the late sections). Builds on
> [Refound on libid](../../design/32-refound-on-libid.md), [Rung 2 — cells](33-rung-2-cells.md),
> and [Foundations before the compiler](34-foundations-before-the-compiler.md). The
> query surface has its own reference: [The relational reading](../../design/37-the-relational-reading.md).

## The proximate goal

Parse birnam → generate **C** → **libtcc** compiles it to hot-loadable native
code (in-process and/or to a dylib/`.so`, per the user's need). This is the
rung-3 target — *not* a bytecode/VM. Everything is a module the siphon loads and
drives through the id api; the front end is built the same way.

The chain now stands: **siphon → libid → store → peg → lang**, and birnam source
parses into a CST entirely through the object model (`api.send`).

## What shipped

### Foundations consumed here

- **String** floor value type (`src/id/id.zig`, commit `0fb62f6`) — an
  uninterned byte string (`String{ptr,len}` + `string_vt` + `mkString`/
  `stringBytes` on the `IdApi`). Source text + literal bytes need this; symbols
  won't do (source isn't internable). Lives in the floor alongside Symbol/
  SmallInt. Prerequisite for peg's message api.

### 3a — the PEG engine (`src/id/peg.zig`, `68c5fbb` + `4f775ee`)

Ported the proven match semantics from the old fact-store kernel's `peg.zig`
(operator set + packrat memo + zero-width guard) onto a **libid-native**
representation (the old file was welded to the fact store, so it was a
port-the-algorithm rewrite, not reuse). Per `docs/34` F5:

- **Patterns are libid objects** (a `Pat` struct payload, read by pointer in the
  hot match loop — fast, no rung-2 dependency). "Grammars as objects."
- **Parse output is a transient CST** (`Node` tree in a caller-owned arena), NOT
  facts. Only a rule application makes a node (a `[start,end)` span labelled by
  rule name, nested rule-matches as children); terminals just advance.
- Operators: `literal / any / eof / charset / seq / choice / star / plus / opt /
  and / not / rule-ref`.

**peg's message (id) api** — peg is driven through `api.send`, so `lang` builds
the grammar without Zig linkage:

- `Peg` facade (exported `"Peg"`): `literal:` / `any` / `eof` / `charset` /
  `ruleRef:` / `seq` / `choice` / `grammar`. Multi-child patterns use a builder
  (`[seq add: child]`) — no collection literals needed yet.
- Pattern combinators on `pattern_vt`: `star` / `plus` / `opt` / `negate` /
  `lookahead` / `add:` / `from:to:`.
- `Grammar`: `rule:is:`, `parse:from:` (source is a `String` → a CST or `nil`).
- **CST as libid `CstNode` objects** (`rule` / `start` / `end` / `childCount` /
  `childAt:`) — message-accessible so `lang` walks results through sends; still
  transient (the Zig `Node` arena is freed right after conversion). Failure →
  the canonical `nil` singleton.

`test-peg` 8/8; the siphon loads `libbirnam_id_peg.dylib` and drives the whole
path (`[0-9]+` grammar → parse `"12345"` → `childCount = 5`) through sends.

### 3b — the birnam grammar (`src/id/lang.zig`, `3bb6121`)

`lang` is a sibling module that `api.import("Peg")` and constructs **birnam's
grammar entirely through peg messages** (`rule:is:` / `seq` / `choice` / `charset`
/ `star` / … via thin Zig helpers over `api.send`). Exposes a `Lang` facade:
`[Lang parse: aString]` → the birnam CST or `nil`.

The **core grammar** (parses the example programs):

- `let x := expr` bindings; `;`-separated statements with optional trailing `;`.
- Blocks: `[:n | body]` (params) and `[| body]` (empty params) — disambiguated
  from sends by the leading-`|` header.
- Sends: everything bracketed — unary `[n printString]`, binary `[a = b]`,
  keyword `[1 to: 20 do: blk]`. A bare `[var]` / `[lit]` is *not* a send (matches
  birnam's bracket rule).
- Literals: integers (with optional `-`), `"strings"`.
- `//` line comments; single-char binops (`= < > & | , + - * / ~`).
- `let` guarded by `!alnum`; `program` requires `eof`, so partial/garbage input
  fails.

`test-lang` 7/7 — parses real fizzbuzz/primes lines + a multi-statement program
with comments, and rejects malformed input. The siphon loads
`libbirnam_id_lang.dylib` and parses a real let-binding → `program` CST.

(Build note: peg + lang both export `module_init`; a test statically linking both
collided, so the dylib entrypoint is exported only in non-test builds via
`comptime if (!builtin.is_test) @export`.)

## 3c — C codegen + libtcc

### 3c.1 — libtcc as an id module (shipped, `233463b`)

`src/id/tcc.zig` — the compiler backend, statically linking the vendored libtcc
and exposing it through the object model. **No general FFI is needed**: compiled
birnam uses birnam's own Method ABI, so a `tcc_get_symbol` result is cast to the
known `Method` type and installed via `api.method`. (A general "alien"/FFI module
for wrapping arbitrary C libs from birnam is a separate later rung.)

- `[Tcc compile: cSource]` → a `TccUnit` (new + compile + relocate, output in
  memory) or nil; `[unit install: cFnName as: #sel on: vt]` binds the named
  Method-ABI function on `vt`; `[unit symbol: name]` → raw address as a SmallInt.
- `test-tcc` 6/6: compile a Method-ABI C function returning SmallInt 42, install,
  dispatch → 42; bad C → nil. The siphon JITs C → native → dispatch through the
  id api.

### 3c.2 — the C emitter (MVP shipped, `f65ae54`)

`src/id/codegen.zig` — `[Codegen eval: aString]` parses via `Lang`, walks the
CST, recursively emits a Method-ABI C function, compiles it with `Tcc`, installs
+ dispatches, and returns the value. **The proximate goal works end to end:**
birnam source → CST → emitted C → libtcc → native → value, all through the id api.

- MVP subset: integer literals + binary arithmetic `[a op b]` (`+ - * /`),
  recursive. Self-contained C — SmallInt tag/untag inlined as `GETV`/`MKV` macros,
  so **no IdApi callback yet**. `skip()` descends the single-child wrapper rules
  (`expr`/`primary`/`message`); node text via `source[start..end]` from the CST
  spans (so the deeply-nested CST was handled in the emitter, no desugar pass —
  for now).
- `test-codegen` 6/6: `42`; `[3+4]=7`; `[[3+4]*2]=14`; `[100/4]=25`; `[-10+2]=-8`;
  out-of-subset input → nil. The siphon runs the full stack.

### 3c.3 — beyond arithmetic (in progress)

Grow the emitter to real birnam: **sends to objects, variables (self/args/let),
blocks, comparison + boolean ops, strings**. The pivot: emitted C must **call
back into libid** (construct values, `send`, look up selectors).

**Step 1 — the pre-relocate symbol window SHIPPED (`e8ebc93`).** `compile:` did
new+compile+relocate in one shot, with no window to `tcc_add_symbol` the runtime
entry points before `tcc_relocate` freezes the code. Added a STAGED path on `Tcc`
that opens it (the all-in-one `compile:` is unchanged — arithmetic codegen +
existing tests stay green):

```
[Tcc new]                          -> a fresh, uncompiled TccUnit
[unit compile: cSource]            -> compile into it (repeatable); unit or nil
[unit bindRuntime]                 -> register the standard libid runtime shims
[unit addSymbol: cName value: ad]  -> patch an arbitrary name -> address (SmallInt)
[unit relocate]                    -> finalise; unit or nil; then install:/symbol:
```

`bindRuntime` registers flat `birnam_*` C-ABI shims that forward to the live
IdApi — `birnam_send` / `birnam_intern` / `birnam_mkSmallInt` /
`birnam_smallIntValue` / `birnam_mkString` / `birnam_nil` / `birnam_true` /
`birnam_false`. So **emitted C never spells out the (append-only, evolving) IdApi
struct layout** — it just declares these stable externs and calls them. `Unit`
gained a `relocated` flag guarding the one-way transition. `test-tcc` 6 → 8: the
staged path constructs its result via `birnam_mkSmallInt`, and emitted C
re-dispatches a message back through libid via `birnam_send`/`birnam_intern`.

**Step 2 — codegen emits real sends through libid SHIPPED (`df74cac`).** The
emitter (`codegen.zig`) now switched off the all-in-one onto the staged path
(`new`/`compile:`/`bindRuntime`/`relocate`); the prelude declares the
`birnam_send`/`birnam_intern` externs alongside GETV/MKV. Every non-arithmetic
send is lowered to a real dispatch:

```
unary   [r sel]        -> birnam_send(r, birnam_intern("sel"), 0, 0L)
binary  [r op a]       -> birnam_send(r, birnam_intern("op"), (void*[]){a}, 1L)
keyword [r k: a m: b]  -> birnam_send(r, birnam_intern("k:m:"), (void*[]){a,b}, 2L)
```

Binary ARITHMETIC (`+ - * /`) stays on the inline GETV/MKV fast path — the floor's
`smallint_vt` has no arithmetic method to dispatch to, so it is load-bearing, not
an optimisation. Receivers and args recurse through `emit`, so sends nest freely
(`[[20 plus: 1] double]`). Args go into a C99 compound-literal `void*[]` whose
lifetime spans the call; selectors are interned at runtime via `birnam_intern`
(hoisting/caching is a later optimisation). `test-codegen` 6 → 7 (unary/binary/
keyword dispatch to methods installed on `smallint_vt`, nested + mixed); the
siphon gains a 3c.3 line.

**Step 3 — variables, multi-statement programs, strings SHIPPED (`0596c8a`).**
The emitter compiles a whole program body, not just one expression: `let x := e`
→ a C local `void* b_x = <e>;` that later statements see; identifiers resolve to
`b_<name>` (a `bound_names` lexical environment; unbound idents → out of subset);
`a; b; c` emits each statement and returns the last one's value; string literals
→ `birnam_mkString("…", len)` with a robust C-literal escaper (non-printables as
3-digit octal). `test-codegen` 7 → 9.

**Step 4 — blocks compile to native closures SHIPPED (`e2125d4`).** The last
construct. A block `[:a :b | body]` compiles to a file-scope C function with the
Method ABI: params bind to `args[i]`, captured free vars to slots in the block
object's payload (read via `self`). The literal is a GNU statement-expression
that lazily builds the block's vtable (installing the fn under the arity-matched
`value`/`value:`/`value:value:`… selector via `birnam_proto`/`birnam_objectVt`/
`birnam_method`) and allocates a capturing instance (`birnam_alloc`). **Invoking
a block is just an ordinary `value…` send** — no special case. Free-variable
analysis (`collectRefs`) walks value positions only (skips selectors), descends
into nested blocks for transitive captures, and excludes a block's own params +
top-level lets (shadowing). Block fns accumulate in `fn_defs` in post-order
(inner first) so each is defined before referenced; final source = prelude +
`fn_defs` + `__ev`. `test-codegen` 9 → 10 (params, capture, multi-arg, stored-
then-invoked, nested closures with transitive capture, block-as-argument).

**⭐ THE WHOLE BIRNAM GRAMMAR NOW COMPILES TO NATIVE.** Every production — literals,
all send forms, arithmetic, variables, multi-statement programs, strings, and
blocks/closures — emits valid C, is compiled by libtcc, and installs/runs in the
live id system. The siphon proves it: `[[3+4]*2]`→14, `[[20 plus: 1] double]`→42
(dispatch through libid), and `let x := 20; let add := [:y | [x plus: y]]; [add
value: 22]`→42 (let + capture + closure).

## Post-grammar: the runtime grew up (SHIPPED 2026-06-29)

Everything the original "Next" list named is now done, plus a stdlib and the
relational reading:

- **Referencing + extending the running system** (docs/36) — symbol literals,
  a `Globals` scope, late-bound unbound idents, and `[Reflect defineMethod:#sel
  on:T as:blk]`. **birnam scripts run from the CLI** (`birnam <file>`).
- **Error model (F7)** — `src/id/error.zig`: errors as propagating values; the
  `object_vt` DNU swap so a send to nil yields a catchable value, not a panic.
- **A collection library** (`src/id/collection.zig`) — id-native persistent
  Vector trie + Range + HAMT-backed Map; `select:/collect:/detect:/inject:into:/
  sortBy:`; String ops (`asLowercase`/`replace:with:`/`words`/asSymbol`, `,`).
- **Collection literals** — `#[…]` vectors and `{k -> v}` maps in the grammar +
  codegen (lower to persistent `add:`/`at:put:` chains; `->` keeps symbol keys'
  colons). Logic variables `?p` for queries.
- **The relational reading** (`src/id/relational.zig`) — `db findAll:where:` (a
  datalog join over the store's live triples, with logic vars), `pull:spec:`,
  `entity`. The store answers both imperative and relational readings (D5.2).
- **GC (F3)** — a self-contained conservative mark-sweep with a per-vtable trace
  hook + auto-collection. See docs/34 §F3 (SHIPPED).

Examples that run end to end: `fizzbuzz`, `primes`, `collections`,
`word-frequency`, `address-book`, `gc`, `auto-gc`.

## The full datalog surface (SHIPPED 2026-06-30)

The relational reading grew to the complete datalog surface — predicates,
negation, disjunction, aggregation (count/sum/min/max/avg, group-by), recursive
rules (self + mutual), time travel (asOf/since/between), and nested/`:many` pull —
plus a clause-reorder planner. It is documented in full in its own reference,
[The relational reading](../../design/37-the-relational-reading.md); the old kernel's impl is
[Datalog](../legacy-kernel/29-datalog.md).

## Codegen + runtime, completed (SHIPPED 2026-06-30)

The last two codegen/runtime gaps the 3c notes flagged are closed:

- **Arithmetic is dispatched, not inlined** (`57f2d57`). The 3c.2 MVP emitted
  `+ - * /` as raw C math (`MKV(GETV(a) op GETV(b))`) — a fast path that was
  load-bearing only because the floor's SmallInt then had no arithmetic methods.
  Now that the prelude installs `+ - * /` on `smallint_vt`, every binary op
  compiles to an ordinary `birnam_send`, so arithmetic dispatches to the SmallInt
  protocol and honors its **error flow**: `[10 / 0]` is a `#zeroDivide` value (the
  guarded prelude `/`), and an error receiver **propagates** (`doesNotUnderstand:`
  returns self) through further arithmetic instead of being untagged into garbage.
  This was the F7 codegen gap. (`GETV` is gone; `MKV` stays — number literals still
  tag through it.) See `examples/errors.birnam`.
- **Ahead-of-time native output** (`d1627b7`). `[Tcc compile: src output: path
  as: #kind]` writes the compiled C to disk as a shared library (`#dll`), object
  (`#obj`), or executable (`#exe`) via `tcc_output_file`, alongside the in-memory
  JIT. Verified on macOS arm64: a `.dylib` is produced, `dlopen`'d, and called.
  ~~Codegen C that references the `birnam_*` shims leaves them undefined in the
  artifact, so the JIT remains how compiled birnam runs today.~~ **CLOSED by
  slice 1 of the images arc** ([docs/40](../../design/40-the-image.md) §3a): the runtime now
  exports the shims as real dynamic symbols and codegen has an artifact emission
  mode (`__birnam_unit_init` re-derives global handles by name at load), so
  `[Codegen compile: src output: path]` writes a relocatable `.dylib` that
  `[Codegen load: path]` runs in ANY birnam process — verified cross-process.
  See `examples/aot.birnam`.

## Test/run surface

`zig build test-id` (9) · `test-store` (8) · `test-peg` (4) · `test-lang` (5) ·
`test-tcc` (5) · `test-codegen` (12) · `test-prelude` (3) · `test-collection` (18)
· `test-error` (9) · `test-relational` (24). `zig build install &&
BIRNAM_LIB_DIR=$PWD/zig-out/lib ./zig-out/bin/birnam <file.birnam>` runs a
script through the full stack (store · peg · lang · tcc · codegen · prelude ·
collection · error · relational). Examples: the seven above plus `predicates`,
`ancestors`, `time-travel`, `pull`, `errors`.
