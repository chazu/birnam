# 64 — Pipeline threading and the combinator vocabulary

> **Status:** Implemented, 2026-08-04. This document specifies the shipped
> message-bearing thread marker (`=>`) and its default-stdlib combinator set.
> The syntax and library vocabulary remain separate implementation layers.

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

The feature is narrow and additive for current executable source, with one
language-level reservation: the previously legal but unused exact binary
selector `=>` becomes structural punctuation. Specifically unchanged:

- **`#|>` remains an ordinary method.** The default stdlib defines it on
  `Object` as `[block value: self]`, and the stdlib suite asserts that a
  prototype may override it. Both stand.
- **Every send expression remains bracketed.** `=>` permits several dispatches
  inside that expression; it does not introduce statement-level bracket
  elision, at top level or in block bodies.
- **No precedence tiers.** `message` remains the flat choice of keyword, binary,
  and unary forms. A threaded leg is one such dispatch, and an argument that is
  not a primary still requires brackets.
- **No implicit lifting.** Absence continues to ride `ifAbsent:` under the
  no-NULL lock. Nothing in this document introduces a nil-propagating send.
- **The Id API floor does not grow.** Parsing uses the PEG construction protocol
  already imported by `birnam-lang`; lowering produces the existing nested
  `Expr::Send` shape; runtime dispatch remains the ordinary Id API send path.

## The `=>` thread marker

`=>` introduces one dispatch within a bracket. The result of each dispatch
becomes the receiver of the next. Every threaded leg carries its own marker,
including the first:

```
[Map
  => new
  => at: "name" put: name
  => at: "kind" put: kind
  => at: "range" put: range]
```

Do not make the first dispatch exceptional. In particular, the mixed spelling
`[Map new => at: "name" put: name]` is invalid; write
`[Map => new => at: "name" put: name]`. Requiring the marker on every leg makes
the CST uniform and prevents the first message's ordinary chain grammar from
swallowing the first marker.

`=>` is grammar rather than a method: its right operand is a dispatch, not a
value, so it cannot be received. This is the one respect in which it differs
from `|>`, and the reason both exist rather than one. A leg is exactly one
dispatch:

- `=> size` is one unary dispatch;
- `=> + 1` is one binary dispatch;
- `=> at: key put: value` is one keyword dispatch, however many selector parts
  it contains.

Ordinary same-category chains do not occur inside a leg. Write
`[x => foo => bar]`, not `[x => foo bar]`, and `[x => + 1 => * 2]`, not
`[x => + 1 * 2]`. This is both the least ambiguous grammar and the most literal
reading of one marker per dispatch.

### Relation to cascades

The surface is cascade-shaped punctuation, but the semantics are not a classic
Smalltalk cascade. A classic cascade sends every message to the original
receiver. `=>` sends each message to the preceding result:

```
classic cascade:  x foo; bar          ~= [x foo]; [x bar]
result threading: [x => foo => bar]    = [[x foo] bar]
```

They coincide only when every preceding message answers its receiver. The
difference is essential for persistent values: each `at:put:` answers a new Map,
and the next leg must receive that new Map rather than the original. `=>` is
therefore a result-threading chain with cascade-shaped grammar, not an
original-receiver cascade.

### Token

Before this design landed, `=>` appeared in executable source only inside
strings and comments, where it read as "yields" (`[21 double]  "=> 42"`). The
characters were already part of the binary-operator charset, so implementation
required no lexer or Id API change.

The exact maximal operator token `=>` is reserved in message position. It is no
longer an invocable binary selector in source. Reservation is exact:

- `#=>` remains a legal symbol literal. Symbols are data and continue to use
  the unrestricted `binops` rule.
- A longer maximal operator such as `=>+` remains a legal binary selector.
- A structural `=>` requires at least one whitespace or comment unit after it.
  `[x =>foo]` is a syntax error rather than a fallback binary send. Formatting
  should put whitespace on both sides even though the existing message rules
  may consume the whitespace preceding a later marker.

`|` is not available for this purpose: it is in the binop charset and means
eager boolean or, so `[x | y]` is already a send.

### Rule

In schematic PEG, with `spacing1` meaning one-or-more whitespace characters or
comments:

```
threadMarker          := '=>' spacing1
reservedThreadSelector := threadMarker | ('=>' !binop)

binaryMsg      := !reservedThreadSelector binops ws expr
threadDispatch := keywordMsg | binaryMsg | unaryMsg
threadedMsg    := threadMarker threadDispatch
                  (ws threadMarker threadDispatch)*
ordinaryMsg    := keywordMsg | (binaryMsg ws)+ | (unaryMsg ws)+
message        := threadedMsg | ordinaryMsg
send           := '[' ws expr ws message ws ']'
```

The negative lookahead is added inline before the existing `binops` rule in
`binaryMsg`; `symbol` continues to use unrestricted `binops`. Keeping `binops`
as the binary message's CST child preserves the ordinary CST shape. The two
reservation alternatives cover both a valid marker (including a comment as its
required spacing) and a bare exact `=>` followed by a non-operator character,
so `[x =>foo]` cannot fall back. A longer run such as `=>+` matches neither
reservation and remains an operator.

`spacing1`, `threadMarker`, and `reservedThreadSelector` are names for inline
pattern values in this sketch, not additional grammar rules. They therefore do
not add CST wrapper nodes. The only new named CST rule nodes are
`threadDispatch` and `threadedMsg`.

`threadDispatch` names the single-dispatch variants, not the existing `message`
rule. Reusing `message` here is incorrect: its binary and unary alternatives
repeat greedily, so `[x + 1 => foo]` can be read as two ordinary binary sends,
with `=>` silently swallowed as the second selector. Under this contract that
mixed form is invalid; the threaded spelling is `[x => + 1 => foo]`.

The `threadedMsg` alternative is ordered before `ordinaryMsg`. Reservation in
`binaryMsg` is still required: PEG choice may try `ordinaryMsg` after a malformed
threaded form, and malformed syntax must not silently fall back to a send of
`#=>`.

Keyword legs terminate without ambiguity. After `put: name`, `keywordMsg`
looks for `ident ':'`, finds `=>`, and stops.

### Semantics

A leg is one whole unary, binary, or keyword dispatch. `[x => foo => bar: 1]`
is `[[x foo] bar: 1]` and must compile to the identical send sequence — `=>`
introduces no new dispatch, no reification of the message, and no runtime cost.

An argument-position chain remains bracketed:

```
[m at: k put: [n => foo]]
```

There is no unbracketed threaded argument and no new precedence rule.

Error propagation is inherited, not added. An unhandled error carries through
ordinary sends under the existing ErrorVt protocol; explicit handlers such as
`ifError:` may recover it. A method inherited from `Object` is still a real
dispatch, however: `|>` is not implicitly success-only and may apply its block
to an error. Use `ifNotError:` when a block stage must be skipped on failure,
and define the new general `Object` combinators through that protocol. `=>`
itself adds no short-circuit rule.

Allocation is inherited too. A persistent update in each leg allocates exactly
as it does in the equivalent nested-send source; S1 and S2 add no builder,
fusion, or chain-length threshold. Any later optimization requires measured
evidence from a real workload, must preserve ordinary-send observability and
evaluation order, and remains outside the `=>` syntax contract and the Id API.

### CST and lowering contract

`threadedMsg` is a named CST node. Its semantic children are `threadDispatch`
nodes in source order; terminals such as `=>` do not become CST children under
the frozen PEG contract. Each dispatch wraps exactly one `keywordMsg`,
`binaryMsg`, or `unaryMsg`, retaining that rule's exact source span.

`rust/birnam-codegen` recognizes `threadedMsg` in `lower_send`, starts with the
already-lowered receiver, and folds its dispatch children left-to-right through
the existing `lower_message` helper. It adds no `Expr` variant. As with existing
same-category chains, every resulting `Expr::Send` carries the whole bracketed
send as its source span; the individual CST nodes remain available for precise
editor and diagnostic spans.

For ordinary messages, emitted code for a threaded form must be byte-for-byte
identical to the corresponding nested-bracket form. Source-sensitive special
forms are called out below because some are recognized from CST shape before
ordinary lowering.

### Id API boundary

The implementation stays entirely above the floor:

1. `birnam-lang` constructs the new rules with the existing PEG `literal`,
   `seq`, `choice`, repetition, rule-reference, and negative-lookahead protocol.
2. `birnam-codegen` lowers the new CST shape into existing nested `Expr::Send`
   values.
3. The emitter and runtime see only ordinary sends.

No `IdApi` field, ABI entry, runtime selector, bytecode operation, or dispatch
case is added. Expanding the Id API floor to implement `=>` is a design failure.

### Executable examples

- [`examples/pipeline.birnam`](../../examples/pipeline.birnam) contrasts block
  application with result-threaded unary, binary, keyword, conditional, and
  error-recovery stages.
- [`examples/combinators.birnam`](../../examples/combinators.birnam) exercises
  the persistent Map, absence, control, and structural combinators together.

Both are ordinary scripts, not documentation-only snippets. The same behavior
is pinned by the language, stdlib, and LSP module suites.

### Source-sensitive special forms

The ordinary spellings remain canonical for forms that
`birnam-codegen-runtime` recognizes from raw CST shape before general lowering:

- `[System embedFile: ...]`;
- `[Module named: ... requires: ...]`;
- `[Alien dlopen: ...]`;
- live `Reflect` operations with selectors `defineMethod:on:as:`,
  `defineMethod:on:as:doc:`, and `removeMethod:on:`.

A threaded send whose original receiver and first leg match one of those forms
is rejected with a compile diagnostic. It is not normalized through a special
"head dispatch" view and must never silently degrade into a runtime-only send
that loses embedding, artifact metadata, or live-revision metadata. This is a
source-form restriction, not an Id API or runtime restriction. It can be
revisited only for a concrete use case with an explicit metadata contract.

## Implementation slices

### S1 — grammar, CST, and lowering (delivered)

- Added exact-selector reservation and `spacing1` locally in `birnam-lang` using
  existing PEG constructors.
- Added `threadDispatch` and `threadedMsg`, preserving the ordinary CST shape for
  source that does not use `=>`.
- Folded `threadedMsg` in `birnam-codegen::lower_send` through `lower_message`.
- Diagnosed and rejected the threaded special-form heads listed above while
  preserving all existing ordinary-form recognizers and metadata tests.

### S2 — language tooling and documentation (delivered)

- Taught `modules/lsp.birnam` to traverse `threadedMsg` for occurrences,
  bindings, selector-arity diagnostics, and symbols where applicable.
- Highlighted `=>` in `tools/emacs/birnam-mode.el` and pinned marker-leading
  indentation/highlighting in its ERT suite.
- Added focused language-guide coverage and an executable example that keeps the
  syntax independent from the combinator implementation.

### S3 — combinator vocabulary (delivered)

Landed `tap:`, conditional threading, persistent-update helpers, absence helpers,
and structural combinators as separately reviewed library slices. They are not
prerequisites for `=>` and do not share its parser acceptance gate. Their
canonical home is the default self-hosted `#stdlib`, on the narrowest protocol
that supports each operation; there is no separate `#combinators` module. Reuse
the shipped error-stage protocol rather than adding recovery aliases, and define
new `Object` combinators through `ifNotError:` so errors remain propagating.

### Acceptance matrix

Parser/CST tests must cover:

- one and several legs;
- every category transition: unary/binary/keyword to
  unary/binary/keyword;
- a multi-part keyword selector as one dispatch;
- `[x => + 1 => * 2]` and rejection of mixed `[x + 1 => foo]`;
- nested `[m at: k put: [n => foo]]`;
- whitespace, newline, and comment spacing after the marker;
- rejection of `[x =>]`, `[x =>foo]`, and `[x => => foo]`;
- preservation of ordinary message chains and of a longer selector such as
  `=>+`;
- parsing `#=>` as a symbol literal;
- compile diagnostics for each forbidden threaded special-form head, alongside
  unchanged success and metadata behavior for its ordinary spelling.

Lowering and runtime tests must compare threaded forms with their nested-send
equivalents, including evaluation order, receiver-once evaluation, error
propagation, and ordinary emitted C. Tooling tests must prove that LSP
occurrences and diagnostics reach every leg and that Emacs highlights and
indents marker-leading lines. The focused suites run first; the delivered slice
then passes `mise run birnam-test`, `mise run emacs-test`, and the repository's
canonical `mise run verify` gate.

## The combinator vocabulary

The design goal is the clarity of ramda or lodash without importing their
shape. Those libraries need currying and data-last argument order because
JavaScript has no receiver; every function must be left waiting for its subject
so that `pipe` can supply it. Birnam has a receiver, so `=>` provides threading
outright. The vocabulary is worth taking; the currying, the data-last ordering,
and the point-free style are not, and point-free reads badly against keyword
messages in any case.

The set below is data-first and receiver-threading throughout. Every entry is
shipped by `src/id/stdlib.birnam` or the native collection rung.

### Ownership

The shared vocabulary lives in `src/id/stdlib.birnam`, which already owns
derived, broadly applicable `Object` and `Collection` protocols and is loaded by
default. Do not create an opt-in `#combinators` module: these selectors are
ordinary language vocabulary and should not carry hidden dependency ceremony.

Install each method on the narrowest applicable protocol:

- `Object`: `tap:`, `when:apply:`, `unless:apply:`, and `juxt:`;
- `Collection`: `groupBy:`, a general `sortBy:` surface over the existing native
  Vector implementation, `uniqueBy:`, and `zip:with:`;
- `MapVt`: path access/update, keyed update/removal/merge, pick/omit, and
  first-present helpers.

Methods remain self-hosted derivations over existing primitives. A measured hot
path may later gain a transparent implementation in an existing native module,
under the same selector and tests; it never expands the Id API. Moving
`atPath:put:` into this shared Map protocol and removing TOML's local definition
is one atomic change, so module load order can never choose the implementation.

### Threading control

| Message | Answers | Status |
|---|---|---|
| `tap: aBlock` | self, after evaluating `aBlock` with self | ✓ |
| `|> aBlock` | the value of `aBlock` applied to self | ✓ |
| `when: pred apply: aBlock` | `aBlock`'s value if `pred` holds, else self | ✓ |
| `unless: pred apply: aBlock` | `aBlock`'s value unless `pred` holds, else self | ✓ |

`tap:` rounds out the threading vocabulary but is not required to implement the
syntax. The existing `|>` is the block-application dispatch inside a chain:
`=> |> aBlock`. No `then:` alias is added. The visually explicit pair preserves
one public spelling for block application: `=>` introduces the leg and `|>` is
the ordinary overridable selector that leg sends.

`when:apply:` matters more than its size suggests. It is the one-armed
conditional transform that birnam deliberately lacks at the boolean level —
`ifTrue:ifFalse:` requires both arms — and its absence is why a branch in the
middle of a sequence forces a break out to a `let` and a two-armed conditional.
`apply:` is deliberate: unlike the iteration/action convention of `do:`, the
taken branch's value replaces the receiver. `unless:apply:` is its exact
predicate-negated twin.

### Error stages

Error handling already has the required combinators in stdlib; `=>` gives them
a linear position but adds no error selector:

| Message | On a non-error | On an error | Status |
|---|---|---|---|
| `ifError: handler` | answers self | answers `handler` applied to self | ✓ |
| `ifNotError: block` | answers `block` applied to self | answers self | ✓ |

`ifError:` is recovery: if its handler answers a value, that value becomes the
receiver of the next leg; if it answers an error, the error keeps propagating.
`ifNotError:` is success-only block application. Kind-selective recovery remains
the existing deferred boundary `[[| computation] on: kindSpec do: handler]`, and
`ensure:` remains the always-run cleanup boundary. Do not add `recover:`,
`catch:`, or a second spelling without repeated evidence for a distinct
mid-chain operation.

The new `Object` methods must preserve this protocol. Define `tap:`,
`when:apply:`, `unless:apply:`, and `juxt:` through `ifNotError:` (or prove the
equivalent ErrorVt override), so neither their predicates nor their blocks run
on an error inherited from an earlier leg. Acceptance tests cover both the
non-error behavior and untouched error identity for every such method.

### Persistent update

Maps answer new maps, which is precisely why threading is needed. `at:put:` is
native; the rest are the operations that currently decompose into scaffolding.

| Message | Answers | Status |
|---|---|---|
| `at: key put: value` | a new Map with `key` bound | ✓ |
| `atPath: keyVector put: value` | a new Map, creating intermediate Maps | ✓ |
| `update: key with: aBlock` | a new Map, `key` replaced by `aBlock` of its value | ✓ |
| `update: key with: aBlock ifAbsent: absentBlock` | the transformed Map, or `absentBlock`'s value | ✓ |
| `removeKey: key` | a new Map without `key` | ✓ |
| `merge: aMap` | a new Map, `aMap`'s bindings winning | ✓ |
| `pick: keyVector` / `omit: keyVector` | a restricted Map | ✓ |

`atPath:put:` moved atomically from TOML's local protocol to the shared Map
protocol in stdlib, so module load order cannot choose its implementation.

`update:with:ifAbsent:` is the honest form: when the key is absent, the fallback
block's value is the result of the operation, matching `at:ifAbsent:`. The
two-keyword `update:with:` is defined through it and answers a `#notFound` error
for an absent key.

### Absence and choice

This is the `<|>` family — choice with a failure identity. Birnam already has
the operation three times in three spellings: `birnam-peg`'s `choice`, an
`ifAbsent:` chain, and error fallback. Naming it once for collections is the
cheap part of this document.

| Message | Answers | Status |
|---|---|---|
| `at: key ifAbsent: aBlock` | the binding, else `aBlock`'s value | ✓ |
| `atAny: keyVector ifAbsent: aBlock` | the first bound key's value, else `aBlock`'s | ✓ |
| `atPath: keyVector ifAbsent: aBlock` | the nested binding, else `aBlock`'s | ✓ |

`atPath:ifAbsent:` retires the pattern where an intermediate `[Map new]` default
exists only to keep the next `at:ifAbsent:` from failing — the default is
supplied once, at the end, where it means something. `atAny:ifAbsent:` retires
nested `ifAbsent:` blocks that express first-present-wins.

### Structure

| Message | Answers | Status |
|---|---|---|
| `juxt: blockVector` | a Vector of each block applied to self | ✓ |
| `groupBy: aBlock` | a Map from block value to Vector of elements | ✓ |
| `sortBy: aBlock` | a Vector ordered by block value | ✓ |
| `uniqueBy: aBlock` | a Vector, first occurrence per block value | ✓ |
| `zip: aCollection with: aBlock` | a Vector of pairwise block results | ✓ |

`zip:with:` stops at the shorter collection; it neither pads nor reports a
length mismatch.

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

## Decided boundary

The grammar, reservation, lowering direction, argument bracketing, ordinary
allocation behavior, block application through the existing `|>`, stdlib
ownership, conditional-transform naming, reuse of the existing error-stage
protocol, and Id API boundary are decided, including the restriction on
source-sensitive special forms.
