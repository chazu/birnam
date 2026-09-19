# 81X — String representation above Id

> **Status:** Implemented, following 81W.

Move canonical String objects, the two-word payload, address-keyed byte owner,
construction, borrowing, clone/new and default cleanup into `birnam-string`.
Ordinary consumers use an explicit live String dependency; Id's internal
consumers and retirement inspection use its admitted immutable storage contract.
Remove the three String fields from IdApi after migrating every consumer.

Unlike numeric payloads, String construction currently checks for collection
before copying native bytes and installs finalizer metadata directly. Preserve
that schedule through an immutable generic native-allocation capability: collect,
prepare private native data, allocate and associate the block, initialize private
ownership, install finalizer. Preparation and initialization cannot call Id,
collect, reenter or invoke arbitrary callbacks. They may allocate native memory.
This capability contains no String semantics and does not use replaceable
`alloc` or `setFinalizer` operations.

Borrowing consults the private owner map, never the raw pointer/length words or
current behavior. Clone ignores arity, handles null, copies owned bytes twice as
before, canonicalizes behavior and installs default cleanup rather than copying
an overwritten source finalizer. Preserve stable binary bytes, exact method
recognition, inherited Image recognition and the frozen image encoding.

Retirement may classify only the exact default cleanup callback on its owning
runtime, and only for a still-owned byte buffer. Pure ownership queries cannot
call Id or collect while RuntimeState is borrowed. Wrappers, replacements,
unowned payloads and another runtime's cleanup remain observing. Per-runtime
test instances retain this distinction and do not share native owners.

Prepare String after the collector is mounted and before other value modules and attach it before publication.
Register partial roots before creating identities. As with the numeric families,
reflection enumerates families in their module construction order; local bootstrap
fixtures now mount String separately from floor identities. Install clone/new once during
preparation; activation retains 81W's position before Collection and reinstalls
only its fourteen methods. Canonical identities and native roots survive export
rebinding. Pin the native library even if subsequent startup fails.

Validate allocation/finalizer schedules, ownership despite payload/behavior
mutation, isolated runtimes, GC during preparation, clone/new and repeated init,
live String operations, and the existing deferred-allocation cleanup oracle.
Use focused debug startup, source-free compiled module, saved native-code image
and baked executable checks. No release validation or new language behavior.

## Validation

Focused debug validation passed:

- 248 Id tests (16 ignored), including isolated String owners, byte borrowing
  after raw payload and behavior mutation, clone copying/canonicalization,
  namespace rebinding/root survival, and an automatic-collection callback that
  mutates input before its byte copy. Construction and clone bypass instrumented
  ordinary alloc/finalizer setters. The existing physical/deferred-allocation
  cleanup oracle still covers default, wrapped, replaced and wrong-runtime cleanup.
- Four ABI tests, 86 host tests (one ignored), four builder tests, four Collection
  tests, 11 Image tests and one TCC test (two ignored) pass. Workspace check is
  warning-free; String and ABI Clippy pass with warnings denied.
- The independent C String fixture passes 410 binary/index/slice cases and 18
  numeric boundaries, partial-root collection, publication reentry, owner-map
  reads despite overwritten payload words, exact cleanup ownership, clone/new,
  live operation replacement and repeated initialization. The installed prefix
  also passes Float's 676 cases, Word's 81 equality pairs, Integer's 405 cases,
  Boolean's 34 cases and nil's 30 cases, plus numeric/closure API-context checks.
- The independent v7 client passes with 34 fields. Storage observation, described
  graph tracing, actual GC traversal and nine floor-discovery cases pass. All
  nine frozen-image authority controls pass; image changes are only counted call
  substitutions. Historical IdApi manifests and fixtures remain unchanged.
- Standalone Id and String match their 33- and six-symbol inventories, relocatable
  install names and system-only linkage. Installed String, Float, Word, Boolean
  and nil match their inventories. Installed Codegen gains exactly the String
  dependency export compared with the previous prefix.
- A fresh external debug prefix contains 25 native libraries and 28 compiled
  Birnam modules. All 1,499 source hashes remain unchanged during installation.
  Actual startup, source-free compiled module loading, a saved v8 image containing
  native Code and a baked executable pass. Both image modes exercise captured
  String aliases, clone/new, slices, numeric conversion, Array splitting, live
  replacement and GC, with dynamic String loading observed.

Logs are `.cargo-target/81x-{check,unit,native,clippy,live,observation,tracing,
api-context,discovery,prefix,prefix-native,image}.log`. No release validation ran.

## Next boundary

Audit StringBuilder's existing native buffer family in Collection next. Its
allocation/finalization and Vector-coupled export guard require explicit review.
Preserve its current image limitations; extracting the implementation must not
add serialization support or alter repeated initialization. Array and Range
remain separate, larger collection-family candidates.
