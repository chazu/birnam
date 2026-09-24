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

Language values cross these operations as Objects; indices and offsets are
Integers. Parse failures retain syntaxError, undefinedRule and leftRecursion.
`mise exec -- cargo test --offline -p birnam-peg --lib` runs the three retained
independent matcher comparisons; production parsing and precise-GC checks run in
`mise run native-messages`.
