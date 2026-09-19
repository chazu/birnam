# 81AY — Namespace owns export enumeration

> **Status:** Implemented and debug-validated, following reverse names in 81AX.

Move the private count/name-at operations into Namespace and migrate Reflect's
allGlobalNames consumer. Preserve raw insertion order, null-valued bindings,
Symbol identities, bounds, owner/storage-observation checks and selected storage
callbacks. Keep IdApi v8, language protocols and image encoding unchanged.

Reflect currently resolves count and name-at separately through its selected
floor on every call. Its shared floor handle is cached, including failed first
acquisition; individual symbol lookups remain live. Missing count skips name-at;
missing either skips Vector import and GC changes. Both selected operations are
retained across Vector import and GC-disable callbacks, count is taken once, and
name-at runs for each original index, filtering raw null names. Vector construction
and GC-enable stay at their existing points. Preserve all of this ordering.

Replace the two Id operation exports with explicit versioned data publications:
`birnam_namespace_count_v1` and `birnam_namespace_name_at_v1`. Each publication
points to an immutable operation/context descriptor owned by its Namespace
instance. The floor publishes these pointers after initialization; it implements
no enumeration operation. Reflect performs the same two live floor-symbol
lookups at the same points, snapshots each selected descriptor, and invokes its
operation at the original point. No global-loader fallback or cached descriptor
is introduced. Native replacement uses the existing live dlsym selection path.

Namespace's factory becomes v3 and creates/destroys a per-runtime enumeration
service. Creation copies opaque storage context, selected count/name-at operations
and a restricted owner/phase preflight, without calling them. Each operation
preflights and invokes the selected storage operation. Destroy the enumeration
service before state, alongside the separately owned names service. Unit Runtimes
retain independent state even when moved. Remove the old production Id wrappers;
keep private test oracles only where existing tests require them.

Verify independent raw-state cases, malformed descriptors, optional absence,
selection through copied/replaced callbacks, early returns, GC timing, count/name
snapshots across callbacks, null filtering, fresh invocation and selected-floor
lifetime. Compare native Reflect traces against 81AX using separately compiled
old/new discovery adapters. Verify dynamic startup, a source-free compiled caller,
saved native Code image and baked executable in a fresh debug installation.
Document and commit this extraction before moving on. No release validation.


## Implemented boundary

Namespace factory v3 is 112 bytes. Its enumeration service owns two immutable
32-byte operation/context descriptors and copies selected raw-storage operations
and a 32-byte floor preflight. Creation/destruction invoke no floor callbacks;
each count/name-at call performs preflight followed by the selected storage
operation. Destruction precedes state destruction. Existing independent Runtime
and raw namespace oracles remain test-only; production Id enumeration wrappers
are removed.

The two 24-byte floor data publications are initialized before id_init returns.
Reflect resolves each through its original floor_symbol helper and copies each
selected descriptor at that lookup. Its shared floor handle, live dlsym calls,
early returns, Vector import, GC transitions, fixed count, null filtering and
Vector construction remain unchanged. The descriptor publication is explicit
native ABI data, not a fallback or aggregate table. IdApi v8, its fingerprint,
and Image source/encoding are unchanged.

## Validation

- Focused check and tests passed: ABI 6; Namespace 7 with 1 diagnostic ignored;
  Id 246 with 15 ignored; host 86 with 1 ignored; builder 4. ABI/Namespace strict
  Clippy passed. The moved-Runtime regression now compares both enumeration
  operations against the independent floor test oracle as well as reverse names.
- Unit service probes verify malformed inputs, callback-free construction and
  destruction, copied floor access, exact preflight/selected-storage order,
  null names, all bounds including usize::MAX, independent contexts and survival
  after the other context is destroyed. Publication tests cover null/unpublished
  addresses, versions, extents, contexts, missing operations and retained copies.
- The independent C namespace oracle passes 2,048 ordered binding cases and
  4,096 independent raw-state cases, absent-name interning, aliases, null-valued
  slots, precise GC, roots and reload. Count/name-at spies prove that the new
  service uses the selected storage operations. Separately compiled 81AX and
  current namespace fixtures produce identical installed results.
- The native Reflect probe shares one independent behavioral oracle between old
  function discovery and new descriptor discovery. Twelve modes pass against
  installed 81AX and this extraction with identical traces. They cover real
  enumeration, both missing lookups, null/nil Vector, wrong arity, duplicate/null
  names, count/name selection changes during lookup, Vector import, GC-disable
  and count invocation, fresh selection on the next invocation, and nested
  enumeration during name lookup. Ten malformed publication/entry cases decline
  at the expected lookup without import, GC changes or operation calls.
- In each execution form, dladdr confirms that both real operations belong to
  the loaded Namespace dylib. The probe passes through dynamic startup, a
  compiled caller after its source is deleted, a saved v8 native Code image and
  a baked executable. Saved enumeration vectors retain aliases across precise
  GC. The earlier Namespace state and names image suites also pass, including
  Reflect's alternate/null reverse names, Alien/Image callbacks, roots, raw
  null versus nil, reload and separation from Globals.
- Independent IdApi v8 client/module, both Block-construction admission modes,
  all four Names consumer admission probes, host factory/capability negative
  cases and nine selected-floor discovery cases pass. Standard library:
  145 passed, zero failed.
- Exact development/installed symbol inventories and deltas pass: Id removes
  two operation exports, adds two Namespace data publications and updates its
  factory dependency to v3; Namespace changes only its factory version. All
  other installed native export sets stay unchanged. Relocatable install names,
  system-only linkage and the absence of a normal Id-to-Namespace Rust dependency
  are verified. Installed inventory remains 43 native plus 28 authored libraries.
- Fresh debug prefix:
  `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81ay-prefix.6l9t2c4q`.
  All 1,759 tracked/nonignored source hashes stayed unchanged through installation
  and execution checks; only this validation record changed afterward. No release
  validation or full install-smoke run was performed.

Receipts are `.cargo-target/81ay-{check,unit,clippy,namespace,native-audit,
host-validation,floor-discovery,stdlib,enumeration-image,namespace-images,
install}.log`, `.cargo-target/81ay-floor-discovery.json` and
`.cargo-target/81ay-source-hashes.json`.

Namespace presence, import and export remain subsequent extractions. Presence
requires auditing the existing import/intern semantics and numeric admission
operands before removing its IdApi entry.
