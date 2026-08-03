# 64 — Pipeline threading and the combinator vocabulary

> **Status:** Proposed, 2026-08-03. Nothing here is implemented. This document
> sketches a message-bearing pipeline operator (`=>`) and the combinator set it
> should be designed against. It is deliberately a syntax-and-protocol sketch,
> not an implementation plan.

Birnam brackets delimit sends. They also, today, delimit precedence: an argument
or receiver slot accepts only a primary or an already-bracketed send, so a
sequence of sends over one threaded value has no notation. The plumbing that
carries the value from send to send has to be written out.

It surfaces two ways, and taste picks which:

```
let d0 := [Map new];                                    // numbered temporaries
let d1 := [d0 at: "name" put: name];
let d2 := [d1 at: "kind" put: kind];

[[[Map new] at: "uri" put: uri] at: "diagnostics" put: all]   // left-nesting
```

Neither is a value being computed; both are plumbing made visible. `d1` is
bound, used once as the next receiver, and never inspected. `modules/lsp.birnam`
carries 140 numbered-temporary bindings and `src/id/project-policy.birnam` 48,
the dominant shape being exactly this.

The property that makes the collapse legitimate is the applicative one: the
shape of the sequence is fixed in advance. No segment's structure depends on a
previous segment's value. Only the receiver flows.

## What does not change

This proposal is additive and narrow. Specifically unchanged:

- **`#|>` remains an ordinary method.** `src/id/stdlib.birnam:344` defines it on
  `Object` as `[block value: self]`, and `modules/stdlib-test.birnam:215`
  asserts a prototype may override it. Both stand.
- **Brackets still delimit every send.** No statement-level bracket elision, at
  top level or in block bodies.
- **No precedence tiers.** `message` remains the flat choice of keyword, binary,
  and unary forms; an argument that is not a primary still requires brackets.
- **No implicit lifting.** Absence continues to ride `ifAbsent:` under the
  no-NULL lock. Nothing in this document introduces a nil-propagating send.

## The `=>` operator

`=>` separates messages within one bracket. The result of each segment becomes
the receiver of the next.

```
[Map new
  => at: "name" put: name
  => tap: [:x | [System println: x]]
  => at: "range" put: range]
```

`=>` is grammar rather than a method: its right operand is a message, not a
value, so it cannot be received. This is the one respect in which it differs
from `|>`, and the reason both exist rather than one.

### Token

`=>` appears nowhere in birnam source except inside strings and comments, where
it already reads as "yields" (`[21 double]  "=> 42"`). `binop` is the charset
`= < > & | , + - * / ~ % @ ! ^` and `binops` is `binop+`
(`rust/birnam-lang/src/lib.rs:241`), so `=>` already lexes as a single token. No
lexer change is required.

`|` is not available for this purpose: it is in the binop charset and means
eager boolean or, so `[x | y]` is already a send.

### Rule

```
threaded := message ('=>' ws message)+
send     := '[' expr (threaded | message) ']'
```

The `threaded` alternative must be ordered before `binaryMsg` in `message`.
`binops` is greedy over the charset and will otherwise consume `=>` as a binary
selector — the same greediness by which `[1 +- 2]` lexes `+-` as one selector
today. This is the single most likely way to implement the rule and get silent
wrong behaviour instead of a parse error.

Keyword segments terminate without ambiguity. After `put: name`, `keywordMsg`
looks for `ident ':'`, finds `=>`, and stops.

### Semantics

A segment is a whole message: keyword, binary chain, or unary chain, exactly as
inside an ordinary bracket. `[x => foo => bar: 1]` is `[[x foo] bar: 1]` and
must compile to the identical send sequence — `=>` introduces no new dispatch,
no reification of the message, and no runtime cost.

Error propagation is inherited, not added. Because errors are propagating values
that carry through sends, a chain whose segment answers an error carries that
error to the end of the chain without further machinery. This is the
applicative short-circuit, and it is already the semantics.

## The combinator vocabulary

The design goal is the clarity of ramda or lodash without importing their
shape. Those libraries need currying and data-last argument order because
JavaScript has no receiver; every function must be left waiting for its subject
so that `pipe` can supply it. Birnam has a receiver, so `=>` provides threading
outright. The vocabulary is worth taking; the currying, the data-last ordering,
and the point-free style are not, and point-free reads badly against keyword
messages in any case.

The set below is data-first and receiver-threading throughout. "Have" marks what
`src/id/stdlib.birnam` or the native collection rung already provides.

### Threading control

| Message | Answers | Have |
|---|---|---|
| `tap: aBlock` | self, after evaluating `aBlock` with self | — |
| `then: aBlock` | the value of `aBlock` applied to self | `|>` |
| `when: pred do: aBlock` | `aBlock`'s value if `pred` holds, else self | — |
| `unless: pred do: aBlock` | `aBlock`'s value unless `pred` holds, else self | — |

`tap:` and `then:` are what make `=>` complete: `then:` reaches the block form
from inside a chain, so `=>` and `|>` compose rather than compete, and `#|>`
keeps its standalone use.

`when:do:` matters more than its size suggests. It is the one-armed conditional
that birnam deliberately lacks at the boolean level — `ifTrue:ifFalse:` requires
both arms — and its absence is why a branch in the middle of a sequence forces a
break out to a `let` and a two-armed conditional. That break is a large fraction
of what the guard-heavy stretches of `modules/lsp.birnam` are made of.

### Persistent update

Maps answer new maps, which is precisely why threading is needed. `at:put:` is
native; the rest are the operations that currently decompose into scaffolding.

| Message | Answers | Have |
|---|---|---|
| `at: key put: value` | a new Map with `key` bound | ✓ |
| `atPath: keyVector put: value` | a new Map, creating intermediate Maps | — |
| `update: key with: aBlock` | a new Map, `key` replaced by `aBlock` of its value | — |
| `removeKey: key` | a new Map without `key` | — |
| `merge: aMap` | a new Map, `aMap`'s bindings winning | — |
| `pick: keyVector` / `omit: keyVector` | a restricted Map | — |

`update:with:` needs an absence policy; `update:with:ifAbsent:` is the honest
form and the two-keyword version should be defined in terms of it.

### Absence and choice

This is the `<|>` family — choice with a failure identity. Birnam already has
the operation three times in three spellings: `birnam-peg`'s `choice`, an
`ifAbsent:` chain, and error fallback. Naming it once for collections is the
cheap part of this document.

| Message | Answers | Have |
|---|---|---|
| `at: key ifAbsent: aBlock` | the binding, else `aBlock`'s value | ✓ |
| `atAny: keyVector ifAbsent: aBlock` | the first bound key's value, else `aBlock`'s | — |
| `atPath: keyVector ifAbsent: aBlock` | the nested binding, else `aBlock`'s | — |

`atPath:ifAbsent:` retires the pattern where an intermediate `[Map new]` default
exists only to keep the next `at:ifAbsent:` from failing — the default is
supplied once, at the end, where it means something. `atAny:ifAbsent:` retires
nested `ifAbsent:` blocks that express first-present-wins.

### Structure

| Message | Answers | Have |
|---|---|---|
| `juxt: blockVector` | a Vector of each block applied to self | — |
| `groupBy: aBlock` | a Map from block value to Vector of elements | — |
| `sortBy: aBlock` | a Vector ordered by block value | — |
| `uniqueBy: aBlock` | a Vector, first occurrence per block value | — |
| `zip: aCollection with: aBlock` | a Vector of pairwise block results | — |

`juxt:` is the genuinely applicative member and the one operation `=>` cannot
express, because `=>` is linear by construction. Several independent
computations over one shared input, with the shape fixed in advance, is the
`<*>` shape exactly. It is what turns "extract seven fields from this entry"
from seven bindings into one send.

## Deliberately excluded

- **Currying and data-last ordering.** Unnecessary given a receiver; corrosive
  to keyword messages.
- **Point-free composition.** `compose`/`pipe` over named functions is what `=>`
  replaces.
- **Operator spellings of the applicative interface.** `<$>` and `<*>` sit at
  the binary tier and would require brackets around each application, costing
  more delimiters than they remove. `juxt:` carries the same meaning as a
  message.
- **Automatic lifting of sends over contexts.** Attractive, and in direct
  tension with the no-NULL lock. Deferred rather than rejected, but out of
  scope here.

## Open questions

1. **`=>` in argument position.** `[m at: k put: [n => foo]]` requires brackets,
   consistent with every other non-primary argument. Confirm that this is the
   intended reading and not an omission.
2. **Chain length and the store log.** A long `=>` chain over a persistent Map
   allocates one Map per segment. Whether this is worth a builder path is a
   measurement question, and per `docs/44` §7 the measurement should precede the
   optimisation.
3. **`then:` versus `|>`.** They mean the same thing in different positions.
   Whether `then:` should simply *be* `|>` used as a keyword-free binary inside a
   chain (`=> |> aBlock`) is a spelling decision with no semantic content.
4. **Where the vocabulary lives.** Most of the table is derivable in birnam over
   existing native primitives, which per `docs/44` makes `src/id/stdlib.birnam`
   the default home. `atPath:` variants and `merge:` traverse per-element and are
   the candidates for the native floor if measurement demands it.
5. **`when:do:` naming.** `when:do:` reads well threaded and poorly standalone.
   If it is only ever meaningful inside a chain, that is an argument for the
   name, not against it.
