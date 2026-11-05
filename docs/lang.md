# Authored Lang

Lang owns Birnam's source grammar and uses the Birnam-authored PEG module. The first slice supplies [modules/lang.birnam](../modules/lang.birnam), which publishes `Lang` with `parse:`, `grammar`, and `furthestFail`. `parse:` accepts String/Bytes and returns a labelled CST covering the complete source, or an Error. Parsing validates UTF-8; source positions count bytes. Grammar matching uses ordinary PEG messages.

Load `seed.dylib`, then `modules/peg.birnam`, then `modules/lang.birnam` with successive `--eval` arguments. This is explicit source loading, not dependency resolution or coordinated module reload.

The initial grammar covers the existing seed subset: bracketed unary/binary/keyword sends, immutable declarations, signed decimal Integer literals, escaped Strings, Symbols, Vectors, lexical Blocks, comments, and optional semicolons. Identifiers use ASCII letters/underscore and subsequent digits. The grammar preserves the seed's token boundaries, including keyword colons, `:=`, and the distinction between binary minus and negative literals. Deferred float/map/relational/result-threaded forms and numeric prefixes/exponents answer `unsupportedSyntax`.

CST labels include `program`, `sequence`, `declaration`, `expression`, `identifier`, `integer`, `string`, `symbol`, `vector`, `block`, `parameter`, `send`, `unary`, `binary`, `operator`, `keywords`, and `keyword`. Token nodes exclude trailing whitespace; enclosing constructs can include consumed trivia. Sequences retain source order and omit separator/comment nodes. CST fields use PEG's normal messages. Integer range and duplicate Block parameters are semantic checks for the subsequent CST-to-code stage, not grammar recognition.

The grammar corpus exercises accepted/rejected forms, byte spans, and complete parsing of PEG's and Lang's own source. `tests/lang_grammar_test.py` runs these through the real host from an unrelated working directory. It is part of `make verify`. This interpreter workload is substantial: parsing both modules took about 29 seconds in the initial local run. That is an implementation observation, not a benchmark or performance qualification; PEG has no memoization and copies persistent collections.

CST-to-executable-syntax translation and `Seed parse:` takeover are the next slice. Until then, source evaluation still uses the bootstrap Odin parser. Full language compatibility, compilation, images, and fibers remain later work.
