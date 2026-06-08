# The PEG Parser and Language Frontend

How source text becomes a typed AST. `src/kernel/peg.zig` is a fact-shaped PEG
engine; `src/modules/lang/lang.zig` defines the Birnam grammar, desugars the
concrete syntax tree into a typed AST, and evaluates it; `src/modules/repl/`
provides the REPL front door.

> Implementation: `src/kernel/peg.zig`, `src/modules/lang/lang.zig`,
> `src/modules/repl/parser.zig`, `src/modules/repl/repl.zig`. Concept:
> [Reification](12-reification.md) — grammar and AST are facts. Pipeline overview:
> `DEMO.md`.

## The PEG engine — `peg.zig`

A Parsing Expression Grammar engine where **patterns and grammars are
fact-entities**, not in-memory structs. Each combinator is an entity with a
`:pattern-kind` keyword plus combinator-specific facts. The supported combinators
(the locked set of 12):

`literal`, `seq`, `choice`, `star`, `plus`, `opt`, `and` (lookahead), `not`,
`rule-ref`, `charset`, `any`, `eof`.

Constructors (`mkLiteral`, `mkSeq`, `mkChoice`, `mkStar`, …) each mint a fresh eid
and assert the pattern's facts. A grammar is an entity holding a linked list of
rule entities (`:grammar-first-rule` + `:rule-next`), each with a `:rule-name` and
a `:rule-pattern`.

`matchPattern` is a recursive-descent matcher with **packrat memoization** — a
per-parse `MemoMap` keyed `(pattern_eid, position)` makes it linear and
left-recursion-safe. On a successful rule-ref match it allocates a CST node
entity with `:ast-rule-name`, `:ast-byte-start`/`:ast-byte-end`, and a linked
child list (`:ast-first-child` + `:ast-next-sibling`). `parse(engine, grammar,
source, rule)` returns a `ParseOutcome` — either the root CST eid or an
`ERROR_VT` diagnostic entity carrying `:peg-position` and a `:peg-expected` list
built from the furthest-failure position.

The engine lives in the kernel; the lang/peg dylibs install a send-dispatched
façade (`parse:from:`, `defineRule:as:`, and the pattern constructors `literal:`,
`ruleRef:`, `star:`, …) on `GRAMMAR_VT`/`PEG_VT`.

## The language module — `lang.zig`

`lang.zig` is where Birnam-the-language is actually defined. The kernel knows
nothing of its syntax; this dylib builds it.

### Grammar

`buildGrammar` asserts the Birnam grammar by sending the PEG pattern constructors:
whitespace/comments, `digit`/`letter`/`identifier`, `integer`/`float`/`string`,
the keyword literals (`nil`/`true`/`false`/`self`), the bracket forms
(`bracket-unary`/`-binary`/`-keyword`, blocks), the `|>` pipe, vector/map
literals, `let`/assignment/return, and the statement list.

### Desugar: CST → typed AST

`desugar` walks the CST and produces a typed AST where each node has a per-kind
vtable so evaluation can dispatch on it:

- Literals → `AST_LITERAL_VT` (`:ast-value` holds the immediate).
- Identifiers → `AST_VAR_REF_VT`; `self` → `AST_SELF_VT`.
- The three send forms → `AST_UNARY_SEND_VT` / `AST_BINARY_SEND_VT` /
  `AST_KEYWORD_SEND_VT` (`:ast-receiver`, `:ast-selector`, `:ast-first-arg` +
  sibling chain).
- Blocks → `AST_BLOCK_VT` (`:ast-block-params`, `:ast-block-body`).
- `let`/`:=`/`^`/sequences → `AST_LET_VT` / `AST_ASSIGN_VT` / `AST_RETURN_VT` /
  `AST_STMT_SEQ_VT`.
- `|>` folds left into nested sends; `#[…]` folds into `with:` sends; `{…}` into
  `at:put:` sends; `let #[a b] := …` destructuring expands via gensym + `at:`.

### Evaluation: tree-walk top-level, stack VM for methods

Top-level forms are **tree-walked**: each AST vtable carries an `:eval` method
that takes the current scope Ref and recurses. The scope chain is fact-shaped —
`SCOPE_VT` entities with `:scope-parent`/`:scope-self`/`:scope-first-binding`, and
`BINDING_VT` entities chained by `:bind-next`. `TOP_SCOPE_EID` is the REPL scope;
`GLOBALS_SCOPE_EID` (its parent) holds built-in globals like `Module`.

User-*method* bodies, by contrast, are compiled to bytecode and run on the stack
VM (see [Compiler, Bytecode, and VM](27-compiler-vm.md)) — `method:do:` on
`VTABLE_VT` installs them. Primitives (SmallInt/Float arithmetic, Boolean
control flow, Closure iteration like `whileTrue:`/`to:do:`, Vector/Cell/String
methods) are installed directly on their vtables.

### Prototypes the module binds

`lang.zig` binds the global prototype objects: `Module` (FFI),
`db` (snapshots, retention, and the `cell:`/`ref:`/`atom:` constructors), and
`language` (the `fileIn:` family). See [Reference Cells](2a-cells.md) and
[Modules](2c-modules.md).

### File-in

`fileIn:` / `fileInSource:` split a source unit on `;` (honoring brackets,
strings, and comments via `formEnd`), then parse + eval each form. Atomic mode
aborts on the first error and only resolves pending FFI modules if every form
succeeded; the on-error variant continues. A filed-in unit is recorded as
`[unit :filed-in-path …] [unit :filed-in-source …]` so it stays re-emittable by
[freeze](2e-images-aot.md).

## The REPL — `src/modules/repl/`

`repl.zig` is the REPL object: a singleton with a `:vtable`, a `:target`, and a
`:prompt`. `handleLine` is the registered handler — it parses a line
(`parser.zig` is a tiny hand-written front end for the bare shell REPL; the lang
module installs the full PEG-based `replHandler` for real evaluation), dispatches
to the target, and prints the result via `printRef`. A `target: <eid>` line
retargets the REPL. The shell owns the loop and the `:`-commands; the handler is
just a function pointer the module registers at init.

## Where to go next

- What the compiled methods become: [Compiler, Bytecode, and VM](27-compiler-vm.md).
- Querying the facts the parser produced: [Datalog](29-datalog.md).
- Driving it: [Language Tour](31-language-tour.md) (Part 3).
