# PEG over Id

Bootstrap loads `libbirnam_peg` with Namespace after values and collections. It
publishes Peg, PegFacadeVt, PegPatternVt, PegGrammarVt and PegCstVt. Repeated loading
returns the existing Peg instance and preserves method replacements.

The ordered matcher, memoization, recursion diagnostics and CST spans are retained.
Patterns, Grammars and CST nodes are Id allocations with weak owner-local metadata.
GC obtains their actual references through `referencesOf:do:` and removes metadata
through `retire:`. Each parse copies only visited pattern data; no borrow spans
an owner message. CST construction roots intermediate nodes and snapshots across
callbacks. Grammar construction and parsing use live String/Integer/Vector/Error
messages; there is no IdApi table or foreign payload access.

The existing message vocabulary remains:

- Peg: `literal:`, `any`/`anyByte`, `eof`, `charset`, `ruleRef:`, `seq`, `choice`,
  `grammar`/`newGrammar`, `seqOf:`, `choiceOf:`, `star:`, `plus:`, `opt:`, `not:`,
  `and:`, `anyOf:`, `range:to:`.
- Pattern: `star`, `plus`, `opt`, `negate`, `lookahead`, `add:`, `from:to:`.
- Grammar: `rule:is:`, `parse:from:`, `furthestFail`.
- CST: `rule`, `ruleName`, `start`, `end`, `childCount`, `childAt:`, `children`.

Language values cross these operations as Oops; indices and offsets are
Integers. Parse failures retain syntaxError, undefinedRule and leftRecursion.
`mise exec -- cargo test --offline -p birnam-peg --lib` runs the three retained
independent matcher comparisons. `bash scripts/oop-id-slice-test.sh` also loads the
real numeric/text/collection owners and checks binary literals, composed grammars,
CST spans/children, syntax/undefined/recursive-rule errors, current byte methods,
allocation-time GC and retirement. Full compiler/source/image/worker integration
remains pending.

Lang grammar construction is authored in `modules/lang.birnam`. Bootstrap admits
its compiled artifact after PEG and Codegen; no Rust Lang library participates
in normal source parsing or image startup. The build-only Rust grammar seed is
accounted for in `rust/birnam-lang-seed`.

The v2 interface uses the Peg object as Method context and immutable native method
declarations. The old per-method leaked native Entry records are removed. An
owner-local scoped reference list replaces temporary root tokens and cleanup
messages. Allocation protects references in a newly constructed private Value
before any allocating callback and validates its zero-byte handle before storing
owner data. The Value classification belongs solely to PEG; Id never interprets it.

Source/literal text arrives through byte-provider application. Symbols are checked
and interned by their owner; diagnostic names use Symbol's byte application.
GC retirement receives dead Oops directly, with no packed-address byte buffer.
The matcher and its independent comparison tests remain unchanged.
