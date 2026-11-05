# Authored PEG

Birnam's first PEG module is [modules/peg.birnam](../modules/peg.birnam). Grammar matching, combinators, recursion checks, diagnostics, and CST construction are Birnam Methods running through the loadable seed interpreter. Native code supplies general storage, control, arithmetic, and byte operations. There is no native PEG matcher.

```sh
make build
./build/siphon ./build/seed.dylib --eval modules/peg.birnam --eval examples/peg/word.birnam
make verify
```

The example prints `#root`, `0`, `3`, and `#word`. Loading the source publishes `Peg` into the seed namespace; its helper prototypes remain lexical captures. The files are loaded explicitly in dependency order. This is not yet the source module dependency/packaging system. Evaluating the module again publishes a new `Peg`; it does not reconcile identities or update existing grammars as a coordinated reload would.

## Public messages

| Receiver | Messages |
|---|---|
| `Peg` | `grammar`, `newGrammar`; `literal:`, `any`, `anyByte`, `eof`, `charset`, `anyOf:`, `range:to:`, `ruleRef:` |
| `Peg` | `seq`, `sequence`, `choice`, `seqOf:`, `choiceOf:`, `star:`, `plus:`, `opt:`, `and:`, `not:` |
| Pattern | `star`, `plus`, `opt`, `negate`, `lookahead` |
| Sequence / choice | `add:` appends a child and returns the receiver |
| Charset | `from:to:` adds an inclusive Integer byte range, from 0 through 255 |
| Grammar | `rule:is:`, `parse:from:`, `furthestFail` |
| CST | `rule`, `ruleName`, `start`, `end`, `children`, `childCount`, `childAt:` |
| `Peg` inspection | `patterns` returns a namespace of concrete pattern prototypes; `cst` returns the CST prototype |

String rule names and Symbols resolve to the same interned identity. `range:to:` takes one-byte String/Bytes endpoints; `anyOf:` constructs a charset containing each supplied byte. Strings use UTF-8, while Bytes can contain arbitrary data. Positions and spans always count bytes.

`parse:from:` accepts a prefix. Include `eof` in the grammar to require full consumption. Only named rule applications create CST nodes. Each node has a Symbol label, a half-open span, and ordered children; `ruleName` returns the label as a String. Sequence and repetition flatten child nodes. Failed alternatives discard their tentative children, and lookahead contributes none.

Ordered choice selects the first successful alternative. A later sequence failure does not reopen that choice. Optionality converts an ordinary mismatch into empty success. Repetition stops at a mismatch or zero-width success. A zero-width success contributes its children once before stopping; `plus` requires at least one advancing success, so an exclusively zero-width body fails.

An ordinary mismatch returns `syntaxError` with a line and byte column. Missing rules return `undefinedRule`; re-entering the same rule at the same position returns `leftRecursion`. Structural Errors propagate through every combinator, including negative lookahead, rather than becoming recoverable mismatches. Advancing recursive rules are allowed.

`furthestFail` is the furthest recorded failure offset in the latest completed parse, initially zero. Successful backtracking, repetition, or lookahead can also record failures. Each parse starts at zero, and each grammar owns its published diagnostic. This clarifies the earlier milestone wording “most recent failed parse”: successful completion also publishes the current parse's diagnostic, consistent with the inspected historical public implementation.

## Matching and replacement

Every pattern responds to `match:at:` with a parse context and Integer position. Its result is canonical nil for mismatch, an Error for a structural failure, or `#[endByteOffset children]` for success. Custom patterns must honor that protocol, including valid source positions and a Vector of CST children. The current module does not validate arbitrary malformed custom results.

The context supports `source`, `failureAt:`, `furthestFail`, and `rule:at:`. Calling `failureAt:` records a maximum and returns nil. Calling `rule:at:` performs the recursion check, sends the selected pattern `match:at:`, and wraps success in a labelled CST node. Parse contexts and active-rule vectors belong to individual invocations, including nested parses of the same grammar.

At parse entry, the grammar snapshots its rule bindings by shallow cloning the rule namespace. Rebinding a rule during parsing affects the next parse. Patterns retain their identities and ordinary live Methods: a Method edit can affect the next matching send even within the current parse. Local Method edits affect the target; explicit edits to a shared behavior affect inheritors. An explicitly captured Method retains its selected implementation.

There is no memoization. Repeated attempts at the same rule/position send again and can observe effects or replacement. This avoids introducing a cache policy before we have specified its interaction with malleability. It also means there is no packrat or linear-time guarantee.

## Implementation and limits

The module uses Namespace objects for grammar, pattern, context, and node storage, Cells for loop state, and persistent Vectors for ranges, children, and active rules. Methods are authored with `Reflect defineMethod:on:as:` and remain inspectable as interpreted Methods. Bulk literal comparison uses the general `commonPrefixLengthWith:at:` byte operation, rather than a native parsing primitive or one send per byte.

This first interpreter implementation favors explicit behavior over speed. Persistent Vector append/concatenation copies storage; active recursion checks scan the active rule vector; recursive rules use the native call stack. The tests qualify a 128-link finite chain, not arbitrary depth, large inputs, throughput, or bounded complexity. There is no performance claim.

The module satisfies the authored PEG milestone on the pinned Mac toolchain. Full Lang, the compiler, native code generation, fibers, images, and dependency-aware module loading remain later work. See [the validation receipt](peg-validation.md) for the exact tests and build evidence.
