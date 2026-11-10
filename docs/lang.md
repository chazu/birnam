# Authored Lang

Lang owns Birnam's source grammar and uses the Birnam-authored PEG module. The first slice supplies [modules/lang.birnam](../modules/lang.birnam), which publishes `Lang` with `parse:`, `grammar`, and `furthestFail`. `parse:` accepts String/Bytes and returns a labelled CST covering the complete source, or an Error. Parsing validates UTF-8; source positions count bytes. Grammar matching uses ordinary PEG messages.

Load `runtime.dylib` and `seed.dylib`, then `modules/peg.birnam`, then `modules/lang.birnam` with successive `--eval` arguments. This is explicit source loading, not dependency resolution or coordinated module reload.

The initial grammar covers the existing seed subset: bracketed unary/binary/keyword sends, immutable declarations, signed decimal Integer literals, escaped Strings, Symbols, Vectors, lexical Blocks, comments, and optional semicolons. Identifiers use ASCII letters/underscore and subsequent digits. The grammar preserves the seed's token boundaries, including keyword colons, `:=`, and the distinction between binary minus and negative literals. Deferred float/map/relational/result-threaded forms and numeric prefixes/exponents answer `unsupportedSyntax`.

CST labels include `program`, `sequence`, `declaration`, `expression`, `identifier`, `integer`, `string`, `symbol`, `vector`, `block`, `parameter`, `send`, `unary`, `binary`, `operator`, `keywords`, and `keyword`. Token nodes exclude trailing whitespace; enclosing constructs can include consumed trivia. Sequences retain source order and omit separator/comment nodes. CST fields use PEG's normal messages. Integer range and duplicate Block parameters are semantic checks for the subsequent CST-to-code stage, not grammar recognition.

The grammar corpus exercises accepted/rejected forms, byte spans, and complete parsing of PEG's and Lang's own source. `tests/lang_grammar_test.py` runs these through the real host from an unrelated working directory. It is part of `make verify`. This interpreter workload is substantial: parsing both modules took about 29 seconds in the initial local run. That is an implementation observation, not a benchmark or performance qualification; PEG has no memoization and copies persistent collections.

The CST-to-executable-syntax and explicit parser takeover layer is described below. The first [authored C compiler](compiler.md) now consumes this front end. Full language compatibility, self-hosting, images, and fibers remain later work.

## Executable syntax and takeover

The second slice adds [modules/lang-syntax.birnam](../modules/lang-syntax.birnam). It adds `Lang code:` (parse and translate), `lower:source:` (translate a CST with its original source), `translator` (the visitor prototype), and `installOn:` (install the authored parser on a Seed object). Loading the module adds behavior without automatically switching parsers.

```sh
make build
./build/siphon ./build/runtime.dylib ./build/seed.dylib \
  --eval modules/peg.birnam \
  --eval modules/lang.birnam \
  --eval modules/lang-syntax.birnam \
  --eval examples/lang/takeover.birnam \
  --eval examples/seed/answer.birnam
```

This prints `42`. The takeover file sends `[Lang installOn: Seed]`. Later `Seed parse:` sends `code:` to the installed Lang object; that path calls Lang's current grammar and translator. The Odin parser remains necessary for initial bootstrapping. No automatic fallback exists in the authored path.

The visitor selects an ordinary Method using the CST's rule Symbol and enters it with the CST node. A fresh shallow clone holds the source for each translation; nested sequences retain that original source. Methods handle the individual syntax families, flatten expression wrappers, combine keyword selector pieces, decode String escapes, check signed-64-bit Integer bounds, and reject duplicate Block parameters. All syntax construction goes through the existing representation owner's messages. The visitor never interprets native payloads or calls the bootstrap parser.

The visitor obtains its syntax owner through its replaceable `syntax` message; the compiler supplies a separate authored IR owner through that protocol. Since 9 November 2026 the default `syntax` Method sends `[Seed syntax]` each time it is called instead of capturing the owner when the module loads. Lang therefore loads where `Seed` is unbound, as in the native toolchain. Replacing `Seed` or its `syntax` Method affects the next translation, and with no `Seed` bound the default answers the lookup Error. The default syntax namespace returned by `Seed syntax` provides:

| Message | Result |
|---|---|
| `literal:bytes:` | Literal retaining a factory and byte spelling |
| `name:` | Name reference |
| `declaration:expression:` | Binding declaration |
| `send:selector:arguments:` | Send with a Vector of argument syntax |
| `block:body:` | Block expression with a Vector of parameter Symbols |
| `vector:` | Vector expression with a Vector of element syntax |
| `sequence:source:` | Sequence with a Vector of forms and source Bytes |

These are replaceable native Methods in the seed, owning the existing syntax representations. Vector-bearing constructors reject incompatible containers. Literal factories still run during evaluation; authored Integer range checks do not construct the source literal at parse time. The authored parser itself executes ordinary Integer/String operations, which remain replaceable and can have observable effects. Arbitrary replacements can consequently disrupt parsing, just as they can disrupt other authored code.

The only additional text primitive is `utf8ErrorOffset` on String/Bytes: it returns the first invalid UTF-8 byte offset, or `-1` for valid text. Lang uses it for located invalid-source diagnostics. `furthestFail` reports PEG's latest completed parse diagnostic; UTF-8 rejection before matching and semantic errors during translation carry their own Error location instead.

Reloading source still creates new module objects. The acceptance test reloads PEG, Lang, and the translator in dependency order, then explicitly installs the newly loaded Lang on Seed. This proves authored bootstrapping, not identity-preserving module reload.

## Acceptance and remaining boundary

`tests/seed_test.py --authored` runs the existing source semantics and negative cases after takeover. `tests/lang_test.py` exercises translator/constructor replacement, GC during translation, complete source reload/reinstallation, and the seed/PEG semantic corpora. Its C guard replaces the original native parser Method entry with an Error-producing sentinel, checks that the sentinel works, and leaves it disabled for the acceptance run. Captured references to that original Method cannot quietly provide a fallback.

The C lifecycle witness registers finalizers on discarded syntax roots and lexical closures created through the authored parser, verifies bounded reclamation, and checks retained code and captures after collection. The exact results and fresh-build evidence are recorded in [Lang validation](lang-validation.md).

This supplies an authored front end for the seed subset and parser takeover. It does not deliver the full language grammar, a compiler/backend, native-speed parsing, packrat memoization, image persistence, fibers, or dependency-aware source loading. Both matching and translation still use the interpreter and native call stack. The native parser's 512-expression guard is not a new promise for the authored parser; arbitrary-depth behavior is unqualified.
