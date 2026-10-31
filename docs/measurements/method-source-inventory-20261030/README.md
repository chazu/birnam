# Method source inventory feasibility experiment

2026-10-30, baseline `58fddef260750bc6a0a07e1b40e6d7f311f9e94f`, Apple M3,
arm64 macOS 14.6, Rust release build.

**Decision: do not switch production to this prototype.** The authored collector
bootstraps without a new native inventory seed and produces the expected source
lists, but its measured cost is much greater than the existing complete metadata
operation. Production runtime code, compiler callers, formats, and the native
collector remain unchanged.

This follows the [candidate comparison](../../design/first-migration-comparison.md).
It is a failed performance feasibility test, not a completed migration or proof
that a Birnam compiler is impractical. The prototype, probe, and bootstrap patch
here are experiment artifacts, not production modules or instructions to adopt
their internal structure.

A [subsequent investigation](../method-source-investigation-20261030/README.md)
measured the arithmetic/control expansion and implemented a small Id entry
simplification. This earlier receipt retains the original baseline and rejection;
the compiler migration remains incomplete.

## What worked

The [authored collector](inventory.birnam) implements syntactic recognition through
ordinary CST, String, Integer, Boolean, and collection messages. It returns an
ordered Vector of exact declaration source texts. The
[probe](probe.rs) compares those texts with the baseline native collector for six
fixtures: an expression with no declarations; a mixed nested/quoted/dynamic/
shadowed/duplicate case; 50 ordinary bindings; 32 method declarations; 32 nested
blocks; and the real `source-registry.birnam` module. All six matched in both
default resumable and explicitly synchronous compilation.

The isolated [bootstrap patch](cold-bootstrap.patch) loads the existing Lang seed,
compiles this collector through `Codegen compile:`, creates a Block from the
returned Method, and explicitly invokes it. It then parses and inventories a
declaration **before** loading Error, GlobalsOwner, or SourceRegistry. Those three
compiled artifacts were removed from the copied prefix before process entry.
Bootstrap subsequently creates them normally and completes.

This proves the proposed early installation route for the exercised program.
It does not require a native method-source collector seed. It also does not prove
an installed production cutover, arbitrary early programs, all malformed CST or
replacement behavior, or a complete release qualification.

## Performance

The replay medians below are milliseconds per operation. The complete metadata
column is measured with the current native collector. The authored columns time
only the proposed inventory on an already parsed CST, excluding parsing, CST
projection, and the other metadata analyses.

| Input | CST nodes | Existing complete metadata | Authored inventory default resumable | Authored inventory synchronous |
| --- | ---: | ---: | ---: | ---: |
| `42` | 6 | 0.124 / 0.123 | 1.454 | 1.070 |
| Mixed recognition cases | 175 | 2.022 / 1.953 | 57.510 | 36.870 |
| 50 ordinary bindings | 302 | 3.375 / 3.262 | 358.955 | 53.947 |
| 32 method declarations | 930 | 10.396 / 10.164 | 896.926 | 484.217 |
| 32 nested blocks | 159 | 1.831 / 1.802 | 43.101 | 29.868 |
| `source-registry.birnam` | 2719 | 31.203 / 30.372 | 2350.290 | 1435.378 |

The two metadata values correspond to the default and synchronous runs. The
synchronous inventory alone costs about 48 times the existing *complete* metadata
operation on the dense and real-module inputs. This is not a measured 48-fold
whole-compiler regression: production callers were never switched. It is strong
evidence against adding this traversal to those callers in its present form.

The native inventory alone takes roughly microseconds, using its existing
`OwnedCst` input. Exact samples, ranges, timing scope, baseline identity, and raw
evidence hashes are recorded in [result.json](result.json). That native-only timer
excludes obtaining its CST, just as the authored inventory timer excludes parsing
its input. The complete metadata timer includes parsing and projection.

Measurements use two warm-up operations and seven samples. Each native-inventory
sample averages 1000 calls; each authored-inventory or complete-metadata sample
times one call. Timing includes the Host scope used for the latter operations and
its cleanup. Automatic collection remains enabled, with no forced collection
between samples. Some workloads have substantial timing variation; the receipt
retains minima and maxima, and local raw results retain every sample. Startup and
collector compilation are outside the timers. The native namespace comes from
Bootstrap; this is not a CLI/stdlib startup benchmark.

An initial exploratory run produced the same qualitative result. It timed single
native inventory calls, which were too short for useful precision on tiny inputs;
the replay corrected that by averaging 1000 calls. The initial data remains in
the local evidence rather than being silently replaced.

## Bounded profile

A separate three-second macOS `sample` capture during repeated synchronous dense
inventory calls recorded 2475 main-thread samples. Its largest individual leaf
entries included method selection (328), vtable/behavior lookup (192), Method
entry (135), send handling (110), and payload lookup (91). Collection and object
table cleanup also appear. The profile run is separate from the timing table.

This points toward dispatch and object/collection bookkeeping as subjects for
diagnosis. Recursive inclusive stack counts must not be added together, and this
single profile does not prove that any validation, root, or counter is redundant.
Nor does it establish that Rust's ownership model caused the slowdown.

## Recommended next task

Reduce the prototype to a CST walk without declaration recognition, then isolate
the cost of reading children, branching, accumulating results, and keeping
temporary values alive. Measure sends, allocations, root work, and collection
time for the same input. Use the emitted code and sampled paths to distinguish
necessary semantics from repeated implementation work.

Change one demonstrated cost only after stating its before/after sequence and
checking current lookup, replacement, callbacks, collection, and code validity.
Rerun the full inventory as the acceptance workload. Do not introduce frozen
bindings, foreign-payload shortcuts, a new service, or a native walker behind a
Birnam wrapper just to make this pilot pass. A broad runtime or implementation-
language rewrite is not justified by this experiment.

This diagnosis is a separate next task. The migration remains incomplete until
its cost is resolved or the user explicitly accepts the measured tradeoff.

## Reproduction

Use a release prefix built from the recorded revision. The experiment used a
detached clean checkout, an initially empty build-cache directory, and a fresh
prefix, leaving the user's installed Birnam untouched. The installer records this
as `development-cache` mode; it is not a release qualification receipt.

From the repository root, a fresh baseline can be constructed with:

```sh
git worktree add --detach .cargo-target/method-source/baseline-source 58fddef260750bc6a0a07e1b40e6d7f311f9e94f
bash scripts/rust-install.sh install --root "$PWD/.cargo-target/method-source/baseline-source" --prefix "$PWD/.cargo-target/method-source/baseline-prefix" --profile release --build-cache "$PWD/.cargo-target/method-source/baseline-cache"
```

Then run the retained [replay tool](run.py). Select the baseline source root so
the independent native reference matches the prefix; keep this experiment's files
in the current checkout.

```sh
python3 docs/measurements/method-source-inventory-20261030/run.py --prefix .cargo-target/method-source/baseline-prefix --source-root .cargo-target/method-source/baseline-source --mode default
python3 docs/measurements/method-source-inventory-20261030/run.py --prefix .cargo-target/method-source/baseline-prefix --source-root .cargo-target/method-source/baseline-source --mode synchronous
python3 docs/measurements/method-source-inventory-20261030/run.py --prefix .cargo-target/method-source/baseline-prefix --source-root .cargo-target/method-source/baseline-source --mode cold
```

The tool builds a standalone probe under `.cargo-target/measurements`, checks the
prefix revision, and leaves logs and JSON there. Cold mode uses a fresh detached
checkout and copied native libraries; it modifies only that checkout and copies.
Its patch has a placeholder replaced with the absolute path of this authored
source. Cold mode requires an unused output directory for its scratch checkout
and prefix; pass `--output` to choose another location. Scratch worktrees are left
available for inspection. No installed prefix or production source is edited.

For a diagnostic profile, the probe also accepts `INVENTORY_CASE=dense`,
`INVENTORY_COMPILE_MODE=compileSynchronous:`, and `INVENTORY_PROFILE=1` when invoked
directly. After `PROFILE_READY`, sample that process for three seconds. These
profile-run timings must remain separate from unprofiled performance samples.
