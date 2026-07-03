# Toward MVP — the interactive runtime, I/O, the FFI, and self-hosting

> **Status:** Shipped (2026-06-30). The work that turns the id-stack birnam from a
> batch compiler into a usable, non-crashing, interactive environment. Builds on the
> [front end](35-rung-3-front-end.md), the [running system](36-the-running-system.md),
> and the [relational reading](37-the-relational-reading.md).

The language already compiled the whole grammar to native, with a stdlib, a
complete datalog surface, a GC, and an error model. What was missing for an MVP was
not capability but **usability and robustness**: a way to explore the system live,
to get data in and out, to not crash on an ordinary mistake, to reach C libraries,
and to grow the library in birnam itself. This page covers those five pieces.

## 1. Robust arithmetic — faults are values, not crashes

Integer arithmetic used to untag its argument blindly and use raw machine ops, so a
non-Integer operand read garbage and an overflowing `*` panicked the process. Now
`+ - * / mod:` and the orderings `< <= > >=`:

- **propagate an error operand** — `5 + (10 / 0)` is the original `#zeroDivide`, not
  a masked type error;
- **reject a non-Integer** with a `#typeMismatch` error;
- **compute in 128-bit and range-check** the 63-bit SmallInt bounds, minting
  `#overflow` instead of wrapping or panicking;
- division/mod keep the `#zeroDivide` guard; equality stays total (a non-Integer is
  simply not equal — no error).

Every fault is an ordinary catchable [error value](36-the-running-system.md) (F7):

```
[[1000000000000000000 * 1000000000000000000] on: #overflow do: [:e | 0]]   ⇒ 0
[[5 + System] on: #typeMismatch do: [:e | [e message]]]   ⇒ "Integer arithmetic needs an Integer"
```

With the error module absent, faults degrade to nil rather than crashing.

## 2. I/O — beyond stdout

The `System` protocol gained file and line I/O, so programs can move real data:

```
[System writeFile: "/tmp/x" contents: "hello"]     → self, or an #ioError value
[System readFile: "/tmp/x"]                         → the contents (String), or #ioError
[System readLine]                                   → the next stdin line, or nil at EOF
```

`readLine` is byte-at-a-time, so it composes with an interactive read loop. Failures
are catchable Error values — no NULL, no panic.

## 3. The REPL — an interactive image

`birnam --repl` (or `-i`, or `repl`) starts a read-eval-print loop over the full
stack — the leap from running files to exploring the running system live:

```
birnam — an interactive image. :help for commands, :quit (or Ctrl-D) to leave.
birnam> let x := 40
40
birnam> [x + 2]
42
birnam> [Reflect defineMethod: #double on: Integer as: [:self | [self * 2]]]
anObject
birnam> [21 double]
42
```

- **Persistent bindings** — a top-level `let` survives across lines, and
  `defineMethod:` extends the live system. This is real, not re-run: codegen's
  `evalPersisting:` lowers a top-level `let` to *also* write the Globals namespace
  (via the `birnam_global_set` shim, the write side of `birnam_global`), so a
  binding is stored once and read back late-bound — no re-execution of prior side
  effects. Block-local lets stay local.
- **Multi-line input** — a form continues (with a `...` prompt) until its brackets
  balance, ignoring brackets inside `"…"` strings and `//` comments.
- **Clean errors** — input is parsed first, so a syntax error reports as such; and
  because faults are catchable values, the loop never dies on a bad line.
- **Exploration** — `:help`, `:globals` (your bindings and their values), `:quit`.

Known limit: each evaluation keeps its compiled native unit (there is no GC of
generated code yet), so a very long session grows slowly.

## 4. The FFI — absorb C libraries (`src/id/alien.zig`)

birnam's compiler *is* a C compiler (libtcc), so the FFI needs no separate
marshaling engine: it **generates a C shim** that unpacks birnam arguments, calls
the real C function, and packs the result back, then compiles + links it with `Tcc`
and installs it as a method.

```
[Alien wrap: "strlen" signature: "l s" as: #clen on: String]
["hello, world" clen]            ⇒ 12

[Alien wrap: "abs" signature: "i i" as: #absVia on: Integer]
[-42 absVia]                     ⇒ 42

[Alien dlopen: "/path/lib.dylib"]   load a library so its symbols resolve, then wrap
```

A **signature** is `<ret><args…>` (whitespace ignored) over `i` int · `l` long ·
`s` cstring · `p` pointer · `v` void (return only). The **receiver is the first C
argument** — the Smalltalk-natural idiom — so a K-argument function becomes a method
of arity K-1: wrap `strlen` on `String` and `["hi" clen]` calls `strlen("hi")`.

The wrapped function's address is resolved by the process dynamic linker
(`dlsym` with `RTLD_DEFAULT`) and patched into the unit before relocation — the same
mechanism that binds the `birnam_*` runtime shims — so resolution is reliable;
`dlopen:` brings an external library into the process first. Doubles and structs
await a Float/struct type; a pointer (`p`) boxes opaque handles meanwhile.

## 5. Self-hosting — the stdlib grows in birnam

The floor (Integer/Boolean/String/System protocol, the persistent collections) is
the irreducible Zig primitive layer. *On top of it*, more of the standard library is
now written **in birnam** — `src/id/stdlib.birnam`, embedded and loaded by default,
defining its methods via `[Reflect defineMethod: …]`:

```
[Reflect defineMethod: #abs on: Integer
   as: [:self | [[self < 0] ifTrue: [| [0 - self]] ifFalse: [| self]]]];
[Reflect defineMethod: #factorial on: Integer
   as: [:self | [[self <= 1] ifTrue: [| 1] ifFalse: [| [self * [[self - 1] factorial]]]]]]
```

It provides Integer `negated/squared/abs/even/odd/max:/min:/between:and:/factorial`
(recursive) and Object `printNl`. **The Zig floor is preserved as an option**:
`BIRNAM_NO_STDLIB=1` runs on the bare floor and these sends become
`#doesNotUnderstand` — the self-hosted layer is additive, never replacing the
primitives.

Writing the stdlib surfaced a real grammar gap: `binaryMsg` matched a *single*
operator char, so `a >= b` and `a <= b` did not parse. The grammar now has a
`binops` rule (one-or-more operator chars as one node), so multi-character operators
parse as a single selector and dispatch normally.

## Where this leaves the MVP

A self-contained, interactive birnam you can program in: write a real program (with
file I/O), run it or explore it live in the REPL, reach C libraries, and grow the
library in birnam — and it does not fall over on ordinary mistakes. What remains is
post-MVP: persistence/images (the docs/30 vision), a Float/struct FFI, and GC of
generated native code.

## Test/run surface

`zig build` then any of `test-id · test-store · test-peg · test-lang · test-tcc ·
test-codegen · test-prelude · test-collection · test-error · test-relational ·
test-alien · test-main`. `BIRNAM_LIB_DIR=$PWD/zig-out/lib ./zig-out/bin/birnam
<file.birnam>` runs a script; `… birnam --repl` starts the REPL. Examples:
`errors`, `ffi`, `stdlib`, plus the earlier set.
