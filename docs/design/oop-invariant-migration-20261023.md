# Completing the Oop-only invariant migration

The working branch is `codex/oop-only-id-slice`. The installed runtime is not
changed. Each completed piece is tested and committed separately. After each
piece, inspect new work and duplication; fix obvious regressions immediately.
Broader performance recovery follows the completed migration.

## Remaining acceptance work

- Finish ownership boundaries between generic Id and bootstrap payload owners.
- Convert remaining native modules, including Storage, to ordinary Oop messages.
- Migrate generated calls and execution together, retaining stackless frames.
- Qualify behavior changes, entered calls, owner replacement and GC together.
- Migrate images, code lifetime, host and workers; run their existing acceptance
  oracles before declaring the complete language migrated.
- Remove obsolete carrier code, perform clean-build/installed-candidate checks,
  and then profile Logue and recover remaining performance without weakening
  malleability or reintroducing a payload catalog.

## Piece 1: argument containers own argument application

`invokeOn:arguments:` now asks the supplied container to `apply:to:`. Likewise,
`loadNative:` asks the supplied container to `load:`, which invokes the Name
owner's `loadNative` method with its arguments. Id does not read another owner's
argument-array layout. The bootstrap Sequence implementation snapshots its own
fields once, protecting entered arguments from later mutation. A different
container can implement these messages using a different representation.

The independent C fixture implements an argument container with zero payload
bytes. Invocation through it collects successfully, and replacing its `apply:to:`
binding changes the next invocation. Existing source-mutation and entered-call
GC tests still pass. All 17 Id unit tests and the full native slice gate pass.

Regression inspection found `at:put:` copied and validated the entire array for
every single-field write. It now validates the extent, the selected old field and
its replacement, with no whole-array allocation or traversal. Application builds
its receiver/argument snapshot in one vector rather than copying through two.

Repeated ABBA CPU medians against `f0f965ed`: allocation/rooting +1.7%, existing
Name interning -2.6%, fresh Name production -4.0%, collection -1.0%. These short
microprobes show no broad regression; they do not measure the changed single-field
write or explicit-invocation path, and are not a Logue result. Raw trials are in
[the evidence](../measurements/oop-argument-protocol-20261023.json).

## Piece 2: native module ownership and live construction

Native image records are private to `native_modules.rs`. Generic Id root listing
no longer reads their fields. A rooted loader object and its registered reference
owner report the resident images' contents through the same GC protocol as other
owners. Images still remain resident, including after partial publication.

After bootstrap, literal Names are constructed through current `nameWithBytes`
lookup and native Methods through the module's current `newMethodAt:` method.
The independent fixture replaces both constructors, collects inside each one,
forwards to the captured originals and checks that all five Names and the Method
from a partially failing module use those replacements. The published Method
remains callable after collection. The complete slice gate and 17 unit tests pass.

Regression inspection removed the scan of all code declarations when cloning a
Method or importing a method whose declaration index is already known. The index
is checked directly. No pointer is inferred from arbitrary payload bytes. Paired
CPU medians against piece 1: allocation/rooting -1.8%, existing-name interning
0.0%, fresh-name production -0.1%, collection +1.3%. No clear broad regression;
startup factory costs are not included in these timed workloads. See
[raw trials and checks](../measurements/oop-module-owners-20261023.json).

## Piece 3: Namespace uses Oops

Namespace no longer imports or sends Operand, Outcome or NativeEntry. Its existing
ordered binding storage and reverse-alias rules are retained. Missing bindings
return the bootstrap nil reference; `valueAt:` and source-global resolution retain
their separate language-nil handling and current dependency lookup. Counts and
indices are immediate Oops, and canonical lookup recognition compares a retained
Method object rather than a native entry/context pair. This is conservative for
cloned methods, so it does not authorize a false optimization.

The new `isSymbol:` method belongs to Id's Symbol implementation; Namespace asks
it to validate keys without reading Symbol payloads. Key validation completes
before borrowing Namespace storage, allowing validation methods to reenter.

Three Namespace storage tests pass (the timing probe remains explicitly ignored),
along with 17 Id tests and the native gate. Added native checks cover reverse alias
order, invalid keys, live `at:` replacement, absent-value handling, and enumeration
that removes every binding and collects inside every callback. Snapshot values
survive; subsequent live-only enumeration reports no removed values.

The gate now audits actual exported symbols for all four migrated libraries;
stale v1 export manifests were corrected. Regression inspection retains existing
ordered storage/indexes and avoids additional per-lookup managed allocations.
Broad microprobe changes are -1.7%, -0.4%, -0.1%, +1.5% in the four existing
workloads; these do not measure Namespace throughput. See
[raw evidence](../measurements/oop-namespace-20261023.json).

## Piece 4: preserve live GC owner changes

Comparison with the existing collector found that the initial Oop slice omitted
registration's marking rule. An owner installed from a trace callback must itself
be marked immediately; otherwise the earlier root snapshot can miss it. Restored
that rule, rejection of dead-owner resurrection during finalization, removal of
owners by nil registration, validation of trace targets as vtables, and refusal
of collector operations after failed finalization. No new per-object type table
or additional tracing pass is introduced.

`referenceOwnerOf:` exposes the existing current/inherited lookup for Storage.
The independent test installs an otherwise unrooted replacement owner inside a
trace callback, then checks its survival and the child's survival. It changes the
object to an inherited behavior, removes the owner, verifies refusal before
sweeping, restores it and successfully collects again. All previous native checks
and 20 unit tests (17 Id, 3 Namespace) pass.

Regression inspection: added work occurs during owner registration; ordinary
marking uses the same algorithm. Paired CPU changes are -0.4%, +0.5%, -3.2%, +0.9%
for the existing four probes. [Evidence](../measurements/oop-gc-owners-20261023.json).
The original permission for live owner changes is preserved rather than replaced
with a blanket prohibition on mutations during callbacks.
