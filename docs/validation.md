# Validation

## Oop-only native migration

Current runtime checkpoint: **`683c9fc2`**, branch `codex/oop-only-id-slice`, worktree
`/Users/chazu/dev/zig/birnam-oop-id`. This is an isolated, uninstalled migration.
The [work summary](design/logue-runtime-work-summary.md) and
[accepted plan](design/oop-only-id.md) describe its scope and remaining work.

From that worktree, run:

```sh
bash scripts/oop-id-slice-test.sh
```

The [latest recorded receipt](measurements/id-send-correction-20261023.json) passes.
The script builds the native closure listed in its package/export inventory and
runs Id/value/collection/I/O/PEG/artifact unit and integration checks. Its explicit
library paths and nonexistent module-search directories keep the installed prefix
and upper language artifacts out of this acceptance boundary.

| Evidence | Coverage |
| --- | --- |
| Native runtime, independent C and TinyCC callers | Bootstrap calling convention, actual loading, live lookup/DNU, entered/captured references and owner-thread refusal |
| Failure subprocesses | Finalization, partial Storage publication and retirement failure containment |
| Allocation/global-lookup subprocesses | Allocation policy/accounting and metacircular lookup replacement |
| Numeric integration and stdin-error subprocesses | Actual numeric/text/collection owners, binary I/O, controlled environment values, file snapshots, PEG and artifact inspection; independent C Error/callback fixtures |
| `dispatch-contract` subprocess | Id installs no `apply`; an ordinary send succeeds even when all lookup on the selected Method is rejected |
| Export audit | Actual dylib exports match the migrated manifests |

The architectural contract is mandatory; passing the other checks cannot waive
it. The old tests that endorsed application/forwarding protocols were removed or
replaced by tests of actual data operations and ordinary sends. Receipts are in
`.cargo-target/oop-slice/qualification/` by default; `BIRNAM_OOP_TARGET` selects a
separate target. The numeric integration log includes binary stdout/stderr, so
read it as bytes or with explicit decoding-error handling.

This does **not** qualify full Bootstrap, authored Error, generated language code,
Execution/stackless integration, code reclamation, images, host/CLI, workers,
installed behavior or Logue performance. Those remain required by the plan. The
commands and results below describe the earlier production migration and must be
ported/re-run for the Oop-only candidate; they are not current passing results for
this branch.

## Earlier production-migration validation reference

[Design 90](design/90-functional-modules.md) has moved the production Rust
modules to Id messages and removed the aggregate Runtime. Workspace and native
integration tests are runnable. The validation planner also covers the installed
Host's source, project, Image, worker, interaction and generation paths; see the
current commands below. Full migration completion still requires complete
validation and obsolete-harness cleanup.

The module-by-module notes below record the incremental migration evidence.
Their historical counts and exclusions describe those slices, not the current
plan. Existing installed prefixes alone do not validate changed source.

The retired IdApi manifests, module-table headers, factory publishers and
construction-graph validator are gone. ABI inventories no longer demand those
exports or fingerprints. The two retained C headers describe Id messages and the
recovery registration envelope. Older native fixtures retain their behavioral
assertions but require message-based setup before execution; do not restore the
old headers to run them.

The uncompiled legacy compiler and its orphaned cache/binding/finalizer tests are
now preserved only in Git; see the [migration reference](../rust/birnam-codegen-runtime/README.md#remaining-migration-reference).
The obsolete `codegen-state` and synchronous `evaluator-lifetime` runners, feature
flag and plan/receipt requirements are removed. Their removal is not a passing
result for the remaining Image/publication work or the legacy optimizer matrices.
The current compiler integration test exercises owned Method/Unit lifetimes.

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

`mise run verify-plan` lists the current Id-message validation plan. The
planner runs workspace tests, independent byte fixtures, validation-tool tests,
the native-message suite and a fresh release installation. Installed acceptance
is divided into five groups: CLI/project/worker behavior, Image entry,
REPL/socket/tooling, generation activation/recovery/rollback, and authored task
scheduling with native boundaries and task Image persistence. Each group
copies the installation before relocating it or creating fixtures.

| Task | Scope |
| --- | --- |
| `mise run verify` | All migrated workspace, native and installed Host checks |
| `mise run verify-exhaustive` | The same complete migrated correctness plan |
| `mise run verify-release` | Correctness plus export/dependency audit and two independent cold-source installs |
| `mise run build` | Development prefix with Cargo reuse |
| `mise run install` | Cold production install |

The retired Runtime diagnostic matrices and performance controller are no longer
part of these plans. Their removal does not prove optimizer equivalence or a
speedup. Method-value reflection and reclamation now run in native owner tests.
Installed artifact checks
cover globals published by nested source loading into previously compiled callers. The task group runs the retained 171-check scheduling
fixture with actual native boundaries and checks normal startup, rejection of
retained task handles, outcome persistence and fresh tasks in mapped/rebuilt
Images. It replaces the old task-control smoke driver. Native materialization
checks retain source, synchronous and resumable Methods through aliases, then
verify Unit and native-byte reclamation after the final reference disappears;
these replace the old method-unit-lifetime driver.
Installed task acceptance also exercises Concurrency batch preparation, the retained
19 service checks, source-free worker startup and temporary image cleanup. Native
materialization tests cover synchronous/resumable eviction and final-owner release,
replacing the old REPL unit-count arithmetic in portable-resumable-lifecycle.
Image owner tests now also preserve nested portable closure construction, boxed
alias identity and private System receiver behavior. Installed generation checks
carry a projected initializer closure through mapped/rebuilt recovery, resave,
successor activation and rollback. These replace the old portable-image-v8 and
stackless-image-lifecycle drivers. Candidate retirement tests independently verify
physical Unit reclamation; frozen format fixtures retain byte-level checks.
Artifact acceptance checks one live parser invocation per compilation; Image
recovery uses portable code without the retired compileImageCode path.
Installed Image acceptance covers native descriptor loading and StringBuilder
clients in mapped/rebuilt images and baked executables. Retained native builder
buffers are rejected without overwriting the destination. The old provider-switching
Image/Store wrappers, TCC export list and diagnostic storage driver are retired;
current installed groups, manifest export audits and native lifetime tests cover
their supported contracts.
The per-owner primitive Image shell drivers are retired as well. Their startup
and live-dispatch cases run in `numbers`, `numbers_loading`, `text_error`,
`collections`, `block_loop`, and `system_output`. The Image owner suite checks
numeric bits/errors, boxed identity, a 130-element Vector, signed Range bounds,
typed Map keys and persistence, private/inherited behavior and captured code
through fresh mapped/rebuilt processes. Installed Image tests cover entry and
baking under the running-phase mutation guard.
The consolidated `system-image-test.py` checks restored System aliases, live
file/environment lookup, binary stdin/stdout, EOF and real input failures,
retained bytes and collector controls in mapped, rebuilt and baked entry modes.
It replaces the five per-service Image wrappers; native service tests retain
live dependency/factory replacement coverage outside running Image entry.
Portable Code's authored suite now checks callable retention and reflected method
removal directly, without the removed Tcc/CodeBinding counters. Exact final-owner
Unit/byte reclamation remains in the native materialization suite. Suite loading
now stops on an Error, and an unhandled test-case Error counts as a failure.
The old authored CompiledUnit suite is replaced by `compiled-unit-cli-test.py`:
57 checks cover inert opening/mapping, dependencies, preparation rejection,
activation/events, exact rollback, discard receipts and inherited supersession.
Its source-free console returns intents to the idle Host instead of fulfilling
inside a test callback. Prepared-token drift/compensation and exact physical
reclamation remain in `generation_transaction` and `candidate_retirement`;
selected Image recovery runs in Host's lifecycle tests.
The installed
task group also checks live closure aliases, captures and ordered nested effects
through conservative and precise collection, replacing the removed closure-elision
flag matrix. Guarded-slot benchmark fixtures whose compiler switches and driver
were deleted are retired; this is not optimizer qualification.
`rust-native-memory-smoke.sh PREFIX` runs the current native memory, root and GC
owner tests under Darwin allocator diagnostics. The old PEG identity-export probe
is replaced by the manifest export audit in `native-message-test.sh`.
The benchmark driver measures the current source-stdlib startup once, without a
removed-provider decoy. Its Image fixture requires a successful save report and
boots a saved main that checks the retained marker. Timing drivers for deleted
Id optimizer test binaries and the removed Design 66 counter instrumentation are
retired; historical measurements remain evidence of their original revisions.
The unused Artifact sentinel wrapper is also removed; the native Artifact tests
exercise rejection before loading or activation through the current messages.

For a focused iteration, preview
`python3 scripts/validate.py --suite routine --plan`, then select named steps with
`--only NAME`. Required prerequisites are included. Local plans have a four-hour
total deadline; `--deadline N` overrides it. Each Host group also has a bounded
step deadline: one hour for interaction/tooling (including documentation generation),
forty minutes for CLI/project, generation recovery or task scheduling, and twenty
minutes for Image entry. Expiration
records failure, never partial success. Do not shrink coverage to fit a deadline.

Every step writes its log and receipt under `.cargo-target/validation`.
Re-running reuses only terminal passed evidence with matching commands, relevant
inputs, toolchain/configuration, prerequisite evidence and output artifacts.
`--no-resume` requests fresh execution. A changed output or added input invalidates
reuse. The final audit also checks each receipt against the requested plan.
Never infer a live process from a receipt saying `running`.

CI fetches locked Cargo dependencies before offline work. Bootstrap/workflow
changes and weekly/manual runs select release acceptance. All suites share one
installation across four independent workers covering all five Host acceptance groups; a final job checks
the complete plan and all prerequisite evidence. Executable modes are preserved
in the seed archive. Logs and receipts are uploaded on success and failure.
Local compilation products belong under `.cargo-target`.

Optimization implementation and default decisions are recorded in
[74AK](archive/design/74ak-bounded-qualification-and-defaults.md). The validation cleanup
changes no language semantics or optimization defaults. Old experimental tools
remain explicitly opt-in; their continued existence is not an acceptance gate.

The Design 81 preparatory extraction phase is closed at 81CA. Its
[closeout recipe and evidence](archive/design/81-production-extraction-status.md#validation-and-evidence)
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

The System GC migration brings the suite to eighteen integration binaries.
`system_gc` covers the four existing controls, live count/Integer conversion,
replacement and native failures, conservative/precise payload behavior, live
mode/layout changes during tracing, native-stack roots in both modes, automatic
suppression with unchanged accounting, explicit collection and module reloads.
System GC participates in the dylib boundary audit. Payload-specific exclusions
in the numeric/collection tests explicitly enable precise mode; the common test
collector still controls only native-root scanning.


Native Loader adds the nineteenth production integration binary, `native_loader`.
Independent C fixtures exercise Unit/Object/invalid results, collecting
initializers, missing/incompatible descriptors and initialization failure through
System's real wrappers. Tests cover path snapshots under mutation, sequence order,
transient rooting and cleanup, collecting/replaced dependencies, exact dependency
failures, lazy Error publication, compiled source and reload preservation. The
existing CLI/image loader smoke is retained for those later migrations.


Source Loader adds the twentieth integration binary, `source_loader`, and a dylib
boundary audit. The test loads the complete authored Module manager through
`System load:`, checks dependency ordering and Store-backed state, and exercises
real source files, independent revision digests, source identity/GC retention,
duplicate and sealed registration, nil/syntax/runtime/file results, collecting
factories/parsers, registry reentry, path mutation, replacement failures and reloads.
This is acceptance for the source-loading slice without embedding; it does not
restore Artifact activation, generation/Image recovery or full Host/CLI startup.


Bytes/embedding adds the twenty-first integration binary, `embedding`. It covers
all byte values, empty payloads, clone/conversion/indexing, MOP changes, collection
and dead-object reclamation, collecting/replaced constructors and byte access,
source-relative paths, current Globals lookup, lexical receiver shadowing, native
failures and invalid/oversized files. An embedded Block runs after source/asset
delete. A real system-compiled dylib loads and runs after asset/generated-C delete.
A retained Bytes also survives actual TCC Unit reclamation, then is itself
collected after release. This proves native payload self-containment; Image
round-trip and full project/CLI integration remain separate migration gates.


Block Loop adds the twenty-second integration binary, `block_loop`, and joins the
dylib boundary audit. It checks 10,000 native iterations, compiled Store counters,
both loop directions, exact errors, live callback/factory replacement, native
failures, transient argument roots and per-iteration result reclamation. It also
checks the native suspension barrier and then installs the unchanged authored
Boolean/while prefix to prove existing/new Block replacement, suspension/resumption
and reload preservation. Full authored scheduling policy and Host/Image remain
outside this slice.


Artifact adds the twenty-third integration binary, `artifact`, and joins the
native boundary audit. It covers every descriptor/list accessor, diagnostic
mapping and limits, collecting dependency replacements, typed failures, transient
roots and reclamation, clone/MOP/reload and compiled source. Inspection of a real
dylib leaves its constructor sentinel untouched. Codecs are unchanged; activation,
PortableCode, Image and Host/CLI remain separate gates.

## Release candidates and resumable promotion

Release tooling requires Python 3.12 or newer on `PATH`, the pinned Rust
toolchain, and arm64 macOS. `scripts/release.sh patch|minor|major` now prepares
and commits VERSION/CHANGELOG **before** qualifying the candidate. It creates
the tag only after acceptance, then pushes and publishes. It does not run an
additional pre-bump qualification pass.

The release path is:

1. `validate.py --suite release --candidate` executes the complete release plan.
   Its one shipping installation is cold, with no development build cache.
   The candidate prefix lives outside the checkout at a stable, commit-specific
   path, recorded in the qualification receipt. Diagnostic native tests and the
   existing independent two-snapshot cold-bootstrap proof retain their separate
   builds and coverage.
2. `dist.sh --prefix PREFIX` packages that exact installation without rebuilding
   or invoking the full installed-test matrix. It checks the receipt, version,
   signatures and staged closure. An existing identical archive is reused
   byte-for-byte. `--output PATH` allows a separate local artifact destination.
   Standalone `dist.sh` still builds cold, but packaging itself is not a claim of
   full qualification.
3. `release-acceptance.py ARCHIVE EVIDENCE --qualification QUALIFICATION/run.json`
   re-audits every required gate, its command, current inputs, toolchain/config,
   dependencies, logs and artifacts. The extracted installation must match the
   qualified prefix exactly. Only then does it run the small archive gate:
   signatures/layout, relocation, source/GC/image checks and startup without
   parsing or compilation. Without `--qualification`, standalone acceptance
   retains its full installed-test matrix.

After an interruption, run `scripts/release.sh --resume` on the clean prepared
release commit. This retains the version and reuses matching passed evidence;
failed, missing, modified or invalidated gates run again. Keep the candidate
prefix and `dist/vVERSION-id-message-acceptance/` between attempts. Deleting the
prefix invalidates build evidence and requires rebuilding. Changed source gets
new evidence; passed status alone never authorizes publication. Aggregate
`reused` display flags do not invalidate an otherwise identical archive proof:
that gate binds the individual proof hashes instead.

A process lock serializes release commands, and the existing validation lock
also protects archive checks that use shared test binaries. Qualification may be
inspected with `--candidate --plan`, but candidate mode refuses partial `--only`
plans. Dirty-tree checks remain available for local development and cannot
produce a promotable archive receipt. Version/tag/push/publish actions are not
part of the nonpublishing validation commands.

The initial change removes two repeated installed matrices and the redundant
shipping build from the previous release path. It adds pipeline regressions to
the mandatory plan and preserves all existing gates. Independent bootstrap proof
builds remain separate. Full release duration is measured on the actual candidate
rather than inferred from the removal of duplicate steps.

Implementation evidence: [release workflow checks, 2026-10-13](measurements/release-workflow-20261013.json).
The 31 tooling regressions pass, including candidate-build reuse, commit
invalidation, interrupted preparation, deferred tagging, failed/changed proof
rejection and unchanged-archive reuse. A real cold build, packaging and the new
archive smoke gate also passed; repeating packaging reused the same archive in
2.16 seconds. This is component qualification of the tooling, not a full release
qualification or an end-to-end release duration measurement. No version was
bumped and no release was published for these checks.


### Two installed-test lanes

The local planner defaults to `--jobs 2`; `--jobs 1` retains serial execution for
comparison. The fixed lanes are interaction/CLI and tasks/image/generation.
Build and native preparation steps finish before either lane starts. Each group
gets its own copied runtime and temporary fixtures. The `host-test-binaries`
gate copies all shared Rust test executables into a hash-verified snapshot;
self-spawned test processes use these immutable copies. Cargo output replacement
cannot change an executable under a running test. CLI-only reference/oracle
builds retain their independent Cargo target directories.

Both lanes use the existing complete gate receipts and final audit. A failed
lane cancels its peer's process group and prevents new checks from starting;
completed gates remain available for resume. Aggregate receipt updates are
serialized, while the subprocesses execute concurrently. The total recorded
`wall_ns` measures actual plan duration, including cache checks and preparation,
not the sum of overlapping group times.

Every installed command reports a label and elapsed time. The group timing files
live under `timings/` beside the qualification receipt and are bound as gate
artifacts. They retain failures and incomplete commands rather than treating a
partial group as passed. CI keeps its existing shards and includes the prepared
executables in the shared validation artifacts.

Measured two-lane qualification: **34.7 minutes** for all 21 gates,
including native integration, all five installed groups and independent cold
bootstrap builds. See [the pre-commit qualification receipt](measurements/release-lanes-20261013.json).
The earlier attempt stopped for a cache-input correction is excluded from this
completed-run duration and is not claimed as passed.
