# Validation

During [Design 84](design/84-message-only-native-modules.md), full-language
validation is unavailable: the old native API and construction interfaces have
been deliberately deleted before caller migration. ABI/Id checks and independent
compiler recovery tests can still run. Deoptimization and concurrency assertions
are retained, but their Runtime/native harnesses need migration. Existing installed
prefixes do not validate the current source. Release validation is not required
for this deletion work.

Use `mise run verify` for everyday correctness and `mise run verify-plan` to see
its steps before execution. This is the current workflow; Design 74's temporary
controllers and chronological measurement notes are historical evidence.

| Task | When to use it |
| --- | --- |
| `mise run verify` | Everyday workspace, independent byte fixtures, installed native behavior and compact semantic integration |
| `mise run verify-exhaustive` | Compiler, runtime, GC, recovery, language or ABI changes; every forced exit and full diagnostic matrices |
| `mise run verify-release` | Release acceptance: exhaustive correctness, export/dependency audit and two independent cold-source installs |
| `mise run verify-performance` | Three alternating off/on pairs on four ordinary application/cold cases; practical slowdown screen without quiet-host waits |
| `mise run build` | Development prefix with Cargo reuse, separated by configuration |
| `mise run install` | Cold production install without development cache reuse |

The full [coverage map and delivery record](design/77-validation-workflow.md)
explains which semantic checks are in each plan. Correctness does not wait for
AC power or a quiet machine. Host admission remains available for an explicitly
requested qualified speedup claim. A practical performance comparison must state
its scope and retain answer/identity checks; incomplete runs are not passes.

For a focused iteration, preview
`python3 scripts/validate.py --suite routine --plan`, then select named steps with
`--only NAME`. Required prerequisites are included. Default total deadlines are
15 minutes for routine and 30 minutes for exhaustive/release. Use `--deadline N`
when a task explicitly needs a different budget; expiration records a failure.
Do not silently shrink coverage to fit a deadline.

The `method-values` step checks owned callable reflection through source,
source-free compiled modules, mapped/rebuilt images and baked executables,
including replacement, removal, callback GC and process-local image rejection.
For a prebuilt debug installation, run `scripts/method-values-smoke.sh PREFIX`.
The `method-unit-lifetime` step uses separate REPL turns to verify that retained
Methods and aliases keep source and anonymous synchronous/resumable Units callable after
cache eviction and GC, then release them after the last binding is removed.
Run `scripts/method-unit-lifetime-smoke.sh PREFIX` against a debug installation.

Each build and matrix writes progress and its log under `.cargo-target/validation`.
Full matrix coverage is checked by the existing coordinate and semantic oracles.
For counter-bound timing prerequisites, select only the needed matrix and workspace,
for example `python3 scripts/validate.py --suite exhaustive --only workspace measure-release`.
The timing driver accepts that `run.json`; it does not require cold-source proof.
Re-running reuses only terminal passed evidence with matching commands, relevant
inputs, toolchain/configuration, prerequisite evidence and output artifacts.
`--no-resume` explicitly requests fresh execution. A changed output or added input
invalidates reuse. Never infer a live process from a receipt saying `running`.

CI fetches locked Cargo dependencies before offline work. Relevant code and test
changes select exhaustive validation; bootstrap/workflow changes and weekly or
manual runs select release acceptance. Other changes select routine. Exhaustive
CI shares one installation and two diagnostic builds across four disjoint matrix
workers; a final job checks the combined coverage and all prerequisite evidence.
Each worker has a fixed deadline. Logs and receipts are uploaded on success and
failure. Local compilation products and raw new measurements
belong under `.cargo-target`, not in the documentation tree.

Optimization implementation and default decisions are recorded in
[74AK](design/74ak-bounded-qualification-and-defaults.md). The validation cleanup
changes no language semantics or optimization defaults. Old experimental tools
remain explicitly opt-in; their continued existence is not an acceptance gate.

The Design 81 preparatory extraction phase is closed at 81CA. Its
[closeout recipe and evidence](design/81-production-extraction-status.md#validation-and-evidence)
cover the unchanged production boundary with core debug tests, independent native
admission and loaded startup/compiled-image checks. Release validation was waived
for this work. The per-contract tasks below remain regression tools, not a queue
of further extractions.

The former factory-based storage-observation, storage-tracing and live-tracing
runners have been removed. Their old task names are not current validation
commands. Retained semantic tests must use the production Id and upper objects
when native integration is restored.
