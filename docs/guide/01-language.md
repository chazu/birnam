# The language

birnam is a message-passing language in the Smalltalk family, rebuilt on a
prototype floor. There are no classes, no statements-vs-expressions split, and
no operator precedence: **everything is a message send, and every send is
written in brackets**.

## Sends

A bracket pair is a send — a receiver followed by a message.

```
[5 factorial]              // unary:   receiver + bare selector
[3 + 4]                    // binary:  receiver + operator + one argument
[v at: 0 put: "x"]         // keyword: receiver + keyword parts, one arg each
```

Because every send is explicitly bracketed, there is no precedence to
remember — nesting IS the grouping:

```
[[3 + 4] * [10 - 2]]       // (3 + 4) * (10 - 2)
```

Two consequences that surprise newcomers:

- `[x]` — a lone variable in brackets — is an **error**, not grouping. A
  bracket pair must be a send, a literal, or a block. Use the variable bare:
  `[x ifTrue: …]`, never `[[x] ifTrue: …]`.
- `[[3 + 4]]` — redundant wrapping of a complete send — is also an error (the
  outer pair has a receiver but no selector).

Statements may be separated by `;`, but the separator is **optional** —
whitespace alone separates them. Every expression is self-delimiting (a
bracketed group, or a single token), so two statements can never run into each
other. `[a foo]; [b bar]`, `[a foo] [b bar]`, and the same pair on two lines are
one program. Comments run from `//` to end of line.

### Result-threaded sends

`=>` introduces one dispatch whose result becomes the next receiver. Every
leg carries a marker, including the first:

```
[Map
  => new
  => at: "name" put: name
  => at: "kind" put: kind]
```

That is exactly the existing nested-send meaning:

```
[[[Map new] at: "name" put: name] at: "kind" put: kind]
```

A leg is one unary, binary, or keyword dispatch. Write
`[x => foo => bar]` and `[x => + 1 => * 2]`; ordinary chains cannot be mixed
before the first marker. A threaded argument remains bracketed, as in
`[m at: k put: [n => foo]]`.

The punctuation is cascade-shaped but does not have classic cascade semantics:
each leg receives the preceding result, not the original receiver. The exact
binary selector `=>` is reserved, while `#=>` remains a symbol literal and
longer operators such as `=>+` remain available. Source-sensitive forms such as
`System embedFile:`, module declarations, `Alien dlopen:`, and live `Reflect`
operations retain their ordinary spellings and reject threaded spellings.

Run [`examples/pipeline.birnam`](../../examples/pipeline.birnam) for executable
unary, binary, keyword, block-application, conditional, and error-recovery
stages. The wider stdlib vocabulary is exercised by
[`examples/combinators.birnam`](../../examples/combinators.birnam).

## Bindings

`let` introduces a binding; `:=` is only ever used with `let`.

```
let x := [3 + 4];
let name := "birnam";
```

At the top level of a file or the REPL, a `let` **persists into Globals** — it
is a real binding in the image, visible to later evaluations (and it survives
an image save). Inside a block, a `let` is an ordinary local. Bindings are not
reassignable; state that changes lives in the fact store (see the idioms
chapter).

## Literals

```
42  -7                     // Integer — a full i64, boxed
3.14  -0.5                 // Float
"hi\nthere"                // String — \n \t \" \\ escapes
#red  #at:put:             // Symbol — interned names; selectors are symbols
#[1 2 [3 + 4]]             // Vector — ELEMENTS ARE EXPRESSIONS, space-separated
{"a" -> 1, "b" -> 2}       // Map — key -> value, comma-separated
```

Vector and map literals build **persistent** collections and evaluate fresh
each time they are reached — a literal used as a fold seed never accumulates
state across calls. `[1 to: 5]` answers an inclusive Range (`[1 to: 0]` is
empty and iterates zero times).

## Blocks

A block is a closure: parameters after `:`, then `|`, then the body.

```
[:x | [x * 2]]                     // one parameter
[:acc :e | [acc + e]]              // two parameters
[| [System println: "hi"]]         // ZERO parameters — note the bare |
```

Run a block with `value`, `value:`, `value:value:`, … up to **four**
arguments; group more into a vector or map. Blocks capture their enclosing
bindings lexically. A multi-statement body separates statements with `;` and
may open its own `let`s — directly after the `|`, with no extra wrapping
bracket:

```
[:self :c | let v := [self value]; [c value]; v]
```

## Control flow

Control flow is messages to booleans, blocks, and integers — there are no
keywords.

```
[[x > 0] ifTrue: [| "pos"] ifFalse: [| "non-pos"]];   // only ifTrue:/ifFalse:
[[| [n value] < 10] whileTrue: [| [self step]]];      // block receiver loops
[0 to: [[v size] - 1] do: [:i | [v at: i]]];          // inclusive integer loop
```

Bootstrap supplies authored `whileTrue:` and `whileFalse:` through live Id sends.
Their entry methods retain a synchronous suspension barrier; the later stackless
control methods provide resumable loops. See the [Block Loop contract](../design/authored-small-modules.md#block-loops).


`&` and `|` on booleans are **eager** — both sides evaluate, always. For
short-circuiting, use a deferred block with `and:` / `or:`:

```
[[x notNil] and: [| [x ready]]]
```

Guard a maybe-nil dereference this way or with nested `ifTrue:ifFalse:`, never
with eager `&`.

## Strings and printing

`,` concatenates (coercing via `printString`), `printString` renders any
value, and `printNl` prints one and answers it.

`System print:` writes without a newline; `System println:` adds one. Both flush
stdout and answer the receiver. Strings are written directly; other values receive
`printString`, which must return a String. An unusable rendering answers nil.
Object `asString` sends the current `printString`, so replacements take effect.

```
[System println: ["answer: " , [42 printString]]];
[x |> [:v | [v printString]]]      // |> threads the receiver into a block
```

## Errors are values

A failing operation answers a **kinded error value** — it does not raise.
`[10 / 0]` answers `error(#zeroDivide: …)`; sending to it propagates it.
Check with `isError` / `kind`; recover with `ifError:`, transform only a success
with `ifNotError:`, and handle a selected kind around a deferred computation
with `on:do:`. One consequence at the top level of a file: a discarded error
**aborts the load** at that form (see the idioms chapter for the full protocol).

## Running code

```
birnam file.birnam         // evaluate a source file
birnam                     // the REPL (:help lists commands)
birnam doc <module>        // read a module's docs
birnam docgen              // build the HTML reference you are reading
```
