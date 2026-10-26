# Store removal: invariant audit before method publication

2026-10-14. Audit baseline: `cae9aa08` (v0.17.0), through `061bf5b0` plus the
corrections documented here. This reviews the delivered ownership changes before
moving the remainder of step 5. It does not implement publication migration or
assert that the whole runtime is independent of Store.

**2026-10-26 update:** the user retired universal Id method subscriptions. The
observer failure and correction below are historical evidence, not a requirement
to restore that hook. Explicit Store invalidation is retained; see the
[current decision](oop-continuation-id.md#method-observation-contract-decision).

## Findings

Two concrete defects were reproduced. One was introduced by the Module registry
migration; the other already exists in the installed 0.17.0 runtime.

| Finding | Reproduction and cause | Correction |
| --- | --- | --- |
| Module registry loses a successful reentrant write | A forwarding `Map at:put:` callback writes another field, forces GC and returns. The outer edit publishes a stale record over it. Task transitions prevent time slicing, but not synchronous reentry. The committed candidate prints `3`, `nil`, `false` after attempting left=3 while the callback writes right=2: the callback value is lost and the edit reports no Error. | Capture the registry root before construction; conditionally replace it through the ordinary `publishRegistry:from:` method. Return `#compensationConflict` on mismatch, preserving callback work without retry. Declarations stop on the first failed field edit. |
| Entered Id observer loses its receiver after unregistering itself | A native `methodChanged:on:` receiver clears Id's observer slot, collects, then continues. With no other roots, the receiver is reclaimed before return. The new regression fails on both HEAD and installed 0.17.0. | Id owns temporary notification roots for receiver, selector and vtable until its live send completes. Nested scopes restore the prior root prefix on success or native failure. Registration replacement still controls future notifications. |

These are ownership/lifetime fixes, not restrictions on method replacement. The
first retains live Map sends and reports a stale data publication. The second
retains a live observer send and imposes no Store or collector-policy dependency
on Id. Neither caches a callback address or replays user code.

The first regression covers record replacement and removal as well as field edits,
with collecting callbacks and declaration/activation error propagation. Activation
stops on a loaded-flag conflict without pretending to undo already installed code.
Earlier successful
declaration fields remain committed; declarations are not newly transactional.
The second covers both successful and failing self-removing callbacks and proves
subsequent reclamation in the native implementation test. Formal models describe
the abstract contracts, not arbitrary replacement code or the compiler.

## Review of the completed slices

| Area | Authority and meaningful inspection/edit path | Conclusion / remaining boundary |
| --- | --- | --- |
| Private scratch and runtime indexes | Local Cells or their actual functional owner; copied references reported through owner tracing | Appropriate removal of accidental database retention. Public facts were not a useful authority for parser temporaries. Root lifetimes still require adversarial collection tests. |
| Default Cells | Ordinary value slot; Cell messages and Reflect slot access | Intentional API transition: explicit fact-backed handles retain Store participation. Clones and alias behavior follow ordinary object ownership; automatic history is no longer implied. |
| Reflect inventories and slots | Id dictionary/parent messages, binding-owner snapshots and ObjectModel reference slots | Inspectability remains available without reconstructing the environment from facts. Inventories are snapshots, not atomic whole-world views or predictions of custom lookup. Foreign native payloads remain owned protocols, not writable offsets. |
| Module registry | Named ordinary slots, record/field messages, persistent Maps | Correct ownership direction; synchronous lost update required the fix above. Old saved fact-backed images remain legacy worlds; new registry clones are independent after editing. |
| LSP records | Ordinary authored state and existing server protocol | No reason to make process state a shared fact authority. Existing transport/server tests remain necessary; storing ordinary records alone does not establish image safety. |
| Globals | Authored binding storage, exact presence/value snapshots and conditional publication; namespace/source resolution precedence retained | No second native binding authority introduced. Bound nil remains distinct from absence for inventory/compensation. Reservation banks are process-local and excluded from persistence. Bootstrap seed/legacy adapter still lives in Store. |
| Method publication | Id dictionary authority, currently managed through Store | Remains unfinished. Authorization, notification, native failure and selective compensation must move together. See the new publication design. |
| Durable/coordinated runtime state | Existing metadata, Generation, Image, task and process owners | Still step 6. Do not infer their independence from the successful ordinary-record migrations. |

The earlier reports' keep/remove conclusion stands: keep Store and Relational for
explicit facts, relationships, history and self-description; remove their default
ownership of language state. The audit strengthens the need for shared owner
contracts and collecting/reentrant tests. It does not support reverting to a
mandatory database or pretending the migration is merely a storage substitution.

## Language invariants

Id remains the floor: it knows object identity, dictionaries, dispatch and its own
root inventory, not Store publication policy or Generation authorization. The new
notification roots are a caller lifetime obligation, analogous to other owner-local
scopes. They are not durable registrations or a new service dependency.

Everything added at the authored layer remains ordinary and replaceable. Data
conflict checks protect a particular conditional operation; they are not probes
for monkeypatching and do not freeze Map/ObjectModel/TaskExecutor methods. Deliberate
whole-registry replacement remains possible. Deliberately replacing a protocol
with an implementation that violates its contract is still possible too.

Removing automatic facts must not reduce inspectability or malleability. Named
owner state, binding inventories and dictionary/slot protocols are the replacement
interfaces. Explicit Store assertions remain independently meaningful. Saved
snapshots can retain values intentionally; that is different from implicitly
rooting every object's current state through a global mirror.

No new native module or language primitive is introduced by these fixes. The
Module change uses the existing conditional slot operation. Full self-hosting and
cold-bootstrap dependency removal remain targets, not delivered claims.

## Retention measurement correction

The language-only registry gate intermittently reported an object delta of 14
instead of its usual 2 (another passing process reported 6). Its fixed allowance
was not a sound reclamation boundary: `System gc` and `System liveObjects` are
allocating authored protocols, and the latter samples after additional sends.
Repeated ordinary, empty-control and longer-batch experiments showed no accumulating
state. The exact instruction responsible for the 14-object outlier was not isolated;
no runtime leak fix or larger tolerated leak budget is claimed from that observation.

The gate now additionally runs the same authored batch under a native caller,
using Id's raw counter after the batch returns and collection finishes. It loads
the actual tools startup image, including TaskExecutor, and checks OS/JSON setup.
With that production world loaded, 0, 1, 30 and three successive
100-call batches each leave **exactly zero** additional objects and zero facts.
Language object deltas remain diagnostic; zero facts are still asserted there.
The Store-absent floor separately verifies that actual discarded manager/record
identities are reclaimed. A deliberate negative control retaining clones in a
Globals Vector fails the exact counter check at the first measured one-call batch
(+8 objects). The injected control was removed after testing.

## Publication readiness

The [publication design](method-publication-owner.md) now records the current
observable ordering and the proposed receiver/selectors, compatibility path and
bootstrap/image requirements. The new native characterization establishes that
post-commit notification failures are not automatic rollback, and ordinary Error
values returned by void observers are not vetoes.

The remaining risk is the exact native/source failure boundary, followed by explicit
compatibility-description ordering and image hook lifetime/admission. Those require
small executable experiments before the owner migration. A broad source rewrite
now would be premature. The next slice should close these gates, then implement
one coherent owner change with the same test matrix.

## Verification record

Qualification results and source hashes are recorded in
[the audit receipt](../measurements/store-removal-audit-20261014.json). This is a
development-cache production rebuild and targeted/full regression qualification,
not a clean-cache release. The installed version is unchanged.

Acceptance covers 826 language assertions, 26 REPL goldens, 14 Id unit tests,
12 targeted native integration binaries plus the new exact retention test, the
36-body Store-absent floor, new/legacy registry and Globals images, and real
artifact/lifecycle checks. The formal gate checks 266 Lean theorems, rejects
61 mutations and obtains all 150 expected TLC outcomes. The final runtime's initial auxiliary
run stopped at the telemetry threshold described above; after correcting that
measurement, the registry gate and all remaining owner suites were rerun.
The receipt distinguishes the final runtime, earlier compatible checks and this
resumption instead of presenting the interrupted command as a clean pass. The executable
negative controls were run against the committed candidate and, for the Id lifetime
finding, installed 0.17.0 without modifying that installation.

Reproduction commands (use an isolated prefix for REPL mutation tests):

```sh
mise exec -- python3 scripts/check-rooting-model.py
mise exec -- python3 scripts/module-registry-test.py PREFIX --legacy-prefix LEGACY_PREFIX
mise exec -- python3 scripts/cell-without-store-test.py PREFIX --registry
BIRNAM_TEST_PREFIX=PREFIX mise exec -- bash scripts/birnam-test.sh
BIRNAM_TEST_MODULE_DIRECTORY=NATIVE_PREFIX/lib CARGO_PROFILE_DEV_PANIC=unwind \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test method_observation --test publication_contract \
  --test generation_transaction --test generation_ledger -- --include-ignored
```

The abstract checks include both task interleaving and synchronous registry reentry,
conditional publication, notification ordering and entered receiver lifetime, with
negative controls. They do not prove full bootstrap equivalence or arbitrary
callbacks. Runtime/image tests supply implementation evidence within their stated
scope; no performance improvement is claimed for these correctness fixes.
