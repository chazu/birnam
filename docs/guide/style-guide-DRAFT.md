# Birnam style guide (DRAFT)

> Draft for discussion. `02-idioms.md` says what idiomatic birnam *means*
> (prototypes, cells, errors-as-values). This says what it should *look like* —
> the layout, decomposition, and anti-pattern rules that keep a bracket-and-block
> language from turning into a wall of punctuation. Where the two overlap, this
> doc defers to `02-idioms.md`.
>
> `=>`, `tap:`, `when:apply:`, and `unless:apply:` are shipped language and
> default-stdlib vocabulary from Design 64.

The grammar gives us three unavoidable sources of visual density: every send is
`[...]`, every block is `[...]`, and zero-arg blocks carry a `[| ... ]`. You
cannot delete that density. Every rule below exists to stop it from
**compounding** — because density is linear and readable, but nesting is
exponential and hideous.

The single most important idea: **when code gets ugly in birnam, it is almost
always drifting rightward, not growing downward. Fix the drift and 80% of the
ugliness is gone.**

---

## Part 1 — The four cardinal rules

These are the load-bearing rules. Everything in Part 2 is a special case of one
of them.

### Rule 1 — Data beats nested conditionals

There is no `cond`, `case`, or `elsif`. That is fine. A chain of
`ifTrue:ifFalse:` that maps *a value to a value* is not a control structure —
it is a **lookup table that forgot it was data**. Write the table.

**Before** (from the retired terminal-renderer experiment — a 7-way branch
nested 7 levels deep):

```
[[kind = "comment"] ifTrue: [| 5] ifFalse: [|
  [[kind = "string"] ifTrue: [| 3] ifFalse: [|
    [[kind = "number"] ifTrue: [| 6] ifFalse: [|
      [[kind = "symbol"] ifTrue: [| 6] ifFalse: [|
        [[kind = "keyword"] ifTrue: [| 7] ifFalse: [|
          [[kind = "selector"] ifTrue: [| 4] ifFalse: [| 8]]]]]]]]]]]
```

**After** — one send, no drift, and the mapping is now *visible as a mapping*:

```
[{ "comment" -> 5, "string" -> 3, "number" -> 6,
   "symbol" -> 6, "keyword" -> 7, "selector" -> 4 }
   at: kind ifAbsent: [| 8]]
```

The `ifAbsent:` block carries the default. This is faster too: one map lookup
versus up to six string compares.

The same surgery applies to `highlightKindForWord:colon:` in `analysis.birnam`
and to any `applyAt:` / dispatch method whose arms are all `status set: "..."`.
When the arms are *actions* rather than values, the table holds blocks:

```
let handlers := { "r" -> [| [self reveal]], "f" -> [| [self toggleFlag]] };
[[handlers at: cmd ifAbsent: [| [| [self unknown: cmd]]]] value]
```

**The rule:** if every branch tests the same variable against a constant and
produces a value or a nullary action, it is a table. Two levels is a judgment
call; three or more is a table, full stop.

### Rule 2 — Name the wall before it stacks

The `[[[[...]&...]&...]&...]` pileup happens when a compound expression is built
inline instead of bound. The reader hits four `[` before the first real token
and has to count brackets to find the verb.

**Before** (`examples/minesweeper/minesweeper.birnam`, repeated ~six times):

```
let ok := [[[[nx >= 0] & [nx < w]] & [ny >= 0]] & [ny < h]];
```

**After** — bind the halves; the intent (`inBounds`) becomes readable, and the
name documents itself:

```
let inX := [[nx >= 0] & [nx < w]];
let inY := [[ny >= 0] & [ny < h]];
let inBounds := [inX & inY];
```

Better still, when the same predicate recurs across a file, it is a **method**,
not a `let` repeated six times. A `bounds`-checking helper on the board object
pays for itself the second time you write it.

**The rule:** if a bracketed expression opens **3 or more brackets** before its
first token, bind the inner pieces with `let` (or extract a method). A good line
should let you find its verb without counting `[`.

### Rule 3 — One decision per method

Blocks cannot early-return, so you cannot flatten a cascade with guard clauses
the way Smalltalk does. The birnam substitute is **extraction**: a method that
makes one decision and hands the rest to another method reads top-to-bottom even
though each piece is still an `ifTrue:ifFalse:`.

`reveal` in minesweeper is the pattern done right — flagged? / revealed? / mine?
each gate delegates instead of nesting a fourth level of logic inline. When you
feel a third `ifFalse: [|` coming, that is the signal to name the else-branch as
its own method and call it.

**The rule:** a method should contain at most one *primary* branch. Its arms may
call other methods freely; they should not grow a second unrelated branch
inside themselves.

### Rule 4 — Use the floor the language already ships

Do not hand-roll what the runtime provides. The clearest tell in the current
tree:

```
// minesweeper.birnam, line ~48:
// "birnam has no built-in whileTrue:, so we spell the loop as a
//  self-recursive block..."
let whileDo := [:cond :body | [[cond value] ifTrue: [| ... [whileDo value: ...]]]];
```

This comment is **stale** — native `whileTrue:` / `whileFalse:` shipped and
`analysis.birnam` already uses them. Reinventing them costs a stack frame per
iteration and teaches readers a retired pattern.

**The rule:** loop with native `whileTrue:`/`whileFalse:`; fold with
`inject:into:` and its derivations; sort with `sortBy:`. If you are writing a
recursive block to simulate a loop, stop and check the floor first. (See
`02-idioms.md` § "Know where the floor is" for the perf line: surface logic in
birnam, per-byte/per-row scanners stay native.)

---

## Part 2 — Layout conventions

Density is fine; *inconsistent* density is what reads as sloppy. Pick one form
for each of these and hold it.

### Blocks and their arms

Zero-arg block: the `|` hugs the opening bracket, one space after.

```
[| [doThing]]
```

Short conditional — inline, arms on the line:

```
[[n = 0] ifTrue: [| " "] ifFalse: [| [n printString]]]
```

Long conditional — break **before** each keyword, arm body indented under it:

```
[[cell at: #revealed]
  ifTrue:  [| [self drawRevealed: cell]]
  ifFalse: [| [self drawHidden: cell]]]
```

Align the `ifTrue:` / `ifFalse:` colons (one trailing space on `ifTrue:`) so the
two arms line up. Never leave a lone `ifFalse: [|` dangling at the end of a long
line — that trailing `[|` is the visual signature of Rule 1 being violated.

### Wrapping a keyword message

When a single keyword send is too long, break so each continued keyword lands
under the first argument column, not under the receiver:

```
[self renderSourceRange: lineStart start: [cursor value] end: start
  y: y promptWidth: promptWidth fg: 8 text: text]
```

Two-space continuation indent. One send per physical line group; do not pack two
different sends onto one wrapped line.

### Accumulation: four forms, ranked

The `let a := ... let b := [a , ...] let c := [b , ...]` ladder in
`render.birnam:summary:` is the least readable way to build a value. Prefer, in
order:

1. **A single `,` chain** when it fits — `["mode=" , mode , " port=" , port]`.
2. **A `|>` pipeline** when the stages are transforms and you want them to read
   top-down instead of inside-out. This is shipped today; `|>` applies a block
   and remains an ordinary overridable method (see `examples/pipeline.birnam`):

   ```
   [nums |> [:v | [v select: [:n | [n even]]]]
         |> [:v | [v collect: double]]
         |> [:v | [v inject: 0 into: [:a :n | [a + n]]]]]
   ```

3. **A `=>` receiver thread** when each stage is naturally a message to the
   preceding result.

   ```
   [Map
     => new
     => at: "name" put: name
     => at: "kind" put: kind]
   ```

   Put `=>` before every dispatch, including the first. Do not mix an ordinary
   first message with later markers. Use `=> |> aBlock` when a block transform
   is genuinely the next stage; do not spell the same operation `then:`.

4. **A `let`-ladder** only when each intermediate genuinely needs a name for
   clarity — not merely because the expression is long.

### Conditional and error stages in a thread

Use `when:apply:` / `unless:apply:` for conditional value replacement, not
`when:do:`. `do:` means iteration or action; `apply:` says that the taken
block's answer becomes the next receiver.

```
[value
  => when: valid apply: normalize
  => unless: complete apply: fillDefaults]
```

Error stages already exist. Use `ifError:` to recover and `ifNotError:` to apply
a block only to success; do not introduce `recover:` or `catch:` aliases:

```
[input
  => parse
  => ifError: [:error | fallback]
  => validate]
```

Use the existing deferred `on:do:` form when matching a particular error kind,
and `ensure:` for always-run cleanup. General `Object` combinators such as
`tap:` and `when:apply:` must preserve an incoming error without running user
blocks.

### Never wrap a bare literal in brackets

`[...]` must contain a **send** or a **block** — never a lone literal. This is a
hard parse rule, not a style preference:

```
[System println: ["hello"]]           // SYNTAX ERROR — bare literal in brackets
[System println: [5]]                  // SYNTAX ERROR
[System println: [#sym]]               // SYNTAX ERROR
[System println: "hello"]              // OK — literal stands alone as the argument
[System println: ["hello" printString]]  // OK — the brackets hold a send
[System println: [| "hello"]]          // OK — the brackets hold a block
```

The tell is a `[` immediately followed by a quote/digit/`#` and then a matching
`]` with nothing between them but the literal. Drop the brackets, or make it a
send/block.

### Statement terminators and top-level brackets

The separator is `;` (not `.` — that is the authoring gotcha that bites everyone
once), and it is **optional**: whitespace already separates statements, because
every expression is self-delimiting. `;` is punctuation, not syntax.

The existing corpus is written with `;`, and that stays the house style for
files that already use it — do not mix spellings within a file. The final form
in a unit takes no trailing `;`. A top-level statement that is a single send
still gets its outer `[...]`; keep it, consistency beats saving one bracket
pair.

### Cells: read and write the same way every time

Read is `[c value]`, write is `[c set: x]`, increment is
`[c set: [[c value] + 1]]`. Do not invent per-file variants. Name a cell for
what it *holds*, not that it is a cell (`running`, `board`, `rngState` — not
`runningCell`); the `[... value]` at the use site already says "cell." Seed
every cell before first read (`02-idioms.md` § "State lives in cells").

---

## Part 3 — Comments and docstrings, by altitude

The examples and the modules are held to **different** comment standards, and
that is correct — do not flatten them to one rule.

- **Examples (`examples/`)** are teaching prose. A 20-line header explaining the
  domain model and a comment above every stanza is *right* here
  (`word-frequency.birnam` and `minesweeper.birnam` are the model). The reader
  is learning both the problem and the language.

- **Modules (`modules/`)** are terse. One `doc:` string per method carrying the
  *contract* (what it answers, given what), and inline comments reserved for
  *why-not-what* — the non-obvious constraint, the eager-`&` gotcha, the perf
  reason a loop stays native. A module comment that restates the code is noise.

Every method defined with `defineMethod:on:as:` carries a `doc:` — no
exceptions; the docstring is a fact in the image that the LSP and `birnam doc`
read back (`02-idioms.md` § "Documentation and tests ride the system").

Register the module (`docModule:is:`) **before** the first `defineMethod:` in
the file — a late registration silently misfiles every earlier method under the
previous module.

---

## Part 4 — A worked before/after

`highlightColorFor:` reduced by Rule 1, laid out by Part 2:

**Before** — 12 lines, 7 levels of drift, up to 6 string compares:

```
[Reflect defineMethod: #highlightColorFor: on: HighlightTheme as: [:self :kind |
  [[kind = "comment"] ifTrue: [| 5] ifFalse: [|
    [[kind = "string"] ifTrue: [| 3] ifFalse: [|
      [[kind = "number"] ifTrue: [| 6] ifFalse: [|
        [[kind = "symbol"] ifTrue: [| 6] ifFalse: [|
          [[kind = "keyword"] ifTrue: [| 7] ifFalse: [|
            [[kind = "selector"] ifTrue: [| 4] ifFalse: [| 8]]]]]]]]]]]
  ] doc: "Map a lexical highlight kind to a Termbox foreground color."];
```

**After** — 4 lines, zero drift, one lookup:

```
[Reflect defineMethod: #highlightColorFor: on: HighlightTheme as: [:self :kind |
  [{ "comment" -> 5, "string" -> 3, "number" -> 6,
     "symbol" -> 6, "keyword" -> 7, "selector" -> 4 } at: kind ifAbsent: [| 8]]
  ] doc: "Map a lexical highlight kind to a Termbox foreground color."];
```

The `doc:` contract is unchanged — that is the point. Style refactors preserve
behavior and the docstring; only the shape improves.

---

## Part 5 — Performance: the pretty form is at worst free, usually faster

The worry with Rule 1 was that a map lookup allocates while a conditional does
not — that we might be trading speed for looks. **We measured it. The pretty
form is never slower, and the right pretty form is 2× faster.**

`docs/guide/style-proof.birnam` benchmarks four implementations of
`highlightColorFor:` **as methods** (it is a `defineMethod:` in the real code, so
method dispatch — a large fixed cost — must be in every measurement, not hidden
by testing bare blocks). Realistic kind mix, 360k calls each, dev build — treat
absolutes as relative, the *ratios* are the point:

| Variant | ns/call | vs. nested |
|---|---:|---:|
| A — nested `ifTrue:ifFalse:` (original) | ~57800 | 1.0× |
| B — inline map, rebuilt every call | ~54300 | 0.94× (≈ same) |
| C — **`let`-hoisted**, method closes over a module-level table | ~29000 | **0.50× (2× faster)** |
| D — Globals-hoisted, `[Globals at: #…]` each call | ~40200 | 0.70× |

Four things fall out, and they revise the naive intuition:

1. **Method dispatch is the dominant fixed cost** (~22µs here, measured
   separately). Style choices move the *logic* cost that rides on top of it — so
   the honest comparison is always method-to-method, never block-to-block. (An
   earlier draft benchmarked bare blocks and reported misleading 5× numbers; the
   method numbers above supersede them.)

2. **The naive pretty form (B) is not slower than the cascade.** Building a
   six-entry map and doing one lookup roughly equals up to six string compares.
   Rule 1 costs nothing even built inline — so on a cold path, prefer B purely
   for readability.

3. **`let`-hoisting (C) is both the fastest and the cleanest.** Bind the table
   once with a module-level `let`; the method *closes over it*. No per-call
   rebuild (unlike B), no per-call Globals lookup (unlike D). 2× faster than the
   cascade, and it reads as a plain `[colorTable at: kind ifAbsent: [| 8]]`.

4. **`[Globals at: #…]` per call is a real tax** — D pays ~11µs more than C for
   that indirection. Reach for Globals-hoisting only when a `let` capture is not
   available (see the caveat below).

There is also a subtler reason the cascade (A) is bad on real input: it is
**worst-case-biased**. A miss — an `identifier`, `literal`, or `punctuation`
kind, which *dominate* real source — falls all the way through to the default,
paying the full six comparisons. The cascade is slowest on exactly the inputs it
sees most; the table's cost is flat across hit and miss.

**Rule 1, performance form:** map over cascade always. For a **hot path**, hoist
the table to a module-level `let` and let the method close over it (variant C).
Reserve inline (B) for cold paths, and Globals (D) for when you cannot capture a
`let`.

```
// once, at module load — the method below closes over this binding:
let colorTable := { "comment" -> 5, "string" -> 3, "number" -> 6,
                    "symbol" -> 6, "keyword" -> 7, "selector" -> 4 };

// per call — one lookup, no rebuild, no Globals indirection:
[Reflect defineMethod: #highlightColorFor: on: HighlightTheme as: [:self :kind |
  [colorTable at: kind ifAbsent: [| 8]]
  ] doc: "Map a lexical highlight kind to a Termbox foreground color."];
```

> **Caveat — verify before trusting C in a compiled module.** The benchmark ran
> under eval, where the file-level `let` is plainly live and captured. Whether a
> method compiled into a **dylib** still closes over a file-level `let` at
> runtime (after the file scope has exited and the unit was reloaded from disk)
> is **not verified here**. Blocks are native closures and module singletons are
> known to survive boot, so it very likely holds — but confirm it for a real
> compiled module before relying on C there. If it does not hold, the
> Globals-hoisted form (D) is the dispatch-safe hot-path fallback, still 1.4×
> faster than the cascade.

This dovetails with `02-idioms.md` § "Know where the floor is":
`highlightColorFor:` is called once per highlight span — surface logic, birnam is
the right home — so shaving its send cost is exactly the kind of win that lives
in birnam. The per-byte scanner that *produces* those spans
(`highlightRangesIn:`) is a different animal and correctly stays a tight native
loop; do not try to prettify that with a map.

## Cheat sheet

| Smell | Rule | Fix |
|---|---|---|
| `ifFalse: [|` cascading rightward | 1 | Map lookup with `ifAbsent:` default (2× faster if `let`-hoisted) |
| Lookup table rebuilt inline in a hot method | 5 | Hoist to a module-level `let` the method closes over |
| Bare literal in brackets `["x"]` / `[5]` | P2 | Drop brackets, or make it a send/block |
| 3+ `[` before the first token | 2 | Bind inner pieces with `let`, or extract a method |
| Third `ifFalse:` nesting in one method | 3 | Extract the else-branch as its own method |
| Recursive block that acts like a loop | 4 | `whileTrue:` / `whileFalse:` |
| `let a … let b := [a,…] … let c := [b,…]` | P2 | Single `,` chain; `|>` block pipeline; `=>` receiver thread |
| Dangling `ifFalse: [|` at line end | P2 | Break before keywords, align the colons |
| Comment restating the code (in a module) | P3 | Delete it; keep only why-not-what |
| Method defined with no `doc:` | P3 | Add the contract docstring |

---

## Open questions for review

1. **One guide or two?** Modules and examples want genuinely different comment
   density (Part 3). Split into two short docs, or keep the altitude distinction
   inside one, as here?
2. **How much teeth?** Some of these (max nesting depth, bind-at-3-brackets,
   keyword-break layout) are mechanically checkable — worth aiming a future
   formatter/linter at, or keep the guide aspirational?
3. ~~**Is the map-over-conditional rule always a win?**~~ **RESOLVED** by the
   method benchmark (Part 5): inline map ≈ cascade (free on cold paths), and a
   `let`-hoisted table is 2× faster. Rule 1 wins on both axes. Two follow-ups:
   (a) confirm the `let`-capture survives compile-to-dylib so C can be the
   blanket hot-path recommendation rather than "verify first"; (b) should a
   formatter *flag* inline tables inside loop bodies as hoist-candidates?
4. ~~**`|>` adoption.**~~ **RESOLVED by Design 64:** keep `|>` as the available
   block-application method; do not force point-free block pipelines. Prefer
   `=>` for receiver-oriented message stages and use `=> |> aBlock` only where
   the next stage is naturally a block transform.
