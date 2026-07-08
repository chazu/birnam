# Post-MVP — the numeric tower, GC finalizers, and baked global handles

Three arcs taken right after the MVP (`docs/38`), each closing a gap that note
named as "post-MVP": a **Float** floor type (and FFI doubles), **GC of generated
native code** (via a finalizer primitive the collector lacked), and **baked global
handles** (the safe layer of the `docs/36` v2 optimization). All on the id stack;
all 12 id suites green; the examples unaffected.

## 1. Float — the numeric tower (`src/id/id.zig`, `prelude.zig`, `lang.zig`)

The floor had exactly one number: the tagged `SmallInt` (F1). `Float` is the
second. SmallInt owns the single immediate tag (maru-style low bit), so a float
cannot also be an immediate — it is a **boxed `f64`**: a heap object whose vtable
pointer at `[-1]` is `float_vt` and whose payload is one `f64`. The IdApi gains
`float_vt` / `mkFloat` / `floatValue` / `isFloat` (appended — ABI stays stable).

**The grammar** gets a `float` rule, `'-'? digit+ '.' digit+`, tried *before*
`number` in `primary` so `3.5` is one float literal, not `3` then a stray `.5`.
Codegen emits the span text (already a valid C double literal) into a new floor
runtime shim, `birnam_mkFloat(…)` (registered in `tcc.zig`'s `runtime_syms`
alongside `birnam_mkSmallInt`).

**The protocol** lives in the prelude — `+ - * / = < <= > >= negated asInteger
printString` on `float_vt`, plus **int↔float promotion in both directions**. A
binary numeric op classifies its argument (`numKind`: int / float / error / bad):

- two Integers stay exact (i128, range-checked — overflow is still a `#overflow`
  value, F7);
- **any Float operand promotes the whole operation to `f64`** — `1 + 2.5` and
  `2.5 + 1` both give `3.5`;
- an Error propagates; a non-number is a `#typeMismatch`; float `/ 0.0` is a
  `#zeroDivide` (matching integer division).

`Float printString` forces a decimal point (`4.0`, not `4`) so a whole-valued
float still round-trips to the `float` grammar rule rather than `number`.
`asInteger` truncates toward zero (range-checked → `#overflow` for a non-finite or
out-of-range float); Integer `asFloat` widens.

**FFI doubles** (`src/id/alien.zig`). The signature alphabet gains `d` (double)
and `f` (float); the generated shim marshals them through `birnam_floatValue` /
`birnam_mkFloat` (also added to `runtime_syms`). So `"d s"` wraps `atof`
(`["3.25" asCDouble]` → `3.25`) and `"d d d"` wraps `pow` on `float_vt`
(`[2.0 raisedTo: 10.0]` → `1024.0`). **Struct-by-value remains out of scope** —
that needs struct-layout description plus field marshaling, a separate arc;
scalars were the blocker the FFI note named, and they are done.

## 2. GC finalizers — reclaiming native code and string bytes (`id.zig`, `tcc.zig`)

The conservative mark-sweep (F3) freed only the **libid wrapper block** of a dead
object. Anything that object owned *outside* the id heap — a `String`'s `ca.dupe`'d
byte buffer, a `TccUnit`'s native code (executable pages) — leaked on every sweep.
The collector simply had no way to run cleanup.

So the collector gains a **finalizer**: a `Block.finalize` hook, set via the new
`IdApi.setFinalizer(obj, fn)`, run once just before the block is reclaimed (it
must not allocate id objects — it runs mid-sweep). Two uses:

- **String** registers a finalizer in `mkString` that frees its bytes. This fixes
  a genuine unbounded leak under string churn (the GC now reclaims a String *and*
  its buffer).
- **TccUnit** gets a `[unit disposable]` message that attaches a `tcc_delete`
  finalizer. The native code's lifetime is the question: a block literal installs
  a `value` method pointing into *this* unit's code (`emitBlock` →
  `birnam_method`), so a returned or stored closure keeps that pointer live. The
  conservative GC can't see that closure→code link (vtable method slots are raw
  pointers, not Oops). So codegen marks an eval's unit `disposable` **only when it
  emitted no blocks** (`blk_counter == 0`) — then the result is pure data built
  through the runtime shims, the code is provably dead, and the collector
  `tcc_delete`s it. With blocks, the unit is kept. Verified: a REPL closure
  survives a 50 000-allocation GC churn and still answers correctly afterward.

**Remaining minor leak:** the per-eval throwaway `proto` vtable is `.permanent`
(small; making vtables collectable is a larger GC-design change — they are assumed
permanent throughout the trace/root protocol).

## 3. Baked global handles — the safe layer of docs/36 v2 (`codegen.zig`)

A global reference today emits `birnam_global("name")`, which at runtime interns
the name (an `O(symbols)` scan) and then reads the Globals store. The store stays
the single source of truth for global **values** — all three writers (REPL-`let`
persistence, `[Globals at:put:]`, define-method's block stash) funnel through
`store assert`. But the interned **selector** is immutable, so we resolve it
**once at compile time** and bake its address as `extern char __g<i>[];`, emitting
`birnam_global_b((void*)__g<i>)` — the read is now a store lookup keyed by the
baked symbol, with no runtime intern. `bakeGlobal` / `putGlobalRef` +
`global_memo` (dedup by symbol identity) in `codegen.zig`; idents *and* the
`Vector` / `Map` / `LogicVar` factory refs all route through it; each `__g<i>` is
patched via the staged Tcc `addSymbol:value:` window before relocate.

This is **late-bound and relocatable** (the value is read fresh from the store
each time; the linker patches the address) and has **zero desync risk** — only the
immutable symbol is baked, never the mutable value. It is the safe layer of the
`docs/36` §3 v2 design.

**Deferred (the rest of v2):** the value-slot O(1) read. To make that airtight the
value box must be owned by the *store* (so the cross-module `[Globals at:put:]`
writer keeps it current through the store's own `assert` path), which touches the
foundational store's hot write path for a marginal win (globals are cold). And
**scope chains** (`docs/36` §6) remain a separate arc.

## Test/run surface

Same as `docs/38`. New tests: Float floor value + finalizer-runs-once +
reclaimed-String-frees-bytes (`test-id`); disposable-unit-reclaimed (`test-tcc`);
the float tower with promotion (`test-codegen`); double FFI `atof` / `pow`
(`test-alien`). Run a float program: `BIRNAM_LIB_DIR=$PWD/zig-out/lib
./zig-out/bin/birnam <file.birnam>`.

## What remains (next session)

- **Struct-by-value FFI** — layout description + field marshaling on the Float/FFI
  base now laid.
- **Value-slot O(1) globals + scope chains** — the rest of `docs/36` v2.
- **Persistence / images** — the `docs/30` vision.
- **De-leak the throwaway eval vtables** — a bigger GC-design change (collectable
  vtables).
