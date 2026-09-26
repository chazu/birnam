# Validation

During [Design 90](design/90-functional-modules.md), full-language
validation is unavailable: the old native API and construction interfaces have
been deliberately deleted before caller migration. The production prefix now also
compiles and executes source through PEG/Lang, Codegen and TCC. Full-workspace and
full-language acceptance remain unavailable; this source path does not restore
every upper module. Deoptimization and concurrency assertions are retained, but
their Runtime/native harnesses need migration. Existing installed
prefixes do not validate the current source. Release validation is not required
for this deletion work.

The retired IdApi manifests, module-table headers, factory publishers and
construction-graph validator are gone. ABI inventories no longer demand those
exports or fingerprints. The two retained C headers describe Id messages and the
recovery registration envelope. Older native fixtures retain their behavioral
assertions but require message-based setup before execution; do not restore the
old headers to run them.

`mise run siphon` builds only production Id and tests the Siphon handoff, real
Id interning/loading, and invalid paths. It is available during caller migration.
The CLI executable and parser tests belong to `birnam-cli`; host-operation
tests remain in `birnam-host`. Build/install-tool tests run independently with
`cargo test --offline --locked -p birnam-build`.

For the converted production prefix, run `mise run native-messages`. It builds
and audits actual dylib exports/dependencies and declared install names, then
exercises Bootstrap, roots, GC,
object behavior, text, numbers and collections through Id. Collection callbacks
return unretained temporary keys and accumulators; tests check both survival during
use and reclamation after success/failure. Root registration is checked beyond
32 enumerators, including ordering, duplicates and tracing their children.

Execution callable tests also run there: owned Method invocation,
binding-only ownership, replacement during an active native call, nested lookup
with distinct contexts, argument GC lifetime and language result conversion. A C
native callable is built separately and loaded through Bootstrap. A second C fixture runs canonical descriptor steps on production Machines and
Blocks: calls/tail calls, budgets, waiting, continuations, native boundaries,
collection-phase rejection and paused code-owner retention/reclamation. Native
inline candidates also exercise flat/graph recovery, GC during recipe reads and
materialization, changed control/lookup/layout/trace-owner guards, single recovery
publication, and release after recovered-frame completion or abandonment. Run
`cargo test --offline -p birnam-execution --lib` through mise for the 21
engine/planner/graph tests. Hot native PC selection tests cover withdrawal,
replacement and collection while entered. Numeric tests prove execution of the
receiver sends, live ErrorVt, checked failures and fallback for replaced entries.
Registration/GC churn without checkpoints covers retired context-address reuse.
Run `mise exec -- cargo test --offline -p birnam-codegen numeric_chain` for the 9
shared arithmetic/lowering tests.

Batch 1's native Execution and displaced Runtime wrapper cleanup are complete.
Allocation-elision is explicitly deferred to batch 2 with its owner protocols and
compiler producers. Optimizer equivalence/performance and the full retained upper Runtime harness
remain unproven; its unconverted cases are migration inputs.

The suite now includes 14 integration binaries. The `compiler` binary tests
source literals, threaded sends, conditional and capturing Blocks, named methods,
replacement with exact captured invocation, syntax/compilation/initialization
failures and precise GC inside factories. Generated protected-boundary exits
deliver once into their callers; destroying a waiting Machine releases its code.
Actual TCC Units survive binding-only and waiting-frame retention, then
both live Unit count and libtcc relocated bytes return to zero. A C recovery
candidate compiled by TCC recovers into a generated baseline, removes its own
binding, collects and attempts checkpoint before returning. This proves real
compiled recovery/mapping lifetime, not the separate allocation-elision emitters.
The script builds TCC with `resource-metrics` for independent allocation evidence.

The compiler test checks ordinary generated execution without automatic PC
candidates. It rejects speculative native Method construction while compiling
small/large functions and captured Blocks, exercises repeated execution and
replaced Integer factory effects, and collects during live Integer conversion and
Machine slot reads. All final Unit/relocated-byte counts return to zero. Explicit
candidate hot selection/withdrawal and compiled recovery/lifetime retain separate
production fixture coverage. This is correctness and construction-path evidence,
not an allocation-elision or runtime-speedup claim.

`allocation_accounting` checks actual GC charges against literal padded-size
expectations, collection at the configured threshold, unchanged charges after storage
release and failed tracing, and successful-collection reset. Replaced Id geometry
queries collect or allocate recursively while the outer object remains rooted;
a subprocess verifies the historical fatal diagnostic for committed overflow.
`memory_activation` independently sums the existing heap, and the compiler test
sums new physical Id records across source compilation and execution without using
the production charge helper. These are actual-allocation accounting
proofs. Prepaid deferred identities and single-charge materialization are no longer
requirements. See the
[revised optimization acceptance contract](design/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
Future elision tests compare language behavior, effects, observable identity and
recovery, and separately measure actual allocation savings. They must not demand
matching baseline allocation counts or automatic collection/finalizer schedules.

Pure/debug checks: `mise exec -- cargo test --offline -p birnam-codegen
-p birnam-execution -p birnam-peg -p birnam-native-compiler --lib` runs 97 compiler,
21 engine, 3 matcher-oracle and 1 native-owner tests. Adding
`--features birnam-native-compiler/resource-metrics` runs a second native-owner
test that checks relocated allocation bytes. Build-tool tests also pass
with the canonical module filenames. Artifact/PortableCode language admission,
Image, workers and full CLI acceptance remain separate.

Store now loads before Execution/compiler in Bootstrap. Its production integration
binary tests schema/cardinality, prospective history and compaction, Cell retention
and reclamation under precise GC, current Store/Integer dispatch, destructive
collecting snapshot callbacks and cleanup on failure, owner-level image transfer,
mutation rejection during tracing, resource-ceiling refusal/recovery, and compiled
source using Store, db, Cells and Globals. It also checks Store-backed Globals
history, MOP authorization/notification, mirror invalidation, hook roots and live
notification-method replacement. GC visitors and release now use sends, with tests for replacement between
emissions, failed marking, inactive visitors and failed release payload retention. `mise exec -- cargo test --offline --locked -p birnam-store
--lib` also runs ten retained algorithm tests, including the independent snapshot
oracle; two manual timing investigations stay ignored. Full image save/restore and
self-hosted task-policy acceptance are not implied by Store transfer/Cell tests.
See [Store's boundary and remaining consumers](../rust/birnam-store/README.md).
Numeric receiver tests also cover current method replacement, collection during
evaluation and after predicate commit, Unit fallback, exact failures, inactive/
repeated commits and custom-lookup fallback. Optimization remains paused.

The obsolete `native-modules` task and installed adoption driver are removed.
Cached floor-function discovery tests are also retired; they asserted the old
parallel API's caching policy. The independent C Id protocol round trip remains
in Id's unit tests. Source/image smoke tests and recovery/concurrency assertions
remain for their later integration; do not count them as passing prefix coverage.

`mise run verify-plan` describes the future full-language workflow below.
`mise run verify` and the other full suites still require upper integration;
they are not current acceptance commands for this checkout. Design 74's temporary
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


The first source discarded-clone consumer runs in the same compiler production
test. It compares physical Id inventories against an unannotated baseline and
checks replaced lookup/clone/allocator/index methods, guard-time mutation and GC,
nonempty and escaping/bound fallback, paused Frame inspection and final code
reclamation. It also prints an alternating local debug timing comparison;
correctness does not depend on wall-clock timing. Exact source, commands and
limits are in [the implementation receipt](design/90-discarded-clone.md).


The combined numeric-predicate source test checks real allocation reduction
against an unannotated baseline; checked arithmetic, errors and comparison
rounding; current operation/factory/allocator effects; mixed/error-family fallback;
paused intermediate inspection; collection before and after the atomic result
commit; and complete native-code reclamation. Its timing output is diagnostic,
not an acceptance threshold. [Exact scope and commands](design/90-numeric-predicate.md).


Relational now loads after Store. `mise run native-messages` includes its real
module, export/install-name audit and production integration test, bringing the
suite to fifteen integration binaries. The test covers the unchanged query/rule/
aggregate/pull/history API, compiled source, live collecting replacements,
reentrancy and snapshot lifetime. Its 21 evaluator tests pass via
`mise exec -- cargo test --offline --locked -p birnam-relational --lib`; the
manual timing test stays ignored.
See [Relational's contract](../rust/birnam-relational/README.md).


The production compiler test now exercises generated Store-backed Globals:
publication across evaluations, current reads from previously compiled code,
Namespace fallback on nil, lexical scope, Store history and query visibility,
collecting getter/setter replacements, refused-write short-circuiting, exact native
failures and compiled Block retention/reclamation. Reflect publication coverage
checks current Store and hook lookup, collecting authorization/notification,
exact refusals and native failures, committed binding/mirror visibility during
notification, and adapter reclamation after refusal or removal. Existing captured
Method and paused-frame tests also run through managed installation. This does not
claim staged module publication/rollback.

The suite now has sixteen integration binaries. `system_output` loads System,
Printing and System Output through production Bootstrap and compares actual
stdout bytes from a subprocess. It covers source sends, binary/UTF-8/empty output,
receiver identity, exact-String conversion, live replacement, collecting methods,
nested output and reload preservation. The dylib audit includes all three modules.
Their retained unit tests and build-tool tests run with
`cargo test --offline --locked -p birnam-prelude -p birnam-printing -p birnam-system-output -p birnam-build`.
Full Host/CLI installation and image smoke tests remain deferred.

The input/environment migration extends the suite to seventeen integration
binaries. `system_services` runs real stdin and environment operations in isolated
subprocesses, including invalid Unicode, binary/long lines, EOF, failed reads,
collecting dependencies and lazy Error publication. Both new modules participate
in the export/install-name/dependency audit. The existing `modules` and `text_error`
tests retain their assertions and now forward replacements through Id sends.

`system_services` also covers File I/O through Bootstrap: binary round trips,
empty files/truncation, source sends, errors, live collecting factories, reloads
and filename snapshots that survive mutation by contents callbacks. File I/O is
included in the dylib audit. `cargo test --offline --locked -p birnam-file-io --lib`
checks bounded reads at the limit, one byte over it, and native read errors using
small fixtures. The production limit remains one GiB.
