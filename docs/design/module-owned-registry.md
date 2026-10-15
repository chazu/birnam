# Module owns its registry

2026-10-14. Follows [slot reflection and Module directory](reflect-slots-and-module-state.md),
committed as `677ca5b6` before this slice.

## Storage and public interface

Module has ordinary `#libDir` and `#registry` slots. The registry is a persistent
Map from module-name Symbols to persistent field Maps. The familiar field names
remain `#registered`, `#kind`, `#requires`, and `#loaded`. No native registry,
implicit fact mirror, new image family or foreign state service is introduced.

| Message | Meaning |
| --- | --- |
| `Module registry`, `Module registry: records` | Read or replace the complete persistent registry value. The writer answers Module. |
| `Module recordNames` | Snapshot of all stored keys in Map order, including partial or nil records; not merely registered modules. |
| `Module recordOf: name` | Current field Map, or nil for absence/bound nil. |
| `Module record: name put: record` | Replace one complete record; preserve other records; answer Module or Error. |
| `Module field: field of: name` | Read the current field, or nil when no record/field exists. |
| `Module field: field of: name put: value` | Update one field, creating its record if absent; preserve other fields and answer Module or Error. |
| `Module removeRecord: name` | Remove bookkeeping only; does not unload code. Answers Module or Error. |
| `Module publishRegistry: next from: expected` | Conditionally publish an edited root; answer Module or `#compensationConflict`. Ordinary override point for conditional edits. |

`Reflect slotsOf: Module` discovers the registry, and
`Reflect slot: #registry in: Module` reads the same value. Maps expose their keys,
values and presence independently of getters' nil/default rules. Saved Maps are
snapshots, not writable aliases; install an edited Map through the owner to make
it current. `registry:` is an explicit whole-value replacement, not a merge.
Snapshots preserve Map structure and value references, not deep-frozen copies
of mutable objects or method definitions. A clone initially shares the persistent registry value but subsequent writes to
its own slot are independent. Cloning a manager does not clone the code world;
artifact code still runs against current Globals. Loading idempotency follows
the selected manager's registry, not an invisible global flag table. This deliberately replaces the former global-db
sharing between cloned managers with ordinary object ownership. Explicitly
re-evaluating the manager initializer likewise creates a fresh registry; saved
images retain their actual owner instead of running that initializer. A caller
replacing a manager can explicitly transfer a retained registry value.

Existing `named:requires:`, `kernel:`, `requiresOf:`, `kindOf:`, `isRegistered:`,
`isLoaded:` and `markLoaded:` remain ordinary methods. A declaration writes only
registered/kind/requires and preserves loaded state and extensions. A kernel
registration writes registered/kind/loaded and preserves existing requires,
matching the old sequence. Marking loaded alone does not register a module.
Nil requirements still read as an empty Vector; nil flags still read as false;
non-nil flag values are returned unchanged, including false.

Module's methods no longer read or write `db`. Explicit facts using these names
remain independent Store data, and `Reflect factsOf:` retains its fact meaning.
The old fact writes and automatic history are not live edits of a new registry.
Use the owner/Map interface or explicitly export a description. Old images keep
their saved fact-backed Module methods and state; no load-time reshaping or
silent fact conversion is performed. Store and Relational self-description stays
in place. Globals, publication, compiler metadata and the task executor's current
storage are still separate Store dependencies.

## Mutation and scheduling

A persistent Map edit reads an old value and publishes a new one across several
sends. Unlike a native Store field write, these sends can be interrupted by task
time slicing. `registryMutation:` therefore uses the existing, replaceable
`TaskExecutor transition:` policy when an executor is present. Before that binding
exists, bootstrap runs the body directly on its owner thread. This introduces no
native scheduler lock, frozen method or guard against monkeypatching.

Field updates protect both the record read and publication; whole-record edits
protect registry read/publication. Nested transitions are supported by the
existing executor. A declaration's three field writes remain separate operations,
as before; this is not atomic module activation and does not roll back a failed
load. The loader's planning, ordering and already-loaded checks are unchanged; activation
also propagates bookkeeping failure as described in the correction below. Reads and traversal remain ordinary resumable sends.

The internal transition cannot wait or yield through the supplied executor;
replacement code must obey that existing bookkeeping contract, or replace the
ordinary policy deliberately. It is not a sandbox or a retry protocol for
arbitrary reentrant Map callbacks. No callback is replayed to repair a stale
snapshot. Map-operation Errors are returned before publishing a replacement,
so a failed edit leaves the existing registry intact; an Error intentionally
stored as a field value is still valid data. Transition depth is restored on an
ordinary Error return. Explicit record or registry replacement intentionally overwrites the
selected value; clients needing an edit should use field mutation inside its
owner rather than publish an old snapshot as if it were a merge.

### Synchronous reentry correction (2026-10-14)

The original task transition did not prevent synchronous reentry through Map
construction. A forwarding `Map at:put:` callback could change a different field
and have that successful change overwritten by the outer edit. The audit reproduced
this on the committed registry implementation; it was a regression, not an
intentional consequence of making state ordinary.

Edits now capture the persistent registry root before construction and send the
ordinary `publishRegistry:from:` protocol. Its default uses ObjectModel's existing
conditional slot replacement: publish only when the current root is the captured
root. Otherwise answer `#compensationConflict`, preserve the callback's state and
do not retry. This is a data publication condition, not a method identity guard.
Replaced Map methods still run normally. The helper is itself replaceable.
Explicit `registry:` remains unconditional whole-value replacement; it is no
longer the hook used by conditional record edits. A client customizing conditional
publication should override `publishRegistry:from:` instead.

`named:requires:` and `kernel:` stop at the first field-edit Error. Earlier successful
field writes remain committed; the declaration is not an all-or-nothing transaction.
`markLoaded:` returns its edit result; `activate:` propagates that result and stops
the remaining plan. Already installed code is not rolled back when bookkeeping
fails. Error objects stored as values remain data.
The task transition still prevents time-sliced lost updates. Neither mechanism
promises correct behavior from arbitrary replacement implementations that discard
these contracts.

The collecting reentry regression covers field edits, record replacement, removal,
named/kernel declarations, markLoaded and activation, including no replay and no
false success.
`RegistryReentry.tla` separately models synchronous callbacks, with overwrite,
swallowed-conflict and replay negative controls; the original ModuleRegistry model
continues to cover task interleaving. Lean additionally checks conditional root
publication and conflict preservation. The source-free floor now exports 36 bodies,
including the new authored helper. See the [audit](store-removal-audit.md) for current
qualification and the subsequent exact native retention gate; the historical
acceptance below records the original slice. Language-level live-object deltas
include allocating telemetry and are now diagnostic, while the native post-return
check requires zero additional objects.

## Required evidence

Check ordinary and nil/false/partial records, field preservation, snapshots,
independent clones, generic Reflect edits, explicit facts remaining independent,
zero fact growth and reclamation. Exercise DAG order, cycles, reentrant reads,
live changes, idempotent activation, inspection failure before activation and
one-step executor quanta. Test new and legacy source-free images and relocation.
Export actual authored registry bodies into the Store-absent floor as with Cells;
this is operational independence, not Store-free full module activation.

Lean models field/record ownership and the old getter results under canonical
Map protocols. TLA+ models the read/publish scheduling boundary and rejects an
unprotected lost update. Existing image, publication, GC and task protocols
retain their independent implementation tests and stated proof limits.

## Acceptance and remaining boundary

[Source-bound evidence](../measurements/module-owned-registry-20261014.json).
The preceding slot-reflection slice was committed as `677ca5b6`; this registry
work follows it without changing native runtime code or ABI.

Passed on the final uninstrumented candidate:

- 826 language assertions and the scratch, Reflect, code-inspector, Cell, slot
  and registry retention gates.
- Source/module-loader and ordinary ObjectModel native integration checks.
- The actual 35 exported Cell, Reflect and Module method bodies on the small floor
  with Store, Relational, Bootstrap, Lang and Generation absent. The optional task
  executor is bound to nil to model the normal Globals missing-name result;
  no scheduler or fake database is supplied. This establishes basic registry,
  traversal and GC behavior, not complete Store-free activation or all Error paths.
- Source-free new and legacy registry images, mapped and rebuilt, preserving
  their own saved ownership/alias behavior, edits and directory rebasing.
- Compiled artifact admission, dependency order, repeat loading, preflight and
  failure checks; relocated Host/CLI acceptance; all 26 REPL goldens.

Store and Relational compatibility also passed on a copy of the final candidate
with TCC resource metrics enabled, as the native Store lifetime test requires.
An initial ordinary-TCC attempt could not supply that metric and is not counted
as acceptance. No production performance claim uses that diagnostic prefix.

The final formal gate checks 246 Lean theorems, rejects 55 mutations, and obtains
all 127 expected TLC outcomes. The models are abstractions of ownership and a
bounded two-task update protocol, not verification of the compiler, arbitrary
replacement callbacks or the entire executor.

Two executable negative controls distinguish the obligations. Replacing
`registryMutation:` with a direct block call loses updates under one-step task
scheduling; the normal implementation passes. The initial candidate published
an Error returned by a Map edit as the registry itself. The final candidate
returns that same Error and retains the previous registry. Tests also check
transition cleanup on this failure and Error objects intentionally stored as data.
Neither check freezes or silently repairs replacement methods.

Across 0, 1, 30 and 100 host-side operation batches, registry creation, editing,
traversal, reflection and removal create zero facts and leave the same two-object
post-GC delta as the zero-call control. Task scheduling still uses its existing
Store-backed bookkeeping when present; that is not registry fact publication.
No CPU speedup or allocation-churn equivalence is claimed.

The build regenerated production artifacts and startup images with the development
Cargo cache. This is not clean-cache release qualification. The installed user
version was not modified. Core bootstrap still depends on Store for Globals,
publication, compiler metadata and other owners; the remaining work follows the
[agreed sequence](optional-store-and-relational.md#migration-sequence).

Reproduction:

```sh
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
mise exec -- python3 scripts/module-registry-test.py PREFIX --legacy-prefix PREVIOUS_PREFIX
mise exec -- python3 scripts/cell-without-store-test.py PREFIX --registry
python3 scripts/artifact-cli-test.py PREFIX
BIRNAM_PREFIX=PREFIX bash scripts/repl-golden.sh
mise exec -- python3 scripts/check-rooting-model.py
```
