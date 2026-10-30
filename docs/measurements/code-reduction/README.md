# Code reduction after the third optimization round

Implemented review items 1–5, 7 and 8. The separate investigation of test-only AST
lowering was not part of this implementation; its tests and reference execution
code remain. Id's interface and object representations are unchanged.

Relative to `02b08ab6`, the implementation commits remove **938 Rust lines and
32 Birnam lines net**, including new tests and benchmark support. These are
physical source-line differences, not SLOC measurements; documentation is
excluded. Birnam's net reduction is smaller than the review estimate because
Map literals mainly remove repetitive syntax, and regression tests add lines.

## Changes and architectural review

- Six duplicate literal tables (788 entries, 800 lines) now derive from the same
  ordered names at compile time. Each original table was compared against its
  names before replacement. The native descriptor layout and order are identical.
- Removed the unused `lower_named_adapter` generator. No repository caller used
  it; production sends still enter the selected Method directly.
- Replaced 49 LSP Map-construction chains with literals. The compiler lowers both
  forms to `Map new` followed by ordered `at:put:` sends. No record factory or
  additional runtime dispatch was added.
- Native I/O modules reuse root-scope cleanup, initializer guards and root-provider
  installation code. Each module retains its own state and temporary references.
  Installation performs the same seven ordered sends; scope cleanup restores
  the prior root-list length on return, error and unwind. No shared service,
  object registry or private object representation was introduced.
- Project records share ordered Vector construction. The authored-policy and
  command formats retain their different keys, String/Bytes choices, defaults
  and validation errors. Decoder cleanup removes ignored description arguments
  and repeated list admission, not validation. Current collection messages and
  constructor ordering remain; native temporary scopes retain the current value.
- JSON and TOML share only the escaped-body scanner. Their separate conversion
  methods remain live and independently replaceable. Plain-string scanning and
  copying are unchanged and do not construct a conversion callback.
- ProjectPolicy searches use one field-based traversal, preserving each public
  entry point, first-match identity, starting index and nil-at-end behavior.

## Performance tradeoff

The [escape-heavy comparison](escaped-text.json) uses bodies made entirely of
backslash-n pairs on the same installed runtime. Five samples per size include
runtime allocation/automatic collection but exclude setup and explicit pre-sample
GC. Output bytes are checked. The shared scanner is **1.4–3.5% slower** in these
cases because it calls the supplied conversion block. This is a measured cost
of removing duplication, not a speedup claim. The plain path is unchanged.

The comparison is reproducible with the `round2_language json --dense` example,
`BIRNAM_JSON_SOURCE` pointing at the before/after definition, and
`BIRNAM_JSON_PRELUDE` supplying the new String helper when using the older runtime.

## Validation

The clean implementation snapshot `d23eceed` built and installed successfully at
`/tmp/birnam-reduction-final`. The usual local installation was not replaced.
The [validation record](validation.json) contains the source digest and commands.

- Workspace library tests: 316 passed, 30 explicitly ignored; all-target checks passed.
- Authored JSON and ProjectPolicy checks: 135 assertions passed on the fresh install.
- Thirteen installed-runtime tests passed, covering native I/O, GC, compiler
  execution, direct Methods, conversion, scheduler, TCC contexts, portable code,
  command records and image restoration.
- Full project command checks passed, including human/JSON output, builds, runs,
  installs, tests, dependencies, relocation and incomplete-native rejection.
- LSP workspace/diagnostics/UTF-16/version checks and owned-state, retention and
  source-free image restoration passed.

New checks cover nested root cleanup after errors/unwind, denied initializer
reentry, collection and failure inside replaced Vector operations, independent
JSON/TOML conversion replacement, and search starting indices/first matches.
Existing direct dispatch, Method-construction purity, callback, image and
validation checks remain in place.
