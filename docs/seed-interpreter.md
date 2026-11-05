# Loadable seed interpreter

Implemented 5 November 2026. This slice adds `build/seed.dylib`, an Odin native module that evaluates the initial Birnam source subset in the siphon's existing Id world. It is the direct evaluator intended to bootstrap authored PEG, Lang, and the compiler. It is not a native Birnam compiler or a CBPV implementation.

## Running source

```sh
make build
./build/siphon ./build/seed.dylib --eval examples/seed/answer.birnam
make verify
```

The example prints `42`:

```birnam
let object := [Object clone];
let body := [[:n | [:self | n]] value: 42];
[Reflect defineMethod: #answer on: object as: body];
[GC collect];
[System println: [object answer]];
```

Native modules and `--eval` source files are processed in command-line order. The seed module must be loaded explicitly. Several source files can use the same namespace in one process. The CLI does not print returned values automatically; output is an explicit message such as `System println:`. A final Error is reported with a failing exit status. The siphon only locates `Seed` in its namespace and sends `evalFile:`; parsing and evaluation stay in the module.

The build uses the existing pinned Mac/Odin/Clang/Boehm configuration. `make build` builds both the host and seed module; `make verify` adds native and source acceptance. No previous Rust implementation or installed Birnam is used.

## Source organization

| File | Responsibility |
|---|---|
| [abi.odin](../src/seed/abi.odin) | Imports the Oop-only host ABI; supplies native construction/call adapters; registers module roots; keeps parser scratch references in GC-managed lists. |
| [module.odin](../src/seed/module.odin) | Installs the module's behaviors/exports and implements the `Seed` parsing, evaluation, and file-entry messages. |
| [parser.odin](../src/seed/parser.odin) | Small bootstrap lexer/parser, UTF-8/source diagnostics, and construction of syntax objects. Token enums exist only in this parser. |
| [syntax.odin](../src/seed/syntax.odin) | Separate ordinary behaviors for literals, names, declarations, sends, Blocks, Vector expressions, and sequences; each implements `evalIn:`. |
| [environment.odin](../src/seed/environment.odin) | Lexical environments, Block captures/calls, receiver-first interpreted Methods, and reflection operations. |
| [values.odin](../src/seed/values.odin) | Native Integer, String, byte/Vector operations, Boolean control, Error recovery, and printing Methods. |

The module links against the C ABI exported by the host, not against a second Id runtime. Its native entries have the same Method/receiver/argument-Vector Oop signature as the C fixtures. Odin context is established inside native entry code where Odin library routines need it.

## Evaluation is object behavior

The parser produces an object graph with distinct vtables for each syntax family. Evaluation sends `evalIn:` to a node. There is no central runtime switch over native expression or value tags. Each selected syntax Method understands its own payload and evaluates child objects through messages.

`Seed eval:in:` sends `parse:` to the current Seed receiver, creates a lexical environment, then sends `evalIn:` to the returned syntax object. Replacing `Seed parse:` therefore changes evaluation. Replacing a syntax family's `evalIn:` also changes later evaluation; both paths have acceptance witnesses.

Syntax fields are inspectable through owner-provided methods. `Seed syntax` answers a namespace of syntax vtables. A sequence exposes `forms` and `source`; a send exposes `receiver`, `selector`, and `arguments`; a literal exposes `factory` and `textBytes`; Block syntax exposes `parameters` and `body`. Other node fields have corresponding getters. Sequences retain the original source Bytes, shared by nested bodies. Detailed runtime source spans and a debugger are not yet supplied.

Literal evaluation uses current `fromBytes:` behavior on the retained Integer/String/Symbol factory objects. Block construction uses current `code:environment:parameters:` behavior; Vector literal construction uses current `fromElements:` behavior. Native transport vectors and parser scratch objects are separate construction machinery. Arithmetic, Boolean branches, and Block `value` operations are ordinary sends, and tests replace those Methods to rule out evaluator shortcuts.

## Language behavior in this slice

Supported forms are bracketed unary/binary/keyword sends, immutable `let`, signed decimal Integers, UTF-8 Strings, Symbols, fresh Vector literals, lexical Blocks, optional semicolons, and `//` comments. Identifiers use the seed's ASCII identifier grammar. Float/map/relational/result-threaded forms and unsupported numeric prefixes/exponents are rejected explicitly. There is no implicit operator precedence or redundant grouping bracket syntax.

Receiver and argument expressions evaluate once, receiver first and then arguments left to right. Binding occurs after argument evaluation. An argument that changes the pending method changes the eventual selection. Vector elements also evaluate in order.

Block invocation supplies a fresh local environment whose parent is the captured environment. Captured locals keep their object references after the originating call returns. Free names resolve through the defining namespace's current bindings, even when invocation occurs from another namespace. Parameters and locals shadow outer bindings. Duplicate parameter names and redefinition in the same lexical scope are errors.

Top-level declarations publish into the current namespace. A new source unit may replace an existing namespace binding; duplicate declarations within that unit are rejected. A declaration returns nil. Storing an Error in a binding retains it for inspection and is not treated as discarding it. A non-final expression that produces an unhandled Error stops its enclosing evaluation sequence; callers can recover from the returned Error explicitly. A final Error is returned as a value. Runtime failures preserve earlier effects.

The entire source unit is parsed before its expressions execute, so a syntax failure runs none of those source expressions. Parsing itself is replaceable behavior and is not promised to be effect-free.

Integers are boxed signed 64-bit payloads. Arithmetic is checked; overflow and division by zero produce kinded Errors. Division truncates toward zero and remainder follows that quotient convention. Literal range diagnostics accept the signed minimum. Arithmetic operands are converted through `asInteger`; native Integer code then validates its owned representation. These are primitive-owner checks, not a generic Id value classifier.

Strings represent valid UTF-8. Bytes can hold arbitrary byte sequences. `byteSize`, `byteAt:`, and `bytesFrom:to:` explicitly operate in bytes; the slice is half-open and returns Bytes. Converting an invalid byte slice to String returns `invalidUtf8`. Character/grapheme indexing and broader Unicode identifier syntax are deferred.

The parser currently limits nested source expressions to 512 and reports `sourceTooDeep`; it does not impose a language-wide limit on future authored parsing. There is no evaluator recursion/fiber scheduler in this slice beyond ordinary native execution.

## Native primitives exposed through messages

| Receiver | Implemented surface |
|---|---|
| Seed | `parse:`, `eval:`, `eval:in:`, `evalFile:`, `syntax` |
| Environment | `forNamespace:`, `lookup:`, `define:as:`, `namespace`, `parent`, `names` |
| Reflect | `methodFrom:`, `methodFor:on:`, `defineMethod:on:as:` |
| Method | Explicit `valueOn:with:` for an already-selected Method; interpreted Methods additionally expose `code`, `environment`, and `parameters` through their own behavior |
| Block | `value` through four keyword arguments, `valueWithArguments:`, `whileTrue:`, `code`, `environment`, `parameters`, and the construction message |
| Integer | `fromBytes:`, `asInteger`, checked `+ - * / %`, `= < > <= >=`, `printString` |
| String | `fromBytes:`, `asBytes`, `intern`, `printString`, concatenation `,`, equality `=`, and byte operations including `commonPrefixLengthWith:at:` |
| Bytes | `asString`, `printString`, and byte operations, in addition to native interning/loading support |
| Vector | `fromElements:`, `size`, `at:`, persistent `at:put:`, `append:`, `concat:` |
| Cell | `with:`, `value`, `value:`; shallow cloning keeps independent own storage |
| Boolean objects | `ifTrue:ifFalse:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, `not`, `printString` |
| Error | Explicit `ifError:`, `ifNotError:`, and `printString` in addition to the native Error protocol |
| Object | Default conversion failures, `printString`, `isNil`, and success-side recovery/composition messages |
| System | `print:`, `println:` |

The core adds `Error kind:message:`, `Bytes asBytes`, empty `Namespace new`, and `at:ifAbsent:` for use by loaded modules. These do not introduce interpreter-specific structures into generic dispatch. The existing collector remains available as `GC collect`.

General collection/control support for authored parsing includes Cells, persistent Vector extension, short-circuit Boolean messages, and `Block whileTrue:`. The loop sends current `value` messages to its predicate and body, propagates Errors, and rejects non-Boolean conditions. `commonPrefixLengthWith:at:` compares a byte buffer with another buffer from a supplied offset and returns the matching byte count; it has no grammar, backtracking, or CST behavior. Their separate source acceptance is `tests/seed/collections.birnam`.

Defining a Method from a Block obtains the Block's code, parameters, and environment through messages. The resulting Method retains those references and binds its first parameter to the application receiver. Its native entry evaluates that captured code directly; it does not send an implicit `value`, `apply`, or `invoke` to the selected Method. Subsequent dictionary replacement changes later sends while captured Methods retain their selected implementation.

## Validation and remaining work

`make verify` retains the original native siphon checks and adds:

- The `42` example and a source corpus covering captures, namespace rebinding/isolation, overrides, Method retention, replacement during argument evaluation, branch/Block/arithmetic replacement, literal constructor replacement, syntax inspection, and current parser behavior.
- Twenty-six negative source cases plus ordering, file-loading, multi-file namespace persistence, and CLI misuse checks.
- C witnesses for effects before a failed source expression, effectful receiver-aware lookup, replaceable syntax evaluation, collection during parser interning, and bounded repeated creation/discarding of closures.

The implementation and qualification are recorded in [seed interpreter validation](seed-interpreter-validation.md).

Authored PEG/Lang/compiler, Map and broader collection libraries, source slot/accessor factories, source module declaration/dependency protocols, full language syntax, image persistence, native-code generation, CBPV, fibers, and complete Alien facilities remain later work. Loading the same seed library again in the same process returns the already-installed Seed; it is not rebuilt-code hot reload. This is the initial evaluator slice, not completion of the PEG milestone or a native-speed claim.
