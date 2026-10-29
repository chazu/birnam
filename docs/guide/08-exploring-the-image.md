# Exploring the live image

The REPL is a live image, not just a calculator. When you encounter an object
you did not create, ask three separate questions:

1. How does this value render?
2. What prototype/vtable supplies its behavior?
3. What facts are attached to the value or its prototype?

Birnam does not provide a universal `inspect` or class browser. The `Reflect`
mirror now answers the structural questions directly, while the fact store
answers questions about an object's current data.

## Start with the REPL affordances

`:help` lists the commands understood by the console, and `:globals` lists the
user-visible bindings persisted by the current session:

```text
:help
:globals
```

Every value has `printString`; `printNl` prints that representation and answers
the receiver, which makes it useful inside a longer expression:

```birnam
let value := [Object clone];
[value printString];
[value printNl]
```

Generic objects normally print as `anObject`. That is expected: Birnam objects
are prototypes and instances, not class-tagged records with a universal display
name.

## Use the documentation surface first

The installed CLI is the most convenient reference browser:

```sh
birnam doc stdlib
birnam doc modules
birnam docgen /tmp/birnam-docs.html
```

The optional selector form is useful when looking for a known message:

```sh
birnam doc stdlib collect:
```

Selector lookup searches the documented registry, so a selector used by more
than one module can produce more than one entry. `birnam doc <module>` is the
better starting point when you are learning a module's whole surface.

When the `Doc` module is already present in an image, its query API is also
available:

```birnam
[Doc modules]
[Doc text: #stdlib]
[Doc lookup: "collect:"]
let value := [Object clone];
[Doc globalNameOf: value]
```

`Doc methodsOf:` returns documented method entries, not every method currently
dispatchable. A normal AOT session does not load `Doc` automatically; use the
CLI for an ordinary REPL unless the project loaded the documentation module as
part of its startup image.

## Ask what kind of value you have

`Reflect vtableOf:` is the low-level type question. It returns the vtable that
dispatches the value's messages:

```birnam
let value := "hello";
let valueVt := [Reflect vtableOf: value];
[Reflect isString: value]
```

For a prototype, pass the prototype itself:

```birnam
let stringVt := [Reflect vtableOf: String];
[Reflect vtableOf: "hello"] == stringVt
```

## Retain and invoke a method

`methodFor:` asks an object's current behavior for a selector and answers a
Method, or nil when it is absent. The Method retains that exact implementation:
replacing or removing the binding changes future lookup, while the retained
Method remains callable.

This includes Methods from synchronous and resumable compiled Blocks. A Method
keeps its compiled Unit alive after the original Block and Codegen cache release
it. Installing that Method under another selector also retains the Unit until
the binding is removed or its behavior becomes unreachable.

```birnam
let addition := [Integer methodFor: #+];
[addition invokeOn: 3 arguments: #[4]] // => 7
```

Use `[method invokeOn: receiver]` for a method with no arguments. An argument
sequence supplies `size` and zero-based `at:`; a Vector is the usual choice.
`[method clone]` answers the same Method.

Construct a Method from a compiled body without installing it:

```birnam
let method := [Reflect methodForArity: 1 implementation: [:self :argument | argument]];
[method invokeOn: Integer arguments: #[42]] // => 42
```

The body declares the receiver first, followed by the selector's arguments.
Construction checks this count and retains the body's lexical captures. Each
constructed Method shares the compiled code and enters it directly. Changing the
original Block's `value:` behavior does not alter the Method. Constructing it makes
no dictionary edit and needs no template cache. The receiving Reflect facade's
constructor remains replaceable, including on a delegated facade.

`Reflect defineMethod:on:as:` accepts such a Method directly or constructs one
from a compiled Block. An arbitrary object that merely answers `value:` is not a
method body. Use an explicit send in a compiled body when you want delegation to
another object's current behavior.

On a vtable, `methodFor:` searches that vtable's method dictionary and its
parents. `[vtable addMethod: method as: selector]` installs the retained entry;
it preserves the implementation and native context. Source-backed compiled bodies
provide image recovery data through their code owner. Raw native Methods need not
have portable source and do not automatically acquire an image recovery recipe.

Methods are process-local values. An image cannot persist a retained Method;
perform `methodFor:` again after boot. See [image saving](04-images.md#saving).

## Enumerate the effective method protocol

`Reflect allMethodsOf:` walks the receiver's vtable delegation chain and returns
one Map per effective selector. The nearest definition wins when a selector is
overridden. Each Map has these keys:

- `#selector` — the selector Symbol.
- `#target` — the source-level prototype name, such as `#String` or
  `#Collection`.
- `#owner` — the owning prototype, or nil for an abstract family such as
  `Collection`.
- `#method` — the store mirror, `#primitive` for a native binding, or
  `#compiled` for a raw compiled trampoline.

The result is ordered nearest-first, then in each vtable's direct method order:

```birnam
let methods := [Reflect allMethodsOf: String];
[methods size];
let method := [methods detect: [:entry |
  [[entry at: #selector] = #asUppercase]]];
[method at: #target];
[method at: #method]
```

`Reflect methodSource:` accepts one of these method Maps. It returns the
recorded Birnam definition span, or nil for a native or undocumented method:

```birnam
[Reflect methodSource: method]
```

The source lookup is lazy, so enumerating a large protocol does not rescan every
module once per method. It searches recorded compiler/source metadata, which makes it useful for understanding a method's implementation as
well as its dispatch shape.

Use the actual dictionary owner for direct methods, without Store:

```birnam
[Reflect directMethodsOf: String]
```

An optional relational description is separate data:

```birnam
[Module load: #relational];
[db backfillMirror];
let vt := [Reflect vtableOf: String];
[db findAll: ?selector where: #[ #[vt ?selector ?method] ]]
```

This is a descriptive snapshot, not method authority. Ordinary edits do not
maintain it automatically. Use current Reflect inspection for live methods; see
the [Store protocol](../../rust/birnam-store/README.md) for explicit observation.

## Inspect facts attached to an object

An ordinary object can also be an entity in an explicitly loaded Store. Facts
are additional data; ordinary slots remain discoverable through Reflect:

```birnam
[Module load: #relational];
let person := [Object clone];
[db assert: person attr: #name value: "Ada"];
[db assert: person attr: #city value: #Boston];

[db findAll: ?attribute where: #[ #[person ?attribute ?value] ]];
[db findAll: ?value where: #[ #[person #name ?value] ]];
[db pull: person spec: #[#name #city]]
```

The first query finds the attributes, the second finds values for a particular
attribute, and `pull:spec:` presents selected facts as a Map. For a nested object,
use the nested pull specification described in the [fact-store guide](03-fact-store.md).

For an object-focused snapshot, `Reflect factsOf:` returns one Map per current
live fact, with `#entity`, `#attribute`, and `#value` keys:

```birnam
let facts := [Reflect factsOf: person];
[facts collect: [:fact | [fact at: #attribute]]];
[facts collect: [:fact | [fact at: #value]]]
```

This is a liveness view: retracted facts and superseded `:one` values are not
returned. Use the `db` queries when you need joins, projections, pull specs, or
transaction-window reads.

There is an important distinction:

- `db attrCount: person` enumerates facts attached to that object.
- `Reflect directMethodsOf: person` inspects its direct dictionary.
- Facts on a vtable exist only when explicitly described; their count is not the
  live method count.

A freshly cloned object may have no application facts even though it inherits a
large protocol from its prototype.

## Find source and method explanations

Prefer the documentation tools for source-level questions:

```sh
birnam doc stdlib
birnam doc compiledUnit
```

The source-backed HTML reference includes Birnam module docs, production Rust
floor documentation:

```sh
birnam docgen /tmp/birnam-docs.html
open /tmp/birnam-docs.html
```

The fact store retains reflective data such as method and module-source mirrors,
but those are implementation-level records. `Doc`, `birnam doc`, and the LSP
hover surface are the stable human-facing queries over that data.

## A compact exploration recipe

When handed an unknown `value`, this is a good sequence:

```birnam
[value printString]
let methods := [Reflect allMethodsOf: value];
let facts := [Reflect factsOf: value];
[methods size]
[facts size]
```

Then inspect a method's `#selector` and use `Reflect methodSource:` when the
source is available. Use `birnam doc <module>` or `birnam docgen` for the
human-facing explanations, and the `db` query surface when you need a more
relational view of the same facts.


## Loading native implementation modules

`[System loadModule: path]` loads a native module through Id's descriptor API
with no constructor arguments. Use `[System loadModule: path arguments: inputs]`
to pass a sequence of explicit Objects to its initializer. `inputs` supplies
`size` and zero-based `at:`; a Vector is the usual choice. The native initializer
returns an Object, or Unit to return the receiving System object. It must export
`birnam_native_module_v1`; the former `module_init` entry is no longer loaded.
See [native loading](native-loading.md) for the native contract and errors,
and [Native Loader](../design/authored-small-modules.md#native-loader) for the current
message-based integration and validation.


## Loading source files

`[System load: path]` reads and evaluates a Birnam source file through Codegen.
Top-level bindings remain in Globals. Source declarations are registered before
evaluation; registration and successful Module loaded-state notification are
separate operations. See [Source Loader](native-loading.md#loading-source-files)
for the preserved errors and registration rules.

This path now runs in the migrated native prefix, including compile-time binary
embedding:

```birnam
let data := [System embedFile: "assets/data.bin"];
[data size];
[data at: 0];
[data clone];
[data asString]
```

`data` is a Bytes object. The literal path is relative to the loaded source file;
compiled code retains the payload after that file is removed. `["abc" asBytes]`
also constructs Bytes. See [embedding](../design/56-embedded-files.md) and the
[Bytes protocol](../../rust/birnam-string/README.md#bytes-prototype). Full CLI
startup, project artifact loading and image round-trips remain separate migrations.
