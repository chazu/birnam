# Test and Bench own their state

2026-10-16. Follows the ordinary-record commit `e41c4f35` and live-loader commit
`3869bd65`. This is the remaining bounded tool-state slice of
[optional Store](optional-store-and-relational.md), before compiler metadata.

## Ownership and APIs

TestContext, ConsoleRunner, TestSuite, TestCase and Test now own nine reference
slots; BenchSuite, BenchCase, BenchResult and Bench own nineteen. Each exposes
`slotNames`; Reflect discovers/edits these fields. Case/result accessors are
ordinary replaceable sends. Clones copy references into independent slots, so
replacing a clone's field does not update the original. Referenced objects can
still be shared. Bench methods now write the actual receiver's fields instead of
the global Bench singleton, making cloned runners independent.

Bench's sample accumulator and JSON separator flag use local Cells. The clock
window, GC calls, calibration ceiling, warmup/error gate, repetition policy,
baseline subtraction, integer arithmetic and JSON field order are unchanged.
The existing run policy re-enables automatic GC after a suite; this slice does
not change it into a restoration of a previously disabled setting.

Only Test's discovery methods consumed `isTestSuite`/`testsModule` facts in the
repository audit. No external field-fact consumers were found for these owners.
Ordinary state therefore no longer needs fact aliases or a compatibility mirror.
Explicit existing facts are never scrubbed or interpreted as live fields.

Test owns an ordered persistent Vector of registered suites:

- `TestSuite named:module:` constructs and registers a suite.
- `Test register:` registers a suite once by identity; `unregister:` removes it.
  Both return the registry. Registration retains a suite until removal; snapshots
  or explicit descriptions may intentionally retain it longer.
- `Test suites` returns the current persistent Vector. Old snapshots remain stable.
- `Test suitesForModule:` filters registered suites by their current module-symbol
  identity, in registration order. Reflective module-tag edits affect later calls.
- `Test describeSuitesIn:` explicitly adds `isTestSuite` and `testsModule` facts to
  a caller-selected Store and returns that Store. It makes no subscription and
  performs no automatic cleanup. Use a fresh Store for each independent snapshot;
  repeated descriptions in one Store can retain earlier tags/history. The snapshot
  observes a registry inventory and current getter sends, not one atomic world.
  Independent `Store new` instances currently cannot themselves be captured in an
  image: this was reproduced on the pre-migration build. Use canonical `db` for
  image-persisted descriptions. Runtime snapshots in separate Stores still work;
  broadening Store instance persistence is a separate task.

This deliberately changes direct database writes from registration commands into
independent application data. Raw suite clones are not automatically registered.
No Store dependency is added to normal registration, discovery, running or cleanup.
Only the explicit description operation sends database messages. Current method
replacement remains observable; there are no method-identity guards, new native
owners, transactions or observer machinery. User-defined reentrant send effects
follow ordinary sequencing rather than a promised all-operation transaction.

## Validation scope

The fixture exercises actual constructors, reflection, shallow clone/alias
semantics, stable registry snapshots, idempotent registration, removal, live module
tags, pluggable runners, replaced/erroring bodies and registration during a case.
Deterministic time sends independently check calibration/sample order and exact
result JSON; a separate run exercises real clock/GC/file-output behavior.

Native retention measures raw Id allocation counts after operation frames return
and precise collection completes. A negative control omits unregistration to show
that deliberate registry ownership is visible to the check. Images must preserve
suite registry identity, deferred Blocks and shared captures, results, reflected
edits and explicit descriptions in canonical `db` with writer source removed.

The Store-absent floor exports actual method bodies, constructs prototypes with
the declared layouts, and exercises all field getters plus suite discovery,
construction, reflection and reclamation. It excludes Store, Relational, Generation
and Lang physically; it does not claim full source/FFI bootstrap without them.
Lean checks registration idempotence, removal and order under ordinary identity,
plus the existing field/clone/fact separation laws. No temporal coordination
protocol is introduced, so no new TLA+ model is needed.

Qualification passed against `/tmp/birnam-harness/prefix`:

- 826 language assertions and every existing postcheck, including the newly wired
  harness check; the standalone focused gate also runs image coverage.
- Deterministic calibration/sample order, exact JSON arithmetic, real clock/GC and
  output-path override; clone independence, custom runners, erroring/replaced bodies,
  reentrant registration, snapshot retention and explicit fact independence.
- Native post-return precise collection at 0, 1, 30 and three 100-call batches leaves
  zero extra objects and facts. Omitting unregistration deliberately fails at the
  measured one-call batch with nine extra objects; registration is intentional ownership.
- Mapped and rebuilt image execution with writer source removed preserves registry
  identity, deferred Blocks, shared Cells, result fields, reflected edits and explicit
  descriptions in canonical `db`. Independent-Store capture rejection was separately
  reproduced on the pre-migration prefix and recorded as an existing boundary.
- The Store-absent floor executes 70 exported programs, including every harness field
  getter, registration/removal, module filtering, constructors and reclamation.
- Forced source bootstrap runs the same semantic fixture successfully.
- 324 Lean theorems, 87 rejected mutations and 203 expected TLC outcomes pass.

The initial isolated-floor test accidentally applied its Logger-only checks when
only harness owners were selected; its test setup now gates those checks separately.
The initial image fixture captured a non-imageable independent Store; it now tests
persisted descriptions through canonical `db` while keeping independent runtime
snapshots in the semantic fixture. No production correction was needed after the
first implementation.

The [receipt](../measurements/test-and-bench-owners-20261016.json) records hashes,
commands, logs and the qualification boundaries. This qualified slice is committed with its implementation and evidence;
the two preceding qualified slices were committed separately. No user installation
or push was performed. No CPU speedup is claimed.
Compiler metadata/source integration remains the next structural dependency;
REPL handles, sockets and tasks still require resource-lifecycle/image-policy work.
