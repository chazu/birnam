# Rung 3 — the front end (peg + lang), and the road to libtcc

> **Status:** In progress (2026-06-27). 3a (peg engine) + 3b (birnam grammar)
> **shipped**; 3c (C codegen + libtcc) is next. Builds on
> [Refound on libid](32-refound-on-libid.md), [Rung 2 — cells](33-rung-2-cells.md),
> and [Foundations before the compiler](34-foundations-before-the-compiler.md).

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

### 3c.2 — the C emitter (next)

Walk the birnam CST → emit Method-ABI C source → feed to `[Tcc compile:]` →
install. Open design choices to settle when starting:

- **CST shape.** The grammar currently makes `expr`/`primary`/`message` rules
  too, so the CST is deeply nested. Either codegen directly off it (reading token
  text via `source[start..end]` — `CstNode` carries `start`/`end`, and the source
  is in hand), or desugar CST → a flatter AST first. Leaning: a small desugar
  pass to a clean AST, then emit.
- **What the emitted C targets.** A small birnam runtime in C (object reps +
  `send`), versus emitting C that calls back into libid through the `IdApi`. The
  old kernel's libtcc backend ([memory] `project_birnam_libtcc`) is the
  libtcc-side reference (vendored static libtcc), but the **emitter is new**.
- **Error model (F7).** Settle as eval/runtime semantics firm up — conditions/
  restarts aspirational, propagating-values the pragmatic floor.

Then, below/after: **gc** (the F3 per-vtable layout descriptor) and the richer
**relational engine** (Date-Third-Manifesto power, store-side, option-independent).

## Test/run surface

`zig build test-id` (4) · `test-store` (11) · `test-peg` (8) · `test-lang` (7) ·
`test-tcc` (6); `zig build install && ./zig-out/bin/birnam-id` runs the siphon
through all rungs (store, peg, lang, and a libtcc JIT).
