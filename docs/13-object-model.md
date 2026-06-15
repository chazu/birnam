# The Object Model: Message Passing

Birnam's surface is a Smalltalk-lineage object language: objects receive
messages, messages are resolved against vtables, and behavior is late-bound. The
unusual part is what sits underneath — vtables and methods are
[facts](12-reification.md), and identity is an eid, not a pointer. This page
covers the object/message layer as a programmer sees it.

> Implementation: `src/kernel/send.zig`, `src/kernel/dispatch.zig`,
> `src/kernel/object.zig`, `src/kernel/mcache.zig`. Selectors in
> `src/kernel/kernel_eids.zig`. Syntax in `DEMO.md`.

## Objects, messages, selectors

Everything you interact with is an object reached through a `send`. A message has
a *selector* (the name being sent) and zero or more arguments. Birnam uses the
three classic Smalltalk message forms:

- **Unary** — `[3 squared]`, `[obj vtable]`.
- **Binary** — `[3 + 4]`, `[a < b]`.
- **Keyword** — `[1 to: 10 do: [:i | …]]`, `[dict at: k ifAbsent: [| 0]]`.

Sends are bracket-delimited. A bracket form with no pattern variables is an
evaluated send; a bracket form *containing* `?vars` is instead a quoted query
clause (see [Querying](33-querying.md) and `DATALOG.md`) — one syntax serves both
code and queries.

Selectors are interned to eids and double as attributes (the same identity names
the message `squared` and the slot it might dispatch to). Interning means
selector comparison is integer comparison.

## Vtables are entities; objects carry their vtable

Behavior lives on a *vtable*, which is an ordinary entity. A method binding is a
fact `[vtable selector method]`. An object knows its vtable through a `:vtable`
fact (`ATTR_VTABLE`, eid 19) when cold, or — when hot and materialized — through
the slot at offset `-1` (see [Refs, the -1 trick](14-refs.md)).

The kernel pre-defines a handful of vtables (`kernel_eids.zig`): `OBJECT_VT`,
`SELECTOR_VT`, `CLOSURE_VT`, `VECTOR_VT`, `ASSOC_VT`, `NIL_VT`, plus per-type
vtables added phase by phase — `SMALLINT_VT`, `BOOLEAN_VT`/`TRUE_VT`/`FALSE_VT`,
`STRING_VT`, `RANGE_VT`, `CELL_VT`, and so on. Even immediates dispatch: a
SmallInt's tag resolves to `SMALLINT_VT`, so `[3 + 4]` is a real method send.

## Prototypes, not classes

Birnam has **prototypes, not classes.** There is no class/instance metaclass
tower. A "type" is just a vtable, and a "constructor" is a singleton prototype
object you send a message to. For example `Module` is bound to a singleton
prototype object; `Module load: "lib.dylib"` sends `load:` to it and *mints a new
object* — this is prototype cloning behavior, not class instantiation. The same
pattern recurs for `db`, `Xf` (transducers), `Pattern`/`Grammar` (PEG), and the
`language` prototype.

Inheritance is **delegation**: a vtable may carry a `:delegate` fact
(`ATTR_DELEGATE`) pointing at a parent vtable. Dispatch walks the delegate chain.
`BOOLEAN_VT` holding the shared `not`/`and:`/`or:` while `TRUE_VT` and `FALSE_VT`
each delegate to it is the canonical example.

## Dispatch, two-tier

Resolving `bind(receiver, selector)` (`VISION.md` §Dispatch):

1. **Hot path.** Tag-check the receiver Ref. If it is a raw pointer, load
   `vt = obj[-1]`; probe the method cache (`mcache`) for `(vt, selector)`; on a
   hit, tail-call the method.
2. **Cold path.** On a cache miss or an eid-tagged receiver, query EAVT for the
   receiver's vtable and then `[vtable selector ?method]`, walking the delegate
   chain as needed; populate the mcache; dispatch.
3. **Invalidation.** Any assert/retract on `(vt, selector)` invalidates the
   cache entry — so live method replacement is immediately visible.

The cache is a 1024-entry direct-mapped table keyed on `(vt_eid, sel_eid)`.
Measured warm dispatch is ~8 ns/op (roughly 6.4× a native indirect call) — the
Smalltalk-VM ballpark the vision targets. See
[The Performance Model](18-performance-model.md) for the full breakdown and the
measurement story behind it.

## `doesNotUnderstand:`

When dispatch finds no method for a selector anywhere in the delegate chain, the
receiver is sent `doesNotUnderstand:` (`SEL_DNU`, eid 198) with the missing
selector as an eid `Ref` (`mkEid(sel)` — the interned selector entity). This is
the standard Smalltalk reflective hook — it is what
lets proxies, forwarders, and dynamic responders be written in Birnam rather than
the kernel. It is seeded at boot so `send.zig` can reference it without interning.

## Built-in protocols

A few protocols are defined by the kernel/stdlib so the basic types behave
uniformly:

- **Number protocol.** SmallInts and boxed floats share arithmetic/comparison
  selectors (`+ - * / = < >`) with mixed-type coercion (`[3.14 + 2.0]`,
  `[3.14 asInt]`). SmallInt additionally has `<= >=` (and `mod:`, `min:`, `max:`,
  `between:and:`, …); floats currently expose only `< >` for ordering plus
  `sqrt`/`floor`/`ceil`/`round`. There is no `//` selector.
- **Boolean protocol.** `True`/`False` singletons answer
  `ifTrue:ifFalse:`/`ifTrue:`/`ifFalse:`/`not`/`and:`/`or:`/`whileTrue:`.
- **nil protocol.** `nil` is the eid-0 singleton object — dispatchable
  (`[nil isNil] → true`), distinct from absence (see
  [the No-NULL discipline](11-fact-model.md)).

The collection/sequence protocol (`collect:`, `select:`, `inject:into:`, …) is
large enough to warrant its own page: [Collections](17-collections.md).

## Blocks and closures

Blocks are first-class closures with `:param` syntax and lexical capture:

```
let makeAdder := [:a | [:b | [a + b]]]
let add10 := [makeAdder value: 10]
[add10 value: 5]    " => 15 "
```

A block is sent `value` / `value:` / … to invoke it. Non-local return is `^`.
Blocks are entities with `BLOCK_VT`/`CLOSURE_VT` vtables and compiled bytecode
bodies — reified like everything else.

> **Eager send vs. deferred block.** A bracketed `[recv msg]` is a *message
> send that runs immediately*. A *block* is `[| … ]` / `[:x | … ]` — the
> leading `|` makes it a deferred value. Anything that should run *under* a
> control construct (`ifTrue:`, `whileTrue:`, `on:do:`, `ensure:`) must
> therefore be a block, not a bare send.

## Errors are values (the Result model)

Birnam has no exceptions. A failed operation returns a first-class **error
value** (an entity with `ERROR_VT`) rather than crashing the process or
silently returning `nil`. This keeps errors *data* — inspectable, storable,
queryable — consistent with the fact-store identity, and it works uniformly
across both evaluators (tree-walker and bytecode VM) without stack-unwinding
machinery.

- **Propagation rides message dispatch.** `ERROR_VT`'s `doesNotUnderstand:`
  returns the error itself, so sending an error *any* unhandled message yields
  the error. A chain like `[[x foo] bar]` short-circuits to the first error —
  no special case in the send loop, just the object model doing its job.
- **Halt + report.** A statement sequence stops at the first error (later
  statements don't run), and an uncaught error surfaces at the REPL / file-in
  as a distinct `! error: <message>` line rather than an ordinary `=> value`.
- **Kinded.** Errors carry a `kind` symbol (`#zeroDivide`,
  `#undefinedVariable`, `#undefinedAssignment`, …) plus a `message`. Read them
  with `[e kind]` / `[e message]`; test any value with `[x isError]`.

Surface:

```
[#[1 2 3] at: 99]                              " => error: vector index out of bounds "
[[| [10 / 0]] on: #zeroDivide do: [:e | 0]]    " catch by kind        => 0 "
[[| risky] on: #Error do: [:e | [e message]]]  " #Error = catch any "
[[| body] ensure: [| [stream close]]]          " cleanup runs on value OR error "
["disk full" error: #io]                        " raise your own (catchable) "
```

`on:do:` and `ensure:` are blocks-as-handlers on `CLOSURE_VT`; an unmatched
`on:do:` lets the error keep propagating. Because the protected computation is
a block, remember the eager-vs-block rule above: `[[| [10 / 0]] on: …]`, not
`[[10 / 0] on: …]`.

**What this trades away** (vs. Smalltalk exceptions, accepted deliberately): no
resume/retry, and cleanup across arbitrary frames must use bracket combinators
(`ensure:`, `withFile:`) rather than dynamic-extent handlers. See ROADMAP
Phase 17.7 for the full decision and the deferred items (VM mid-sequence halt;
type-mismatch hardening).

## Where to go next

- The Ref encoding behind identity and the -1 trick: [Refs](14-refs.md).
- The collection protocol: [Collections](17-collections.md).
- How hot objects get materialized for dispatch speed:
  [Promotion](25-promotion.md) (Part 2) and
  [The Performance Model](18-performance-model.md).
