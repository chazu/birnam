# First milestone: authored PEG on the Odin seed

Draft implementation contract, 4 November 2026. The milestone direction is agreed; the detailed subset and invocation proposal below are now concrete for review. No runtime, authored PEG implementation, or passing acceptance result exists in this checkout yet. [Architecture](../ARCHITECTURE.md) governs this work.

## Outcome and boundary

From a clean checkout, build an Odin seed and run a Birnam-authored PEG module that constructs grammars and parses input. Grammar matching, combinator behavior, rule recursion, and concrete syntax tree construction execute as Birnam code through ordinary Id messages. Native primitives supply general object/data/file operations, not a hidden PEG engine.

This is a single-threaded interpreter milestone. It does not deliver compiled Birnam, full Lang, fibers, image saving/loading, optimized code, native module packaging, or a performance claim. It must not invoke, link, or generate artifacts through the old Rust system or an installed Birnam.

## Reference inspection

The following sources were read in `/Users/chazu/dev/zig/birnam`; that checkout is evidence, not a build dependency:

| Source | Useful evidence | What does not carry over |
|---|---|---|
| `docs/guide/01-language.md` | Explicit bracketed sends, bindings, literals, lexical blocks, message-based control, Error values | Store-only mutation advice, old stackless loop machinery, broad claims unsupported by a seed |
| `modules/lang.birnam` | Authored grammar construction through `Peg`, public `Lang parse:`/`grammar`/`furthestFail`, byte-span CST consumer expectations | Full production grammar as a mandatory M1 feature set |
| `modules/lang-test.birnam` | Optional separators, block captures, failure recognition, later result-threaded-send behavior | Old Codegen, artifact, image, or installed-runtime harness |
| `docs/design/63-peg-behavior-freeze.md` | Ordered choice, byte spans, prefix matching, repetition progress, rule errors, message-visible CST | Zig commands, native arenas, root providers, representation and memoization implementation |
| `rust/birnam-peg/src/lib.rs` public selector declarations | Confirmed `seqOf:`, `choiceOf:`, `rule:is:`, `parse:from:`, `furthestFail` still exist | Rust implementation or algorithms as a bootstrap dependency |

The old tree has authored Lang over a native PEG implementation. This milestone writes PEG in Birnam; merely wrapping the old native matcher would not meet it. Source examples also differ across history, so the small public protocol below is the milestone reference rather than every old spelling.

## Seed syntax and evaluation

The seed supports a subset of the existing language, not a different bootstrap syntax. Unsupported forms produce an explicit diagnostic; they must not be silently accepted with another meaning.

| Form | M1 contract |
|---|---|
| Sends | `[receiver unary]`, `[receiver + argument]`, `[receiver at: key put: value]`; nested brackets express grouping. No operator precedence. `[x]` and redundant wrapping of a completed send are invalid. |
| Bindings | `let name := expression`; immutable lexical bindings inside blocks; top-level bindings remain available to later source loads through the seed namespace. No assignment syntax added. |
| Separation | Whitespace or optional `;`, including a trailing separator; `//` comments end at newline. |
| Literals | Signed decimal Integers, Strings with `\n`, `\r`, `\t`, `\"`, `\\`, interned Symbols including keyword selectors, and fresh persistent Vectors `#[expressions]`. |
| Blocks | `[| body]` and `[:x :y | body]`; lexical captures, nested blocks, local lets, last-expression result. Empty body answers canonical nil. Support ordinary `value`/keyword `value:` forms needed by the authored module, including the existing zero-to-four argument block surface. |
| Names | Lexical lookup then the current source namespace. `true`, `false`, and `nil` resolve to canonical objects. Namespace operations are messages, not a new qualifier syntax. |
| Control | Boolean branch selection and block application are messages; only the selected deferred branch runs. The evaluator does not hardwire selector meanings for `ifTrue:ifFalse:` or arithmetic. |
| Order | Evaluate receiver and argument expressions once, in source order, then send. Vector elements similarly evaluate in order. An evaluation sequence returns its last result. |
| Errors | Runtime failures answer kinded Error objects; source syntax failures include location. A discarded Error aborts the source unit at that form, with previous effects retained. Error protocol operations allow inspecting/recovering the value; ordinary sends to an Error propagate it. No native exception crosses Id. |

For the seed, propose signed 64-bit Integer storage with checked overflow reported as an Error. This matches the inspected guide's width but is a milestone choice, not a new permanent numeric invariant. A minus sign belongs to a decimal literal only when followed by digits; binary subtraction remains a message.

Floats, map literals, relational variables, result-threaded `=>` sends, and the complete production syntax are deferred. A Map can still be constructed through messages for grammar/rule tables. Deferral does not remove these forms from Birnam; the authored Lang milestone must handle the wider existing syntax. Full Unicode character semantics are also deferred: PEG and source spans operate on bytes.

## Minimum object/library surface

Use ordinary objects for persistent syntax trees, lexical environments, closures, Methods, parser patterns, and results. Private native tokenization scratch structures may be implementation data, but a fixed native AST/value tag switch must not become the authoritative object model or bypass replaceable evaluation behavior.

The seed supplies only general operations needed to author PEG:

- Object cloning, behavior access, identity comparison, and object-owned mutable slots; method construction/installation through Reflect and vtables. `Reflect defineMethod:on:as:` follows the existing receiver-first block convention (`:self` followed by message arguments).
- Canonical Boolean/nil behavior; Block evaluation and captures; Error construction, `isError`, and `kind`.
- Integer comparison and checked arithmetic; Strings/Bytes with size, byte access, and bulk slicing/equality; persistent Vector and Map construction/access/update. No per-byte messaging requirement for bulk literal matching.
- Namespace lookup/publication and sequential source loading. Recognize existing `[Module named: #peg requires: #[]]` and `markLoaded:` through objects/messages if used; M1 is source loading, not the finished dependency/package system.
- File input, basic printing, process success/failure, and an explicit GC collection message for acceptance.

Reuse public selectors already demonstrated by Lang where applicable. The exact general-purpose collection and slot selector list must be inventoried from the newly authored PEG source as it is written; no unused native library should be built preemptively. Mutable parser state belongs to objects rather than reassignable lexical variables or a required Store database.

## Authored PEG contract

Export `Peg` using the existing spelling. The module implements the following through Birnam methods:

| Surface | Meaning |
|---|---|
| `Peg grammar` | New independent grammar/rule table and diagnostic state. |
| `literal:`, `any`, `eof`, `charset`, `ruleRef:` | Pattern constructors; literals match exact bytes, `any` consumes one byte, EOF consumes none. |
| `seqOf:`, `choiceOf:` | Sequence and ordered choice from a Vector of patterns. |
| Pattern `from:to:` | Add an inclusive byte range to a charset. |
| Pattern `star`, `plus`, `opt`, `negate`, `lookahead` | Repetition, optionality, negative and positive lookahead; these spellings are confirmed by the inspected public method declarations. |
| Grammar `rule:is:` | Define a named rule; String rule names resolve to the same interned identity as Symbols. |
| Grammar `parse:from:` | Prefix parse from a named root rule; include `eof` explicitly to require full consumption. |
| Grammar `furthestFail` | Per-grammar byte offset from its most recent failed parse. |
| CST `rule`, `ruleName`, `start`, `end`, `childCount`, `childAt:`, `children` | Ordinary messages exposing labelled rule nodes with half-open byte spans and source-ordered children. |

A successful root rule produces a CST root. Only rule applications create nodes; sequences/repetitions flatten child rule nodes. Ordered choice starts each alternative at the original position. Failed alternatives must not leave successful CST children behind. Lookahead consumes no input and contributes no children. Successful zero-width repetition terminates rather than looping forever.

An ordinary mismatch answers a `syntaxError` Error; an undefined root/reference answers `undefinedRule`; re-entering a rule at the same input position answers `leftRecursion`. Do not report structural grammar failures as ordinary mismatches that choice can silently recover from. Include Error support in the seed so the old no-Error-rung nil fallback is unnecessary. Memoization, if introduced, stays authored and scoped appropriately; its storage algorithm is not a required native mechanism.

## Intended acceptance program

This is an executable target for the milestone, not code claimed to run today:

```birnam
let grammar := [Peg grammar];
let letters := [[Peg charset] from: 97 to: 122];
[grammar rule: #word is: [letters plus]];
[grammar rule: #root is:
  [Peg seqOf: #[[Peg ruleRef: #word] [Peg eof]]]];
let parsed := [grammar parse: "abc" from: #root];
#[[parsed rule] [parsed start] [parsed end]
  [[parsed childAt: 0] rule]]
```

Expected observations are `#root`, `0`, `3`, and `#word`. A second parse of `"abc!"` must answer `syntaxError` because the root requires EOF. These observations are checked through messages, not native CST inspection.

## Delivery slices and evidence

1. **Object floor and entry:** implement the reviewed invocation contract, same-universe Methods and data, and a minimal C caller demonstrating an Oop-only boundary without Odin context knowledge. Verify replacement and captured/entered Method lifetime before adding an evaluator.
2. **Seed evaluator:** evaluate representative source forms, captures surviving return, message-based branches, and source failures. Replace a method and observe the next send change; check that native semantic type switches or implicit application have not been introduced.
3. **Authored PEG:** implement the public protocol in Birnam, load it from source, and run successful/failed grammar fixtures. No native parse/match fallback is allowed.
4. **Qualification:** exercise collection with live grammar, CST, closure captures, and Method objects; test fresh-parse independence and reclamation over repeated bounded workloads. Preserve an actual build/test receipt and dependency inventory.

Acceptance includes nested rule spans; prefix versus EOF matching; ordered-choice backtracking; zero-width star/plus; lookahead without CST leakage; undefined rules; direct/indirect left recursion; a deep finite rule chain; and independent diagnostics from two grammars. Replace an authored matching method and demonstrate that later parsing observes it. Memoization must not make results stale across separate parses after replacement.

GC validation must distinguish survival from leaking everything: add a bounded reclamation check accounting for conservative retention. Use the collector's own supported observations or finalization instrumentation only in the harness. The proposed Boehm baseline is not qualified merely because allocation works. Fiber, incremental-mode, and Alien tests from the collector research remain later obligations.

A clean build and test must work without access to the old checkout, Cargo, an installed Birnam, or generated artifacts from the Rust tree. Record the exact Odin/C/collector versions and commands actually used. Do not invent a passing gate or require obsolete Zig/Rust acceptance commands.

## Review boundary before implementation

The [invocation proposal](seed-invocation.md) deliberately exposes a few decisions: the uniform argument carrier, exact send/bind placement, and the native recursion base case. They must be resolved before implementing dependent dispatch/evaluator code. The seed Integer choice is also a proposal. The first code slice should be bounded by these contracts rather than a full runtime port.
