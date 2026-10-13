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
[Module load: #html];
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

`inject:into:` supplies the general Collection protocol, including `collect:`,
`select:`, `count:` and `allSatisfy:`. For first-match search, use `detect:` or
`detect:ifNone:`. The installed Vector, Range and String drivers stop traversal
at the first non-nil match. Map searches its values projection in the same order;
constructing that projection still visits the map. A matching nil means "not
found", so search continues. Custom collections that provide only a fold retain
the generic search fallback. Predicates and traversal helpers remain replaceable
ordinary methods; search callbacks can suspend through the stackless driver.

Equality is by **value** where it makes sense: numbers, strings, symbols, and
whole collections compare element-wise with `=`. Identity is `==`.

## Result-threaded construction

Persistent construction is the primary `=>` use: each update's new value feeds
the next receiver without a numbered-temporary ladder.

```
[Map
  => new
  => at: "bind" put: "127.0.0.1"
  => at: "port" put: 4400]
```

Every leg begins with `=>` and contains exactly one dispatch. `|>` remains the
overridable block-application method; inside a receiver thread it is an
ordinary binary leg, written `=> |> aBlock`, not aliased as `then:`.

The general combinators live in the default self-hosted stdlib:

```
[value
  => tap: observe
  => when: predicate apply: transform
  => unless: otherPredicate apply: fallback]
```

`apply:` is value-producing; `do:` remains the iteration/action convention.
These `Object` combinators preserve an error without running their predicates or
blocks, using the existing `ifNotError:` protocol.

Maps provide persistent update and absence helpers: `atPath:put:`,
`update:with:`, `update:with:ifAbsent:`, `removeKey:`, `merge:`, `pick:`,
`omit:`, `atAny:ifAbsent:`, and `atPath:ifAbsent:`. The plain
`update:with:` answers a `#notFound` error for an unbound key; use the explicit
form when absence is expected. None mutate the receiver.

The structural vocabulary is `juxt:`, `groupBy:`, `sortBy:`, `uniqueBy:`, and
`zip:with:`. `juxt:` runs several independent blocks over one value;
`groupBy:` preserves input order within each bucket, `uniqueBy:` preserves the
first value for each derived key, and `zip:with:` stops at the shorter
collection.

The complete runnable demonstrations are
[`examples/pipeline.birnam`](../../examples/pipeline.birnam) and
[`examples/combinators.birnam`](../../examples/combinators.birnam).

## Application configuration

The standard `AppConfig` module resolves one application's startup snapshot;
it does not become a mutable global settings registry. Give it defaults and the
arguments that belong to the application:

```
let defaults := [Map
  => new
  => at: "bind" put: "127.0.0.1"
  => at: "port" put: 4400];
let config := [AppConfig resolve: #example defaults: defaults args: args];
[config at: "port"];
[config originAt: "port"]
```

Values resolve in this order: defaults, TOML, environment, then command-line
arguments. The conventional file is `~/.example/example.toml`; `--config PATH` and
`EXAMPLE_CONFIG` select another path. A setting named `port` uses `EXAMPLE_PORT`
and `--port VALUE` (or `--port=VALUE`). `String`, `Integer`, and `Boolean`
defaults establish the scalar conversion contract. `path`, `values`, and
`origins` expose the selected file and resolved snapshot for diagnostics.

Keep history and other durable application data in a separate module; an
`AppConfig` snapshot should not own those writes.

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
[r ifError: [:e | fallback]];                     // recover an error
[r ifNotError: [:value | [value printString]]];   // transform only success
```

Note the receivers are **zero-arg blocks** — `[10 / 0]` bare would evaluate
eagerly before `on:do:` ever ran; `[| [10 / 0]]` defers it.

`ifError:` and `ifNotError:` receive values directly rather than deferring a
computation. On a non-error, `ifError:` passes the receiver through; on an error,
it answers the handler's result. `ifNotError:` is the exact dual. That already
supplies pipeline recovery without another `recover:` or `catch:` alias:

```
[input
  => parse
  => ifError: [:error | fallback]
  => validate]
```

If the handler recovers to a value, later legs continue with that value. If it
answers an error, propagation continues. Use `on:do:` when the error kind must be
selected at the deferred computation boundary, and `ensure:` when cleanup must
run regardless of the result.

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
