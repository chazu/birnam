# Collections and the Sequence Protocol

Birnam ships a small family of collection literals and a single unified sequence
protocol that works across all of them. This page covers the literal syntax, the
core protocol (`collect:`/`select:`/`inject:into:`/…), ranges, the `reduced`
early-out, and transducers — the user-facing layer built on the
[RRB-Tree and HAMT](16-persistent-structures.md) substrate.

> Vtables/selectors: `src/kernel/kernel_eids.zig` (`VECTOR_VT`, `RRB_VT`,
> `ASSOC_VT`, `RANGE_VT`, `STRING_VT`, `REDUCED_VT`, `XF_VT`). Built in Phase 16
> (Tiers A–E). Design notes: project memory (`stdlib_locks`).

## The bracket-literal family

The bracket family, distinguished by bracket and prefix. Two general-purpose
value literals are shipped (`#[ ]` and `{ }`); the other two forms are reserved:

| Form | Kind | Status | Example |
|---|---|---|---|
| `#[ … ]` | vector (RRB-backed sequence) | shipped | `#[a b c]` |
| `{ … }` | map / association | shipped | `{k: v j: w}` |
| `#( … )` | quoted clause-list (used inside `where: #(…)`), not a general array | shipped, query-only | `where: #( [?e :name ?n] )` |
| `#{ … }` | set | grammar reserved; proto not yet implemented | `#{1 2 3}` |

Map entries are space-separated, not comma-separated. Vectors are RRB-Trees; maps
are HAMTs. Both are persistent — "modifying" one returns a new collection sharing
structure with the original.

## One sequence protocol over everything

The defining design choice (Phase 16 "SEQ-UNIFICATION"): a *single* sequence
protocol is implemented once on `OBJECT_VT` in terms of `inject:into:`, and every
sequence-shaped thing answers it — vectors, ranges, maps, strings. Add
`inject:into:` (and a notion of iteration) to a type and it inherits the whole
protocol:

- `collect:` — map a block over elements → new sequence.
- `select:` — filter by a predicate block.
- `detect:` (and `detect:ifNone:`) — first matching element.
- `count` / `count:` — size, or count of matches.
- `inject:into:` — the fundamental fold (`reduce`) everything else is built on.
- `min:` / `max:` / `between:` — comparison reductions.
- `asVector` — realize any sequence into an RRB vector.

Pipelines compose with the `|>` thread operator, which threads a value as the
*receiver* of the next send (see [Language Tour](31-language-tour.md)):

```
#[1 2 3 4] |> select: [:x | [x even]] |> collect: [:x | [x * x]]
```

Because the protocol is one implementation over `inject:into:`, a vector, a
range, a map, and a string all run the *same* `collect:` code path.

## Ranges

A range is a lazy integer sequence (`RANGE_VT`), minted with `to:` / `to:by:`:

```
[1 to: 10]          " 1,2,…,10 "
[0 to: 20 by: 5]    " 0,5,10,15,20 "
```

Ranges answer `do:`/`inject:into:`, so they plug into the sequence protocol
without materializing — `[1 to: 1000000] |> count` does not allocate a
million-element vector. The familiar `[1 to: 10 do: [:i | …]]` iteration form is
the same range answering `do:`.

## `reduced` — early termination

`reduced` (`REDUCED_VT`) is a one-slot box that wraps a final fold value. When
`inject:into:` sees a `reduced`, it stops folding and returns the wrapped value.
This is how short-circuiting operations (`detect:`, `take:`, "any?/all?") are
built on top of the single fold primitive without a special control-flow
mechanism.

## Transducers

Transducers (`XF_VT` / `XF_PROTO`, Phase 16 Tier E) are composable
transformations decoupled from the collection they run over. The `Xf` prototype
provides constructors:

- `Xf map: [block]`
- `Xf filter: [block]`
- `Xf take: n` (implemented via accumulator size + `reduced`, no mutable cell)
- `aXf then: bXf` — composition (rides `CLOSURE_VT`; a dedicated `comp` is
  deferred per the design DNA until channels justify it)

A transducer is an `rf → rf'` transformation. `coll transduce: xf` runs the whole
pipeline in **one eager fold** — no intermediate vectors between stages, unlike a
`|>` chain that materializes between steps. Same transducer can later drive a
different context (e.g. a channel) without change.

## Applicative helpers

Phase 16/17.6 adds accumulating combinators over collections:

- `validate:` — applicative validation that *accumulates* errors rather than
  short-circuiting on the first.
- `zip:` — pair up two sequences.
- `whenAll:` — proceed only when all of a set of values are present.

## Maps and associations

Maps (`{k: v}`, `ASSOC_VT`) are HAMTs. Keyed access follows the No-NULL
discipline (see [The Fact Model](11-fact-model.md)): `at:ifAbsent:` is the
primary form; bare `at:` is `at:ifAbsent:[raise]`, because absence is not a value
and the system refuses to manufacture one. Out-of-bounds sequence access is a
domain error (raise), not a nil sentinel.

## Where to go next

- The persistent structures underneath: [Persistent Data Structures](16-persistent-structures.md).
- Using these at the REPL: [Working with the stdlib](32-stdlib.md) (Part 3).
- The `|>` pipe and block syntax: [Language Tour](31-language-tour.md) (Part 3).
