# Everything Is Facts: Reification

Tenet 1 says *everything is an object*. Tenet 0 says *the image is a database*.
Put together, they force a stronger claim: the machinery of the language itself —
its vtables, methods, grammar, syntax trees, and bytecode — is stored as facts in
the same indexes as your data. This page explains what that means and why it is
the source of Birnam's malleability.

> Design: `VISION.md` §Object Model, §Compilation Path. Demonstrated end-to-end
> in `DEMO.md`. Eid catalog: `src/kernel/kernel_eids.zig`.

## The claim

There is no separate "implementation" layer hiding behind the object system.
Walk the stack and every level is fact-shaped:

| Layer | Stored as facts… |
|---|---|
| **Vtables** | an entity per vtable; `[vt selector method]` triples |
| **Methods** | closure entities; `[method :bytecode-stream …]`, `[method :constant-pool …]` |
| **Grammar** | PEG patterns as entities (`:pattern-kind`, `:pattern-first-child`, …) |
| **Syntax trees** | CST/AST nodes as entities (`:ast-rule-name`, `:ast-selector`, linked children) |
| **Bytecode** | byte slices referenced by `:bytecode-stream` facts on method entities |
| **Diagnostics** | parse failures as entities (`:peg-expected`, `:peg-position`) |
| **Dispatch state** | `:vtable`, `:delegate`, even a `:bind` override hook — all facts |

Because each of these is *just facts*, all the substrate's mechanisms apply to
them uniformly: you can query them, diff them across transactions, snapshot them,
retract and replace them live, and attribute them to a module.

## A worked example: a method is a fact

Installing a method is not a special operation — it is an assertion. Conceptually:

```
[someVtable  #squared  <closure-eid>]
```

To *replace* `squared`, you assert a new `[someVtable #squared <new-closure>]`;
the `:one` cardinality of the selector-attribute implicitly retracts the old
binding (see [Cardinality](11-fact-model.md)). The change is itself stamped with
a `tx`, so "when did `squared` change, and to what?" is answerable after the
fact — for *any* method, including kernel ones, without having planned to log it.

This is why tenet 4 ("live mutation is first-class") and tenet 6 ("reflection is
query") are not separate features. They are the same fact mechanism read in two
directions.

## The compilation pipeline is fact-shaped

Source does not flow through opaque in-memory structures. Each stage materializes
its output as facts (`DEMO.md`):

```
source string
  → PEG engine          (fact-shaped patterns + packrat memo)
  → concrete syntax tree (entities: :ast-rule-name, byte spans, linked children)
  → desugar pass         (CST → typed AST with per-kind vtables)
  → compile block bodies → bytecode (stored as facts on method entities)
  → eval: top-level AST is tree-walked; user methods run on the stack VM
  → result Ref
```

Every stage above the kernel substrate is therefore malleable. PEG patterns,
grammar rules, the per-kind AST eval methods, SmallInt primitives, and your own
methods are all asserted facts that can be retracted and replaced live without a
kernel restart. The grammar that parses Birnam is data that Birnam can edit.

## Dispatch reads facts

The most consequential reification is dispatch itself. Resolving a message
`[receiver selector]` ultimately consults facts:

- The receiver's vtable is found via its `:vtable` fact (`ATTR_VTABLE`, eid 19)
  on the cold path.
- The method is the value of `[vtable selector ?method]`.
- The delegate (inheritance) chain is `:delegate` facts (`ATTR_DELEGATE`).
- `bind` can even be *overridden in Birnam*: a `:bind` fact (`ATTR_BIND`) on a
  vtable carries a closure that runs before the Zig primitive — metacircular
  dispatch policy.

A method cache (`mcache`) keeps the hot path from re-querying every send, and a
materialization layer promotes hot objects to structs for speed — but the
*source of truth* is always the facts. The cache is just a cache. This is the
central performance tension, treated in detail in
[The Performance Model](18-performance-model.md).

## Why this matters

Reification is what makes the malleability boundary
([the vision](10-vision.md)) sit so high. Because the compiler, grammar, AST,
bytecode, and dispatch are all facts above the frozen kernel, you can rewrite any
of them from inside the running system. The kernel only has to know how to store,
index, query, and dispatch on facts — not what those facts *mean*. Meaning lives
in the malleable tower.

## Where to go next

- The object/message layer that sits on these facts:
  [The Object Model](13-object-model.md).
- How facts are physically stored and shared:
  [Persistent Data Structures](16-persistent-structures.md).
- The cost of source-of-truth-is-facts: [The Performance Model](18-performance-model.md).
