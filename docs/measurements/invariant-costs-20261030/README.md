# Removing costs inherited from earlier runtime interpretations

These are operation microbenchmarks, not startup/application speedups. Each JSON
file records the baseline revision, workload, raw samples and median. Release
builds were used. Setup and explicit collection are outside timing; automatic
collection and the stated result checks remain inside. Small changes should be
treated as noise-sensitive; the seven samples are not confidence intervals.

| Change | Representative result | Important limit |
| --- | --- | --- |
| Packed String construction | 1 MiB Host construction 114.0 → 3.65 ms (31.2×); builder conversion 56.35 → 0.868 ms (64.9×) | 16-byte operations became slower in this isolated first step: Host 4.78 → 6.42 µs, builder 1.77 → 2.03 µs. |
| String algorithms | 1 MiB replacement 37.41 → 22.05 µs (1.70×) | All-space trimming changed only about 3%; scanning still dominates. |
| Default call preparation | 100 compiled sends 412.0 → 361.6 µs (1.14×) | Includes the rest of Machine execution, not just frame copying. |
| Host reference scopes | 100 sends and scope cleanup 97.57 → 47.13 µs (2.07×) | Initial setup still needs token-based protection before its tracing callback exists. |
| Project records | 256 KiB source resolution 30.22 → 1.906 ms (15.86×) | File discovery/parsing is outside timing; the old path encoded a cached snapshot on each resolution. |

## Changes and preserved behavior

1. String construction uses packed byte providers. Host constructs Bytes in bounded
   groups rather than expanding a complete input to an Oop Sequence. The current
   String constructor still runs, including collecting replacements.
2. String trimming/replacement scan their own storage, then construct results after
   releasing internal access. Already-computed buffers are not copied again merely
   to call the constructor. Snapshots needed across callbacks remain.
3. Default `prepareCall:selector:` copies Execution's own frame slots directly.
   Replacing that bulk operation remains observable. Explicit scalar operations
   still work, including the authored preparation implementation. Their interception
   no longer defines the internal steps of the default bulk operation.
4. Host contexts report scoped native references through the existing GC tracing
   operation. No Id/GC API was added. Scope closure needs no per-result release;
   native setup retains grouped arguments until reporting is available.
5. ProjectBuild/HostProject observation traffic uses existing project-policy Maps,
   Vectors and Strings. Policy receives the original snapshot Oop. Native code
   decodes records where it needs their values. The executable's remaining command
   result adapters retain their existing JSON transport. Native String extraction
   also uses String's bulk operation directly, avoiding an intermediate Bytes copy.

## Reproducing

Build the release native composition and the examples. Use a composition matching
its source revision; stale native libraries are not a valid baseline.

```sh
CARGO_TARGET_DIR=.cargo-target/oop-slice mise exec -- cargo build --offline --locked --release --workspace --lib
CARGO_TARGET_DIR=.cargo-target/oop-slice mise exec -- cargo build --offline --locked --release -p birnam-siphon --example invariant_costs --example project_transfer_costs
BIRNAM_TEST_MODULE_DIRECTORY="$PWD/.cargo-target/oop-slice/release" .cargo-target/oop-slice/release/examples/invariant_costs strings
```

Other `invariant_costs` modes are `text`, `calls` and `roots`. The project example
requires a complete installed test prefix; point `BIRNAM_TEST_MODULE_DIRECTORY`
at its `lib` directory. Its baseline uses `--legacy-json` against the pre-change
HostProject implementation. The baseline hashes in the JSON records identify that
code; the example source is included in the corresponding change commit.

## Checks

Focused checks cover binary data and chunk boundaries, current constructors,
collecting callbacks, forbidden per-byte Sequences, direct String bulk reads,
Host scopes closing out of order, reclamation, forbidden per-result retain/release,
forbidden scalar sends inside default call preparation, and replacement of the
bulk operation itself. Existing Execution unit tests and compiler integration
cover authored replacement, aliases, inspection and recovery. Project checks
cover collecting policy calls, unchanged snapshot identity, source observation,
query liveness and error propagation. Workspace all-target/all-feature compilation
also passes.

Native tests which initialize separate runtimes must run in separate processes;
a combined HostProject test invocation failed that initialization constraint, and
the affected source-observation test passed when run independently. These are
focused checks, not a claim that the entire release test matrix was run.
