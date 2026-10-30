# 63 — PEG behavior freeze

> **Scope:** PEG grammar and behavior reference retained from the completed
> port. Historical Zig and `IdApi` details are not implementation requirements.
> The current implementation is `rust/birnam-peg`; the
> [invariants](../../INVARIANTS.md) remain authoritative.

Patterns and grammars are objects. Parsing does not require Store facts.

## Construction contract

`Peg` creates `Pattern` objects for `literal`, `any`, `eof`, `charset`,
`ruleRef:`, `seq`, and `choice`. Patterns compose through `add:`,
`from:to:`, and the unary operators. The ergonomic forms are
`seqOf:`, `choiceOf:`, `star:`, `plus:`, `opt:`, `not:`, `and:`, `anyOf:`, and
`range:to:`. A `Grammar` defines named rules with `rule:is:` and parses with
`parse:from:`. Rule names supplied as Strings are interned and are identical to
the corresponding Symbols.

## Operator semantics

| Pattern | Match and consumption | CST effect |
|---|---|---|
| literal | Exact byte sequence; advances by its byte length | None |
| any | One byte, failing at end of input | None |
| eof | Succeeds only at `source.len` | None |
| charset | One byte in any inclusive range | None |
| seq | All children in order; failure restores the caller's path | Flattens child rule nodes |
| choice | First succeeding child at the same input position | Child nodes of selected branch |
| star | Repeats until failure; zero matches allowed | Flattens repeated child nodes |
| plus | Repeats until failure; at least one required | Flattens repeated child nodes |
| opt | Child or zero-width success | Child nodes if present |
| and | Positive lookahead; child must succeed, position unchanged | None |
| not | Negative lookahead; child must fail, position unchanged | None |
| rule-ref | Runs the named rule with packrat memoization | One labelled node |

Repetition has a zero-width-success guard: a successful child that does not
advance input terminates the loop rather than spinning. `parse` is prefix
matching; callers that require complete input include `eof` in the root rule.

## CST contract

Every rule application creates one node, including the requested root. Nodes
carry the interned rule Symbol, a half-open `[start, end)` span measured in
source bytes, and children in source order. Terminals and combinators do not
create nodes of their own. The message-visible `CstNode` exposes `rule`,
`ruleName`, `start`, `end`, `childCount`, `childAt:`, and `children`.

The native match tree is allocated in a caller-owned arena. Message conversion
mints traced `CstNode` objects, protects partially converted children with a
temporary root provider, and frees the scratch tree after conversion. A Rust
port must not make the CST durable or hold a Rust borrow across a callback.

## Failure and diagnostics

The primitive Zig `parse` returns null on failure and records diagnostics on the
Grammar instance. The message `parse:from:` returns a kinded Error when the
Error rung is loaded, otherwise canonical `nil`:

| Kind | Condition | Message shape |
|---|---|---|
| `undefinedRule` | Rule reference or root has no definition | `undefined rule: <name>` |
| `leftRecursion` | A rule re-enters at the same input position | `left-recursive rule: <name>` |
| `syntaxError` | Ordinary match failure | `syntax error at line <n>, col <n>` |

`furthestFail` is the Grammar's per-instance byte offset reached by the most
recent failed parse. A diagnostic from one Grammar must not be overwritten by a
parse on another Grammar. A left-recursion or undefined-rule failure is not
reported as an ordinary syntax error.

## Frozen acceptance fixtures

`src/id/peg_test.zig` is the executable characterization suite. It covers
operator construction, ordered choice, `eof`, nested rule spans, message-only
CST walking, zero-width repetition, lookahead, undefined-rule and
left-recursion errors, and a 128-rule deep chain with one-byte half-open spans.
The `parse.json-20kb` benchmark in `bench/parse.birnam` remains the compile/
parse-corpus performance comparator; Rust PEG changes must report its result
against the current Design 46 baseline before changing any threshold.

The acceptance command for this freeze is:

```sh
zig build test-peg
```

It must pass on both selected IdApi floors. The full `zig build verify` gate
remains mandatory before the Rust PEG rung can replace the Zig implementation.
