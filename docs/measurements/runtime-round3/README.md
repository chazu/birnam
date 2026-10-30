# Third runtime simplification round

The ten implementation commits address all findings from the preceding review.
Id's interface and payload opacity are unchanged. The measurements below are
operation costs, not additive estimates of whole-program speedup. Raw samples,
input sizes and the preceding revision are in the adjacent JSON files.

| Operation | Representative result | Scope |
| --- | --- | --- |
| PEG child access | 955 → 663 ns, **1.44×** | 4,096-child CST; 128-child case flat |
| C emission | 13.31 → 9.08 ms, **1.47×** | 256 KiB literal, including byte transfer |
| Liveness analysis | 8.01 → 0.566 ms, **14.2×** | 4,096 sparse-PC blocks |
| Name indexing and emission | 15.49 → 4.57 ms, **3.39×** | 4,096 distinct Symbol expressions |
| Nested admission reuse | **1.08×** emission, **1.12×** inventory | 128 nested blocks; hashing and emitted bytes still cost time |
| Project record construction | 55.04 → 60.87 ms, **0.90×** | 2,048 records; fewer old collection versions retained, but slower in this run |
| Plain TOML unescaping | 3.50 s → 0.113 ms | 4 KiB without escapes; now one bulk copy instead of per-character accumulation |
| Source composition | 3.28 s → 45.74 ms, **71.7×** | 256 ordered 4 KiB file parts; includes file reads |
| Process output | 85.93 → 14.36 ms, **5.98×** | 256 KiB stdout; includes shell startup |
| Precise GC | 13.72 → 14.15 ms, **0.97×** | 16,384 live empty objects; no demonstrated speedup |
| Signature reflection | 40.87 → 2.42 µs, **16.9×** | 1,024 adapters; captured Methods **17.1×** |
| Host command record roundtrip | 7.03 → 0.975 ms, **7.21×** | 256 KiB source, replacing JSON transport |
| Queue images | Previously rejected; now save and restore | Correctness fix; no comparable prior save timing |

## Startup

[Fresh-process startup](startup.json) was effectively unchanged: **4.561 s →
4.544 s**, a 0.4% difference within the observed variation. Five samples per
prefix used alternating order and one excluded warmup. Each launch restored the
installed startup image, compiled a small script and printed a checked marker.
Filesystem caches were warm; no source reload or concurrent benchmark/build/test
workload was used. This round does not demonstrate a startup speedup.

The baseline installed code is `fd90082a`; the review baseline `85776142` differs
only in its validation documentation. The new installed code is `33b3488a`.

## Changes and contracts

- PEG keeps immutable CST children in one shared native slice. Scalar reads do
  not clone that slice, and grammar failure-position reads do not copy rules.
- C byte initializers are formatted directly into the output. Compiler names
  retain first-occurrence order, while successor indexing preserves sparse PCs
  and the previous missing-target behavior.
- Portable program admission still checks all descendants and the same depth and
  aggregate-byte limits. Child views retain checked offsets into the same
  immutable bytes; lowering, emission and inventory construction reuse them.
  Wire formats are unchanged.
- Collection builders still send `new`, `add:` and `at:put:`. Their native scopes
  replace only the builder's own old accumulator reference after the send
  completes. Independently retained aliases and inputs remain live. This is
  native GC bookkeeping, not a Birnam ownership or borrowing requirement.
- TOML unescaping, source assembly and process stdout use StringBuilder. Empty
  inputs, escape meanings, source ordering, the first read error and stdout
  contents are preserved. Automatic allocation and GC costs are included in the
  timings; these synthetic cases previously produced many intermediate Strings.
- GC reuses cleared capacities, rebuilding the object list, address index and
  marks each time. No references, allocation identities, failures or marks are
  carried into the next collection. Epoch rejection and retirement order remain.
- Signature associations are weak, provider-local data. Known Methods and
  captures still verify the current context's declared Method. Unknown aliases
  retain the comparison path. Retirement removes dead keys and contexts.
- Host command records now cross messages as ordinary Maps, Vectors and whole
  Bytes values. Explicit Rust field projections replace JSON encoding/parsing;
  they share algorithm code, not hidden object state. Non-UTF-8 paths and
  arguments remain raw bytes. CLI/file JSON remains unchanged.
- Queue's ordinary ObjectModel pair nodes can be imaged. A native owner may
  explicitly approve an exported behavior's all-Oop slot layout with
  `imageAcceptsSlots:`. Both approval and ObjectModel's layout check are required;
  raw fields and unknown families without approval are rejected. This operation
  is above Id. Existing specialized image encodings retain their checks. See
  [the runtime contract](../../contracts/runtime.md#images-of-native-oop-slot-values).

## Reproduction

Rust examples `round3_costs`, `round3_builders`, `round3_gc`, `round3_signatures`,
`round3_transport`, and `round3_text` live in `birnam-siphon`.
`round3_compiler` and `round3_nested` live in `birnam-native-backend`.
The emission comparison uses `round2_runtime emit`.
Build with `cargo build --release -p PACKAGE --example EXAMPLE` and set
`BIRNAM_TEST_MODULE_DIRECTORY` to the matching native library directory.
The text example reloads the three definitions under test from the working tree;
its other runtime modules come from the same installed prefix for both runs.
Signature reflection uses a complete installed bootstrap. The pure compiler
examples need no runtime prefix.

Measurements use medians of five samples (GC: seven). Input construction and
explicit pre-sample collections are outside the timers unless stated otherwise.
These are local diagnostic measurements on Apple Silicon, not release thresholds.
The builder and GC results are recorded as regressions rather than speedups.

## Integrated validation

A clean detached snapshot at `33b3488a` built and installed successfully at
`/tmp/birnam-round3-final`; the user's usual installation was not replaced.
The [validation record](validation.json) identifies the source digest and checks.
Workspace checks covered all targets. Artifact, Codegen, native emission and PEG
unit tests passed, including rejected foreign/truncated child views.

Eleven focused tests passed against the fresh installation: image slot admission
and fresh-process restoration, installed scheduler reset/resume, Queue snapshots
and current node access, compiled execution, direct native Methods, native
boundaries, collection, activation, allocation accounting, command record
transport, and independently retained accumulator aliases. The image test ran
without an ImageFormat source override.

The installed text smoke test passed for escape meanings, empty input, first
file error and stopped reads, and stdout. The complete project command smoke
suite passed for human/JSON checks, build/run/install/test, path and pinned Git
dependencies, relocation with source removed, and rejection of incomplete native
files before running main. Signature construction purity was also tested against
the staged installed runtime with the current Execution library.
