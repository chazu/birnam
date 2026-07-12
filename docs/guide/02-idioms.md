# Idiomatic birnam

The language chapter tells you what parses; this one tells you what the
codebase itself does. The style is Self-like: prototypes, small methods hung
on singletons, state as facts, errors as values.

## Prototypes, not classes

Every well-known name — `Integer`, `String`, `Vector`, `Html` — is a
**prototype object** that owns its family's behavior. There is no class
hierarchy to design; you clone an object and teach it methods.

```
let counter := [Object clone];
[Reflect defineMethod: #bump on: counter as: [:self |
  let c := [db cellFor: self attr: #n];
  [c set: [[c value] + 1]];
  [c value]]];
```

`clone` is the one creation verb (`new` is an alias). The block's first
parameter is always the receiver — name it `self`.

## The module pattern

A library is a singleton prototype bound in Globals, its methods defined with
docstrings riding the definition:

```
[Globals at: #Greeter put: [Object clone]];
[Reflect docModule: #greeter is: "One-line summary — REGISTER FIRST, before any define."];

[Reflect defineMethod: #hello: on: Greeter as: [:self :name |
  ["hello, " , name]] doc:
  "A greeting for `name`."];
```

Two hard rules learned the slow way:

- `docModule:is:` must precede **every** define in the file — docstrings
  record under the *current* module, so a late registration leaks earlier
  defines into whichever module loaded before you.
- Guard-load dependencies rather than assuming them:

```
[[Globals includes: #Html] ifTrue: [| nil] ifFalse: [| [System load: "modules/html.birnam"]]];
```

## State lives in cells

Bindings don't reassign, and objects have no instance-variable syntax. Mutable
state is a **cell** — a named, historied slot in the fact store, keyed by
object and attribute:

```
let c := [db cellFor: self attr: #count];
[c set: 0];
[c set: [[c value] + 1]];
[c value]
```

This is also the loop-state idiom: a `whileTrue:` body updates cells, not
locals. Seed a cell before reading it — an unset cell answers a raw sentinel,
not nil. `nil` itself is a legal cell VALUE (absence and bound-nil are
distinct; use `ifAbsent:` forms where the protocol offers them).

## Collections are persistent

`add:`, `at:put:` and friends answer a **new** collection; the old one is
unchanged. That is exactly why folds work:

```
[items inject: #[] into: [:acc :e | [acc add: [e squared]]]]
```

`inject:into:` is the universal primitive — `collect:`, `select:`,
`detect:ifNone:`, `count:`, `allSatisfy:` and the rest all derive from it, on
every family (Vector, Map, Range, String). Folds cannot early-exit; for
scan-until-hit over long input use `whileTrue:` with cells, or keep the
scanner on the native floor.

Equality is by **value** where it makes sense: numbers, strings, symbols, and
whole collections compare element-wise with `=`. Identity is `==`.

## Errors as values

There are no exceptions. A failure is a **kinded error value** that propagates
through sends (an error answers itself to almost everything):

```
let r := [File readText: "nope.txt"];      // -> error(#ioError: …)
[r isError];                                // true
[r kind];                                   // #ioError
```

The protocol:

```
[[| [10 / 0]] on: #zeroDivide do: [:e | 0]];      // handle a kind
[[| [self risky]] ensure: [| [self cleanup]]];    // always-run cleanup
[r ifError: [:e | fallback]];                     // branch on error-ness
```

Note the receivers are **zero-arg blocks** — `[10 / 0]` bare would evaluate
eagerly before `on:do:` ever ran; `[| [10 / 0]]` defers it.

The fileIn contract: at the top level of a loaded file, a *discarded* error
aborts the load at that form and `[System load:]` answers it (announcing the
path on stderr). Binding an error with `let` is intentional capture and
continues; inside method bodies, an ignored error is your choice — `on:do:`
is the handled path.

## Documentation and tests ride the system

Docstrings are facts in the image (`birnam doc html`, the LSP hover, and this
site all read the same registry). Tests are the same shape:

```
let s := [TestSuite named: "Greeter" module: #greeter];
[s test: "greets by name" do: [:t |
  [t expect: [Greeter hello: "ada"] equals: "hello, ada" describes: "hello:"]]];
```

Loading a `*-test.birnam` file only registers the suite;
`birnam modules/run-tests.birnam` runs them all and exits 0/1.

## Know where the floor is

One send costs roughly 1.5–2µs. Idiomatic birnam happily spends sends on
surface logic — delegation, composition, one-send-per-line methods — but
per-byte and per-row inner loops belong to the native floor (`split:`,
`trimmed`, `indexOf:`, `bracketDepth`, `sortBy:` are all native for exactly
this reason). Lift surfaces into birnam; keep scanners native; measure before
moving a boundary.
