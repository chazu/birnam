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

## Piece 5: Storage diagnostics use Oops

Storage's observation protocol now carries only Oops, including allocation sizes,
identities and coverage bits. It asks the current GC owner for payload references;
the old numeric payload-kind replies and Operand/Outcome conversions are gone.
Normal and nested-observation errors remain distinct failure objects (details 1
and 2). Namespace publication and callback methods use ordinary Method objects.

The native visitor checks all report endpoints, runs collection during reporting,
attempts a nested observation, retains an expired callback, and tries it again
during a later observation. Expired callbacks fail without poisoning the new
observation. A swallowed malformed trace emission still fails the observation.
A separate process makes Namespace publication fail and verifies that reloading
cannot falsely report the half-initialized module as complete; collection remains
safe. Namespace and Storage track successful initialization separately from the
publication of their native context. All slice gates and export audits pass.

Regression inspection removes the duplicate-identity hash set by checking adjacent
sorted identities. Completed root/edge lists move into reporting instead of being
cloned; the retained allocation inventory already keeps every endpoint alive.
No generic Id hot-path work changes in this piece, so the unchanged microprobes
were not repeated. Storage throughput and full-language profiling remain pending.
[Qualification receipts](../measurements/oop-storage-20261023.json).

## Piece 6: Boolean and nil use Oops

Boolean and nil publish the same canonical identities and preserve conditional
selection, eager Boolean operators, predicates and optional printing installation.
Only the chosen block receives `value`, through current lookup. Absence remains
distinct from the language nil object. Context validation no longer dereferences
a foreign native context pointer.

Printing initially used an ordinary Sequence context. Regression inspection
showed those per-field messages were unnecessary: each printing Method now uses
its String dependency directly as its context Oop. There is no Printing box or
context wrapper allocation. Literal text Names belong to the module and are
passed to the current `fromBytes:` method. The Name owner provides `size` and
`at:` as object methods; minimal Id still does not classify payloads.

The native gate verifies identities and predicates, short-circuit behavior with
an uncallable untaken argument, GC inside the selected branch, subsequent `value`
replacement, printing text, and a captured printing Method after binding removal
and GC. Replacing the String dependency's `fromBytes:` changes the captured
Method's subsequent call. The seven migrated dylibs pass the export audit.

The broad probes change -3.1%, +1.9%, -0.8%, +1.4%; no clear broad regression is
established, and these are not Boolean-specific throughput measurements.
[Evidence](../measurements/oop-boolean-nil-20261023.json).

## Piece 7: retire owner metadata before releasing objects

Restored `registerRetirement:` for owners such as ObjectModel and Bytes. Each
registered owner receives `retire:` with the dead Oops as arguments before Id
removes its records or releases storage. Registration is idempotent, owners are
rooted, selection remains live, and no registry borrow crosses a callback. This
reuses the existing dead-object vector instead of allocating an address packet or
a temporary managed container.

The independent callback verifies every supplied object still has an allocation,
requests a nested collection (a no-op), and counts each object exactly once even
when the owner was registered twice. A separate process makes retirement fail;
storage remains allocated and further collection is refused. All native gates,
unit tests and export checks pass. The only new normal collection work is the
previously required callback for each registered owner; an empty owner list does
not allocate. [Receipts](../measurements/oop-retirement-20261023.json).

## Piece 8: activation and allocation policy

Restored inactive/active construction, activation validation and repair, allocation
preparation, budgets, automatic collection, allocation charges, external-storage
charges, precision settings and the existing native-caller `collect` operation.
Explicit `collectPrecisely` continues to use declared roots only. Incoming values
are already protected by Id; construction adds a temporary root only for its new
result across replaceable size/charge queries. Successful collection resets the
charge; a charge callback that collects adds to the new current total afterwards.

GC installs ordinary construction methods above raw Id storage, using fresh
messages. The new bootstrap Name, Sequence and native-Method constructors also
participate, with payloads initialized before accounting callbacks. Existing Name
interning remains nonallocating and allowed during collection. Failed activation
leaves construction available for repair; failed finalization remains terminal.
Readiness is distinct from publication of native context, preventing an incomplete
initialization from being reported as complete.

Added checks exercise inactive refusal, failed activation followed by owner repair,
exact geometry charges, external charges, unchanged counters when settings change,
automatic collection at `prepareAllocation`, replacement of `allocationChargeFor:`,
and collection inside that replacement while preserving the new result. Normal
and separate-target cold qualification pass, including all earlier owner, loader,
callback and failure tests. Allocation-omission guards remain part of the pending
ObjectModel/compiler migration.

Regression review removed repeated checks of the permanently rooted bootstrap
context and code-owner liveness already established by declaration lookup. Bad
contexts and undeclared code owners are still rejected before invocation. In an
identical-caller comparison of pre-activation paths, median costs fall 8.6% for
allocation/rooting, 10.9% for existing-name interning and 4.3% for fresh names.
These measurements do not include the newly restored allocation policy or GC.
The earlier probe now activates GC before its collection timer; older inactive
GC timings are therefore not used to claim a collection speedup.

Rejected a proposed shortcut that reused allocation-snapshot identities instead
of calling `identityOf:` during owner lookup. That query is replaceable; blindly
reusing a physical fact would bypass supported replacement behavior. No new cache
or side table was added to justify the shortcut.

[Measurements and receipts](../measurements/oop-allocation-policy-20261023.json).
