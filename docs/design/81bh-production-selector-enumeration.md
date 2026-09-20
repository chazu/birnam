# 81BH — Object Model owns native selector enumeration

> **Status:** Implemented and validated against completed 81BG (`0a6f3501`)
> with independent native clients and debug startup/image execution.

Following [81D](81d-id-api-partition.md), move `IdApi.vtableSelectorAt` into
Object Model's existing enumeration service. Id retains default dictionaries,
ordered keys, mutation visibility, symbol identity, weak-key reclamation and
the private lookup/mutation machinery. This adds no language operation.

## Observed boundary

The existing native entry first performs owner-thread/storage-observation
admission, then exact registry lookup, then indexed selection. An unregistered
vtable (including null, an instance or a stale identity) fails with `vtable is
not registered`, even for `SIZE_MAX`. A registered dictionary returns null past
its end. Selection neither allocates nor sends, roots, interns, walks parents,
filters weak entries or consults custom lookup. Overwriting a key preserves its
position; removal shifts successors; re-adding appends. Weak-key GC preserves
survivor order. These facts also apply to Id's internal opaque dictionaries.

Object Model will select from a transient Id-owned ordered-key view. Id admits
the access and checks exact membership before returning pointer/length; Object
Model performs the bounds check and copies the identity without an intervening
callback. No raw Rust record layout, retained borrow or second dictionary crosses
the boundary. Internal Id dictionary primitives remain private and unchanged.

## Caller audit

- Store uses its first saved execution selection. `backfillMirror` selects the
  callback anew for every direct key, after its existing type/parent traversal.
  It collects pairs before borrowing Store state, without additional roots or
  durable basis/retention changes. `removeMethod:on:` authorizes and checks
  owner first, scans direct keys until null or identity match, then retains the
  existing mirror-retract, ordinary remove send and observer order.
- Codegen's direct-presence check reads `vtableOf` first and stops on null.
  Reflect's `allMethodsOf:` retains its automatic-GC toggles, Vector import,
  globals-fact collection, direct-key order, duplicate suppression, descriptor
  callbacks and parent sends. Both use Codegen's saved execution selection.
- Image's direct-presence check reads `vtableOf`, rejects null, interns the
  requested name, then scans. It uses its first saved execution selection.
- Portable Code captures enumeration at its original Runtime installation,
  before root registration. Direct invocation-arity checks retain their indexed
  callback, symbol inspection and saturating index increment order.
- Host Config-image checks, REPL setup and worker selector admission carry the
  LoadedFloor capability explicitly with their existing API. No process-global
  selector or API-address registry is introduced.

The optional warm Block constructor compares selector callback identity before
invoking it. Its new declaration must also compare the capability context with
the independently saved canonical service selection. A changed function or
context must decline without invocation. Ordinary construction still uses the
live selection. Native declaration v5 carries fourteen comparison operands;
the compiler-facing admission export and image encoding remain unchanged.

## Interface and adversarial review

Enumeration v2 extends the existing deep module: capability adds `selector_at`
and the floor adds `selectors(context, vtable) -> ViewV1`. Native initialization
keeps six arguments, with v2 enumeration sixth; Native Loader installer becomes
v6. IdApi v13 drops its final field (27 fields / 216 bytes). Factory and Id
provider/dependency exports become v2. Historical headers/manifests stay frozen.

Keep count/index behavior from 81BG, including callers that replace the shared
context: their copied descriptors must supply callbacks appropriate to that
context. No new ordinary sends, dictionary snapshots, root loans, global
compatibility facade or callback forwarding back to the removed Id entry.
Invalid-vtable checks must precede bounds checks. Factory construction is inert,
service destruction uses its owned address and the borrowed floor is copied.

Validation will compare independent 81BG/current native traces for ordered
selection, invalid input, weak GC, live callback/context replacement and consumer
selection. Exercise warm-constructor rejection and ordinary fallback, actual
Object Model ownership, ABI inventories and host admission. Verify the resulting
debug build through siphon startup, source-free compiled callers, saved native
Code images and baked executables before committing. No release validation.

## Completion evidence

The final debug prefix was built from 1,899 frozen source hashes and publishes
the unchanged 43-native/28-authored library set. Only documentation changed
after that build. The v13 manifest and Rust descriptor agree on the six-argument
signature, v2 enumeration type and fingerprint
`95360346c9da31651a5cf2224d6c727d5e99b275865965230fa92b52a18b3889`.

- Workspace debug build and all-target checks passed, along with all-target,
  all-feature checks for Id, host, Codegen, Tcc, Alien, Image, Store and Portable
  Code. Core and affected consumer unit suites passed 395 tests, with 22 existing
  ignored tests. The final ABI metadata passed its nine tests again. The Id
  weak-symbol test now also observes removal through the Object Model service.
- Independent 81BG/current native probes have identical exit/stdout/stderr for
  ordered growth, overwrite, removal, append, bounds, GC, custom-lookup bypass,
  owner/observation admission and root callbacks. Null, ordinary instance,
  unregistered address and collected-vtable failures match at both index zero
  and SIZE_MAX. Store traces match through changing selector/context selection,
  callback GC, reload, owner refusal, missing direct keys and successful removal;
  mirror backfill preserves basis and retention.
- Both focused enumeration runners passed. Strict compilation passed 63 current
  C translation units. Sixteen existing native consumer traces match 81BG,
  including Native Loader reentry/reload; error-presence and numeric-context
  admission checks also passed.
- The installed independent IdApi v13 client/module passed. All 17 host factory
  and capability admission cases and nine floor-discovery cases passed. All 71
  Birnam library export inventories match the expected version renames; the
  affected manifests, existing installed Id wrapper exports and system-only
  dependencies agree with the actual libraries.
- Warm Block admission matches the baseline, including cold/absent-family
  refusal and copied API operands. New checks prove that changed selector
  function/context decline without invocation, ordinary construction calls a
  wrapping selector, and the floor's canonical operands survive changes to its
  public descriptor while a consumer retains an independent original copy.
- Five suites passed siphon startup, source-free compiled callers, saved native
  Code images and baked executables: selector enumeration, vtable enumeration,
  Namespace, Native Loader and Block Loop. The new probe verifies actual Object
  Model selector ownership, ordered changes, rooted identity across GC and
  Image save's live selector callback. The installed standard-library suite,
  including Reflect enumeration, passed all 145 tests.
- Actual image-backed worker transcripts match 81BG byte for byte: twelve wire
  round trips and orderly Stop/Stopped. Baseline/current debug readiness was
  28.43/26.85 seconds. This does not claim the full worker-service suite passes
  its existing ten-second ready deadline.

Local receipts are `.cargo-target/81bh-*`. The tracked native and image runners
preserve the focused checks. Historical ABI fixtures remain unchanged. The
previously documented broad workspace all-feature artifact-codec feature issue
is outside the affected-package checks above. No release validation was run.
