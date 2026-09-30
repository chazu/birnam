# 81BT — Storage-observation scope ownership

> **Status:** Completed production extraction after 81BS (`4155fa10`).
> Debug validation only; no behavior, public IdApi or image-format change.

Move the per-Runtime observation epoch, active flag and one-use permission from
`runtime::storage::ObservationState` into the existing `birnam-storage` module.
Keep runtime identity allocation, host admission, ordinary method dispatch,
physical fact construction and synchronous reset guards in Id. Storage already
validates descriptions inside protected observations; this slice supplies its
owning scope state without moving the Heap or adding collection policy.

## Existing state and exact sequence

Each Runtime starts with observation epoch zero, inactive and without a permit.
Its runtime identity comes from Id's `NEXT_RUNTIME` atomic counter, initialized
to one per loaded floor. That counter, its `fetch_add` order and the exact
`storage runtime identity exhausted` invariant remain in Id. The selected
identity remains there too and is copied into the same public view field.
Do not move this numbering authority into a shared Storage-module global.

| Site | Current behavior to preserve |
|---|---|
| `require_storage_access` | Check the owner thread first, then reject active observation with `Id access during storage observation` |
| `observe_storage` | Owner check; reject active or already-permitted entry; ordinary owner admission; reject collection phase and execution/staging/retirement conflicts; find selector; validate request; create one permit |
| Ordinary observation dispatch | Send `withStorageView:` normally on the current Vtable object, preserving replacement, lookup effects, code leases and the enclosing permit reset guard |
| `with_storage` | Check permit, active flag, collection phase, exact dispatch-read depth and entry scopes in the existing order; then validate the request |
| Accepted default entry | Consume the permit first; checked-increment the epoch; on overflow return INVALID; otherwise set active and create its reset guard |
| Visitor | Build/copy the same facts, end the RuntimeState borrow, then invoke the same visitor with its context and copied epoch |
| Return/unwind | Clear active at the existing guard drop, and clear permit at the outer guard drop, without restoring previous flag values |

Nested host observation and ordinary unpermitted sends retain BUSY. Malformed
requests do not consume an epoch. A permission that reaches the default method
is consumed before epoch-overflow refusal. Visitor failure still consumes that
accepted observation identity; neither visitor status nor cleanup rolls back
the epoch. The epoch belongs to the Runtime, not to a thread, visitor or stored
view. No snapshot may remain usable as a new authorization.

## Owned scope and internal factory

The existing Storage library will own one opaque native scope per Runtime with
only these three fields. It retains no Runtime/Heap pointer, language object,
visitor, request, view or source arrays. A moved Runtime retains its scope;
separate Runtime instances, loaded floors and threads have independent scopes.

Use an immutable 64-byte factory with version, byte size and six operations:

- `create() -> context`: initialize epoch zero, inactive, no permit.
- `destroy(context)`: free the native owner exactly once.
- `read(context, out)`: copy a 16-byte state record containing `epoch: u64`,
  `active: u32` and `permit: u32`, with flags encoded as zero or one.
- `set_active(context, enabled)`: assign the active flag.
- `set_permit(context, enabled)`: assign the permission flag.
- `advance_epoch(context, out_epoch) -> int`: checked increment, write the new
  epoch and return 1 on success; on overflow return 0 without changing state or
  output. It does not change either flag.

Both flag setters use the existing native boolean convention, nonzero means
true. They assign unconditionally and do not add a state-transition policy.
Read, flag assignment and epoch advancement allocate nothing and invoke no Id
or foreign callbacks. Creation/destruction manage only native scope storage.
All output pointers belong to the synchronous caller; none is retained. Access
is serialized by Runtime's existing owner-thread checks.

Id's wrapper projects the active/permit fields at their existing read sites and
calls the matching setter from each existing reset guard. Do not consolidate
the permission and active guards, combine admission with epoch advancement, or
cache a state snapshot across ordinary dispatch or visitor invocation. Keep the
existing local epoch value when constructing the view.

Export `birnam_storage_observation_factory_v1` from Storage and
`birnam_storage_observation_dependency_v1` from Id. Prepare and pin this factory
before Id initialization, without implicitly running Storage description
validation. Validate version, exact size and all six operations inertly; accept
the same descriptor repeatedly and reject retargeting. Update C/Rust fixture
preparation and focused build prerequisites. Storage's normal Siphon initializer
stays in its existing position; no new library is added.

## Adversarial boundary review

Storage's existing `ACTIVE` builder pointer, `NEXT_TOKEN` counter and provider
sink scopes protect description callbacks. They are separate from Runtime's
protected-observation state. Leave them unchanged. In particular, do not use a
provider token as an observation identity or share one Runtime's admission flags
with another merely because both call the same loaded module.

Keep Id's floor-specific runtime numbering unchanged; the new per-Runtime epoch
does not create a new identity domain. Do not make a visitor failure restore a
used epoch, move permission consumption after the overflow check, clear active
before the visitor, or permit Id reentry while its protected view is active.
Do not hold a native state borrow across callbacks or introduce a callback into
any factory operation.

The raw storage view, allocation identities/extents, roots, provider order,
strong edges, deferred counts, coverage gaps and precise-mode snapshot are
unchanged. No settlement, tracing or collection is introduced. Public storage
ABI v1, IdApi v20, twelve-argument native initialization, Native Loader v12 and
image v8 remain unchanged. Restored runtimes retain the existing fresh-runtime
observation semantics. The installed library count stays 80.

## Independent proof and completion gates

Preserve completed 81BS source and its installed debug prefix before migration.
Run the same new observation witness against old and new runtimes. Compare
runtime/observation identities and status/visitor transcripts for independent
and moved owners, repeated accepted observations, malformed requests, visitor
failure, direct unpermitted invocation, nested host entry, live replacement of
`withStorageView:` and reset during Rust unwind.

Unit-test epoch advancement to `u64::MAX`, overflow without state/output changes,
independent owners and exact flag assignment. Separately exercise the actual
Id refusal path to prove permission is consumed before advancement fails and
the visitor is never invoked; distinguish injected advancement refusal from the
module's own near-maximum arithmetic proof. Preserve both existing fail-stop
diagnostics and their precedence with owner-thread and active-observation cases.

Capture actual scope creation and transitions through a loaded factory; verify
state allocation occurs for each Runtime and not during inert attachment. Run
actual host missing/invalid factory admission. Retain the independent storage
observation, description and tracing fixtures, provider reentry/stale-token
checks, GC phase restrictions and original physical/logical storage counts.
Verify dynamic Siphon startup, source-free compiled callers, mapped/rebuilt
saved native Code images, baked execution, aliases, module reload and actual
image-backed worker parity. Check unchanged descriptor/source bodies, exports,
linkage, admission contracts and final source/artifact receipts. Add a focused
gate, ABI reference, README links and changelog; commit the completed extraction
before selecting another boundary.

## Completed implementation and evidence

The native Storage scope, immutable factory, Id adapter, separate reset guards
and pre-initialization host attachment are implemented. Runtime identity remains
in Id, and the existing Storage description-provider state is unchanged. The
export manifests and shared C/Rust fixture preparation include the dependency;
focused shell builds now include Storage where required.
Final source comparison confirms the physical-fact builder, public observation
method and Storage description implementation are byte-for-byte unchanged.

Completed debug checks:

- ABI, Id and Storage library tests: 299 passed, 15 ignored, none failed.
- The three Runtime scope tests also pass against the archived 81BS production
  source, with test-only adapters for its original Cell fields. All 13 baseline
  and current observation transcript lines match, including runtime identities.
- `scripts/storage-observation-scope-test.sh` passes: six focused Rust tests,
  the workspace debug build, loaded-factory transitions and advancement refusal,
  both exact fail-stop diagnostics, and all 14 host admission cases.
- The 36 existing shell-script changes only add the Storage build prerequisite;
  their shell syntax and the new focused script's syntax pass.

- All-target workspace check passed. The focused scope gate also passed against
  the installed debug libraries, including all 14 actual host admission cases.
- Independent C storage-observation and storage-tracing fixtures produce exactly
  matching output against preserved 81BS and current libraries. They check
  extents, allocation identities, behavior reassociation, provider descriptions,
  coverage gaps, stale sinks, cleanup and 13 forbidden Id mutations, plus exact
  graphs, cycles, roots, raw payloads and malformed graph rejection.
- `scripts/storage-observation-scope-image-smoke.sh PREFIX` passed real Siphon
  startup, source-free compiled callers, mapped/rebuilt saved native Code images
  and baked execution. The source and saver dylib are removed before image
  execution, and the independent image reader requires a v8 native Code record.
  The native consumer finds Storage already loaded, verifies operation ownership
  and independent contexts, exercises root callbacks and automatic collection,
  and preserves arithmetic, aliases and module reload.
- A separate real-host consumer successfully observes storage before execution
  and after source, compiled, mapped, rebuilt and baked-image execution returns
  to an idle boundary. It requires mapped/rebuilt outcomes explicitly. Every run
  verifies epochs 1 through 5, malformed-request refusal, nested BUSY, a visitor
  returning -77, subsequent guard cleanup, aliases and stable allocation identity
  after GC. Native-method and collection-callback rejection are checked
  separately; those refusals are not used as evidence of successful observation.
- Old/new actual image-backed workers passed twelve wire-v1 value round trips
  and orderly stop, with byte-for-byte equal complete transcripts. The external
  debug harness did not change production handshake deadlines.
- Collection Phase and Allocation Budget regression gates passed, including
  their exact failure diagnostics and 12/14 host admission cases with the new
  Storage prerequisite.
- Installation contains the same 52 native libraries and 28 compiled modules.
  All 80 export sets match the baseline plus the two declared factory/dependency
  symbols. All 36 native manifests match; Id and Storage link only system
  libraries. Installed admission contracts match exactly, including IdApi v20,
  the runtime ABI hash and image v8. Native initialization remains twelve
  arguments and Native Loader remains v12.
- All 493 current source inputs and 531 installed artifacts match their receipts.
  Source digest:
  `6d801c105f4a7b47d842acb3f334b91573ec97f9b871c6ba2a566d276a2b1982`.
  Artifact digest:
  `3893d65bf420ae98bbf9e7984c85c548537c085072191728dd04c22bff7345e4`.

The [v1 reference](https://github.com/chazu/birnam/blob/cffba722/abi/storage-observation-scope-v1.md), independent C
declaration, `mise run storage-observation-scope`, README links and changelog
accompany the implementation. Local evidence and prefix markers are under
`.cargo-target/81bt-*`. No release validation was run.
