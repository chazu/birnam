# 81BG — Object Model owns native vtable enumeration

> **Status:** Implemented and validated against completed 81BF (`5580088c`)
> using debug builds, independent native clients and dynamic/image execution.

This extracts `IdApi.vtableCount` and `vtableAt`, following the observation
boundary in [81D](81d-id-api-partition.md). Id retains its ordered vtable registry,
default dictionaries, identity, mutation epochs and GC membership. Object Model
owns native count and checked indexed selection over a transient borrowed view.
It does not forward the old Id entry points or retain a second registry.

## Observed semantics and caller audit

Both public entries select the process-published Runtime, perform existing
owner/observation admission, and read the current ordered registry. Count is
its length; out-of-range indexed reads, including `SIZE_MAX`, return null.
Neither operation allocates Birnam objects, roots entries, sends messages or
freezes a snapshot. Vtable creation appends; collection removes dead entries
while preserving survivor order. Enumeration does not keep otherwise dead
vtables alive. The private internal registry operations remain Id primitives.

Store's `backfillMirror` is the only production indexed consumer. It selects
its first saved execution API, reads count once, then reads the indexed callback
separately for each iteration. Parent-protocol and selector callbacks intervene;
they can mutate the registry or replace either enumeration callback. The fixed
initial bound and fresh per-index selection must remain. Store collects carriers
before borrowing StoreState and does not add roots during traversal. Mirror
backfill must not advance its durable basis or retention window.

Codegen's `liveVtableCount` uses its first saved execution API, returns nil for
nonzero arity without reading count, then reads count once, clamps to i64::MAX
and constructs Integer in the original order. Prelude saves enumeration with
its API solely to supply Native Loader. Direct Native Loader retains its first
execution selection and passes it to native initialization, including reentry
and later registration against a different incoming API.

ForeignAddress validation also reads count after payload validation, import and
`vtableOf`, then uses a saturating count-plus-one parent-chain bound. Its Store,
Codegen and Tcc callers use saved Runtime selections; Alien captures the
capability with its earlier API OnceLock so initialization callbacks retain
the same access. Null/size/missing-family refusals must still skip count.
Image's inheritance helper likewise uses its first saved API and a saturating
count-plus-one bound. Tcc's Unit-cache binding reads count after its arity,
payload and vtable checks, without adding one. All three read once before their
parent callbacks; none may substitute a visited set or reread during traversal.

## Explicit boundary

An immutable Object Model factory creates/destroys independently owned services.
The siphon pins that library and attaches its inert factory before `id_init`;
the ordinary object-model method installer still runs at its original point.
No initial object, selector or method installation moves earlier.

The immutable copied floor descriptor supplies a single representation borrow:
`view(context) -> { entries, count }`. Id performs the existing admission and
returns its own current ordered array. The view is valid only until the next
registry mutation; Object Model immediately copies the requested fact without
another callback, allocation, root or retained borrow. Independent Runtime
movement and teardown must remain valid.

The mutable enumeration capability contains version, extent, context, count and
at callbacks. Each call copies its selected function/context before invocation.
Callers retain the capability with the same API selection, not an API-address
registry or process-global replacement selector. Native initialization receives
it as its sixth argument; Native Loader's installer gains the matching argument.
IdApi v12 removes the two fields (28 fields / 224 bytes). Other native preparation
contracts and the image format remain unchanged. Historical v11 evidence remains
independent; all current native callers and fixtures must migrate.

## Adversarial review and completion evidence

Reject a snapshot, rooted iterator, ordinary-send replacement, or primitive
count/at forwarding facade: each changes the contract or leaves the operation
in Id. Preserve live callback selection, result nulls, owner/observation
diagnostics, callback reentry/GC, copied APIs, registration versus saved execution,
and library lifetime. Service destruction uses the original owned address even
if its public context has been replaced.

Compare native clients compiled against archived 81BF and its frozen debug
prefix with current clients using an independent v12 header. Exercise live
registry changes, bounds, GC removal and survivor ordering; replace count/at
between Store callbacks and check the mirror basis and Codegen arity/clamping.
Check inert factory admission and independent service lifetime. Verify actual
library ownership, ABI inventories, direct Native Loader handoff and host
admission. Build a fresh debug prefix from frozen runtime sources, then verify
startup through siphon, source-free compiled callers, saved native Code images
and baked executables before committing this extraction. No release validation.

### Completed checks

The fresh debug prefix was built from 1,883 frozen source hashes; all still
matched after validation. It contains the same 43 native and 28 authored
libraries. All 71 installed export inventories match the baseline except for
the declared enumeration exports and Native Loader v5 symbol changes. The
existing Id and Native Loader manifests match their actual exports. Object
Model retains its historical absolute install name.

- Workspace debug build and all-target checks passed, as did all-target,
  all-feature checks for the affected Id, host, Codegen, Tcc, Alien, Image and
  Store packages. ABI, host, Id and Object Model unit suites plus the moved
  Runtime test passed 350 tests, with 16 existing ignored tests.
- Independent 81BF/current enumeration and consumer traces match exactly.
  These cover live append, checked bounds, weak GC membership and survivor
  order; Store's changing indexed callback and fixed initial count; first-saved
  API selection; Codegen refusal/clamping; and ForeignAddress versus Tcc Unit
  parent traversal bounds. Six owner-thread, storage-observation and root
  callback outcomes also match in exit status, stdout and stderr.
- Sixteen existing native consumer traces match 81BF, including Native Loader
  reentry and reload with the sixth descriptor. Error-value admission and
  numeric-context checks passed. The focused Namespace suite and final
  `scripts/vtable-enumeration-test.sh` passed; 62 current C translation units
  passed strict compilation checks.
- The installed independent IdApi v12 client/module passed. All 16 host factory
  and capability admission cases and nine floor-discovery cases passed. The
  installed standard-library runner passed all 145 tests.
- Nine suites passed siphon startup, source-free compiled callers, saved native
  Code images and baked executables: vtable enumeration, Namespace, error import,
  error presence, Namespace enumeration, Word error values, Native Loader,
  Source Loader and Block Loop. The new enumeration image probe verifies actual
  Object Model callback ownership, live registry changes, rooted identity across
  GC and Image save's use of the explicit capability.
- Image-backed worker transcripts match 81BF byte for byte: twelve wire round
  trips followed by orderly Stop/Stopped. Debug readiness took 24.76 seconds
  for the baseline and 26.60 seconds for this extraction; this does not claim
  the full worker-service suite passes its existing 10-second ready deadline.

Local receipts are under `.cargo-target/81bg-*`; the tracked native and image
runners preserve the focused checks. Broad workspace all-feature validation
still has the previously observed artifact-codec `module` feature dependency
issue; the affected-package checks above passed. No release validation was run.
