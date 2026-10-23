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

## Piece 9: preserve root lookup replacement

A new independent test reproduced a stack overflow after replacing the root
`lookup` method with a forwarding native implementation. Applying that method
needed lookup again, through the same replacement, before its body could execute.

Native Methods now have an ordinary bootstrap metavtable of their own. A
metavtable controls lookup for a vtable. The lookup base case follows the
self-metavtable relation, allowing additional roots to be created through normal
messages. Default lookup is optimized only when that root relation and the
current lookup binding both hold; other meta-level lookups remain observable.
The native bootstrap root and its methods remain editable objects.

The existing positive-fact cache now accommodates both bootstrap roots and shares
its invalidation with the root-relation check. This removes repeated header reads
introduced by the first fix. Binding, parent and association mutation, retirement,
and epoch-saturation checks remain covered. Added finalization coverage rejects a
detached receiver whose metavtable has already been released.

The test now replaces global lookup, sends messages and collects successfully,
restores it, constructs a third lookup root through messages, and observes that
root's replacement. All native gates pass; 19 Id and 3 Namespace unit tests pass.
Final common-path CPU changes versus `eaed1bef`: -0.6% allocation/rooting, +2.0%
existing-name interning, -0.3% fresh names. The initial fix's +7.2% existing-name
result and final measurements are both retained in
[the evidence](../measurements/oop-metacircular-lookup-20261023.json).

## Piece 10: caught errors are data; reflective inputs are Oops

A focused test exposed another consequence of treating failure as an intrinsic
allocation property: catching an error and retrieving it from a container failed
again, and `allocationAt:` could not return that allocated object as data.

Failure marking now belongs to the native return convention. `Protocol::send`
returns an ordinary error-object reference in `Err`; Rust native entry points
mark it again when propagating failure. Independent C callers use `id_error_value`
and `id_failure` for the same distinction. Allocation records contain no failure
bits or `set_reference` operation. Normal and failing views resolve to the same
object identity, and GC admits their ordinary object reference.

Native checks now catch an error, store it, collect, read its detail and allocation
identity, and rethrow the same object. Existing malformed-context, swallowed-error,
partial-initialization, retirement and finalization checks continue to pass.
Twenty-one Id and three Namespace unit tests pass.

ObjectModel prerequisites now include the ordinary Object prototype, size/Symbol
behavior queries, allocation-address/geometry queries, and object-valued binding
facts. Batched representation checks take Oop arguments rather than an address
byte packet. Binding comparisons take Method objects rather than native-entry
structs. Representation shortcuts decline when the current supporting methods
have been replaced; tests also cover a changed Method application protocol.
Symbol name hashing is performed by its owning implementation.

Regression inspection removed the canonical-reference validation made redundant
by ordinary heap references. Catching/rethrowing needs no wrapper allocation.
The failure-bit changes affect the error path; broader performance recovery
remains deferred until the language migration is complete.
[Reproduction and qualification receipts](../measurements/oop-error-data-20261023.json).

## Piece 11: allocation-omission permission uses Method objects

Restored `canOmitAllocationFor:` over current Method bindings. The collector keeps
two Method witnesses as ordinary traced references, compares the current allocation
and preparation methods, verifies default storage meanings and checks the mutation
epoch before admitting omission. It performs no allocation charge or reservation.
There is no serialized native-entry/context packet or separate binding-guard cache.

Native checks accept the default allocation path and an equivalent Method clone,
decline after replacement of raw allocation, and accept again after restoration.
All existing gates pass. Regression inspection found no new work in ordinary
allocation; this is the optional permission query used by the pending compiler
and ObjectModel migration. [Receipts](../measurements/oop-allocation-permission-20261023.json).

## Piece 12: ObjectModel uses Oop messages

ObjectModel now uses object-valued layouts, Method witnesses, allocation facts and
retirement arguments. Its ordinary-slot ownership and weak layout/owner metadata
remain local to ObjectModel. Native table references are reported by their existing
owner before ObjectModel reports its additional behavior-owner links.

Native checks cover raw/reference layouts, generic cloning while an allocating
callback changes the source and collects, private behaviors, and a replacement
Integer conversion that reenters slot access and collects. Clone snapshots retain
the original reference and full-width raw bits. Nested scopes restore the parent
root inventory. All existing native gates and export audits pass.

The new test also reproduced an inherited-layout defect: after creating a private
behavior, raw slot access could treat a declared raw-word field as an object
reference. Slot access and precise tracing now use the same inherited-layout
interpretation as image/reference-slot queries. The test rejects that read and
checks that an address stored only in a raw field does not retain an object.

Regression review removed an unnecessary table of bound Methods from the port;
only the two actual witnesses are retained. Per-element construction scopes avoid
retaining every temporary index until a slot-construction loop ends. No additional
Id hot-path bookkeeping is introduced. High-level Integer/Vector adapters and
raw-word image transport await integration with those owners; this checkpoint does
not claim their complete acceptance. [Receipts](../measurements/oop-object-model-20261023.json).

## Piece 13: Word values and expression evaluation use Oops

Word now receives and returns objects throughout. Constructed Words retain their
fresh identity and full eight-byte unsigned payload. Boundary sizes participate
in unsigned operations; full-width values remain Word objects. Explicit low/high
32-bit projections support lossless transport without sharing private payloads.
ObjectModel's raw image slots now round-trip `u64::MAX` through that protocol.

The native gate checks full-width construction/copy, wrapping arithmetic, shifts
outside the word width, division failure, guarded expression evaluation, operator
replacement/refusal and restoration, GC, and image-slot transport. The existing
machine-arithmetic unit test also passes. Expression guards compare current Method
objects and retain the epoch check before committing a result.

Regression review replaced a byte-by-byte program-reading loop with the argument
container's bulk `apply:to:with:` protocol. The container supplies one snapshot plus
additional inputs; no temporary managed input object or per-byte Id send is needed.
Replacing that protocol changes the subsequent evaluation, as the native test
verifies. Broader throughput measurement remains deferred with the compiler and
full-language integration. [Receipts](../measurements/oop-word-20261023.json).

## Piece 14: Bytes and decoded views use Oops

Bytes transfers data through object protocols. Its own buffer objects remain the
storage authority; other owners supply bytes through `appendTo:` and bulk byte
values. Names and Symbols provide that protocol without exposing borrowed native
pointers. Buffer operations and binary framing still run inside the Bytes owner.

Removed the mirrored pointer/length payload, the Bytes/TaggedView discriminator,
and empty buffer records for decoded views. Buffers and views retain their actual
owned data, and their handles have zero payload. Neither contains managed payload
references. Owner-local scoped roots replace per-scope retain/release message
loops. Lazy decoded-table construction no longer requires the unused Vector
factory; eager Vector results still use the current factory.

The native gate checks zero/255 bytes, self-append, Name/Symbol transfer and live
replacement, SHA-256 against an independent expected digest, full-width reads,
GC inside external-capacity accounting, and a decoded view after its original
buffer has been collected. Existing binary framing/hash unit tests and all prior
native gates pass. Numeric adapters are exercised with an independent object
provider; full Integer/Vector integration remains in the migration queue.
[Qualification receipts](../measurements/oop-bytes-20261023.json).

## Piece 15: String owns bytes and uses object-valued extensions

String and StringBuilder now own their native byte arrays directly. Removed the
separate Bytes backing object and payload mirror; handles have zero payload.
Byte transfer uses the provider's current `apply:to:with:` or `appendTo:` method.
Internal text construction passes an ordinary Sequence to the current String
`fromBytes:`; this preserves zero bytes, which Names deliberately reject. `bytes`
and `asBytes` call the current Bytes `fromBytes:` and return independent copies.

Numeric Methods capture a completed Sequence of Integer and Float dependencies;
split Methods capture their Vector dependency directly. Removed leaked native
extension records, permanent dependency retain tokens, and redundant copies of
incoming call roots. Only temporary results need owner-local scoped roots.
Id's incoming-call roots and Method-context tracing provide the existing lifetime
obligations. No new shared payload classification or dispatch table was added.

The native gate covers binary text, independent clones, builder self-append,
replacement and trimming, Symbol conversion, independent byte copies, signed
64-bit/IEEE bit preservation, parsing overflow, GC during external accounting and
numeric conversion, captured numeric Methods after all extension bindings are
removed, and split construction with collection during Vector calls. It checks
replacement of byte-provider application, String construction, Bytes construction,
and Vector construction. The full prior native gate and export audit pass.

Regression review retained live Bytes construction and removed redundant incoming
argument rooting. This is structural simplification, not a claimed timing win:
strict byte transfer still expands bytes into Oop arguments, and full-language
performance recovery remains deferred. Real Integer/Float/Vector integration is
still pending. [Qualification receipts](../measurements/oop-string-20261023.json).

## Piece 16: Float arithmetic uses Oops

Float keeps its eight-byte IEEE payload and exact-vtable recognition. Its native
methods receive Oop contexts and Oop arguments/results. Full-width bit transport
uses Word's public low/high projections and constructor; no foreign payload is
read. ObjectModel still allocates the storage and owns the registered raw layout.
Precise and conservative owner tracing retain their existing policies.

Removed the per-call root-token allocation/retain/release loop. Id already keeps
incoming call objects alive, and scoped roots now protect only temporary objects
across callbacks. Current Float, Integer, String and Error constructors remain
observable, as does the current Namespace ErrorVt binding. Error diagnostics stay
ordinary language objects, distinct from failed Id returns.

The native gate checks exact signed-zero, infinity, quiet/signaling NaN and
subnormal bit transport; arithmetic and mixed input; signed rounding/conversion;
zero-divide/domain/overflow objects; live constructor and ErrorVt replacement;
invalid extent/private-vtable rejection; and collection inside allocation and
numeric conversion. All prior gates pass. Integer and Error are independent C
providers here; the full-language integration and performance work remain pending.
[Qualification receipts](../measurements/oop-float-20261023.json).

## Piece 17: Integer and mixed arithmetic use ordinary Methods

Integer receives and returns Oops, preserving its eight-byte signed payload,
full-width Word transport, checked arithmetic, Error propagation and constructor
replacement. Numeric evaluation and predicates accept Method objects instead of
native-entry carriers. Retained original Methods are witnesses for optimization
assumptions, not an alternate dispatch table. Guards check current application
semantics, allocation policy, live ErrorVt and the epoch; they decline on changes.

Mixed arithmetic Methods capture Float directly as context. A completed private
evaluator Method captures Float for the existing synchronous numeric visitor path.
Removed the leaked native Mixed record and its permanent dependency root. Incoming
call objects use Id's existing roots; new boxed words and diagnostics are protected
only across callbacks that can collect them.

Regression review found an avoidable chain of up to twenty comparison messages
for copied or unknown Methods. The native Method owner now compares a supplied
candidate list in one message, reading the selected Method once, without another
registry. Canonical witness identity still needs only one application check.
The two-Method comparison uses that same implementation. Predicate validation also
checks the epoch after input reads, and respects the Oop size encoding's epoch cap.

A fresh-process integration check loads the real Integer, Float, String and Bytes
owners. It covers signed limits, overflow and zero-divide Error values, exact
signed comparison, mixed arithmetic in both directions, pre-install replacements,
copied Methods, parsing and byte indexing, inclusive iteration with collecting
callbacks, live ErrorVt, evaluator fallback with GC while boxing negative values,
and predicate refusal after constructor, application or arithmetic replacement.
All earlier gates and the independent C/TinyCC callers pass. Error is still an
independent C provider; full Execution/compiler/image/worker qualification and
performance recovery remain pending.
[Qualification receipts](../measurements/oop-integer-20261023.json).

## Piece 18: Vector uses Oops and a single scoped root list

Vector retains its persistent 32-way trie and tail. Its payload and node layouts
remain owned by Vector and are declared to ObjectModel for tracing. Vector still
delegates to CollectionVt; nodes delegate to ObjectVt. Only Oops cross messages,
including full-width index conversion and printing. Printing accumulates an owned
Bytes object through current element printing/byte messages and the current String
constructor, replacing borrowed byte-pointer transport.

Replaced per-scope root-token vectors and retain/release/cleanup sends with one
owner-local scoped reference list. Nested scopes append and restore their suffix;
reset retains just the current persistent result. Removed the now-infallible
error wrappers around keep/reset and the redundant receiver/argument copy in each
entry point. Cold initialization still retains its objects until the complete
Vector root provider can be registered.

The fresh-process integration check exercises 1,070 elements across tail and trie
growth, persistent snapshots, inherited Collection behavior, signed invalid bounds,
slices, reversal, concatenation, collection equality, both stable sorts with fresh
keys and nested Vector operations, folds, GC at every selected tree allocation,
printing with GC during byte-capacity accounting, and String split using actual
Vector construction. Review caught and fixed an initial wrong ObjectVt parent;
the inherited-method check now protects that contract. All prior native gates pass.

Broad throughput work remains deferred. The concrete removed costs are root-token
allocation and retain/release dispatch for each temporary reference; no application
speedup is claimed yet. [Qualification receipts](../measurements/oop-vector-20261023.json).
