# 81AZ — Namespace owns interned lookup

> **Status:** Implemented and debug-validated, following enumeration in 81AY.

Move the canonical intern-then-find operation from Runtime into Namespace.
Symbol interning remains in Id, as required by 81D. Runtime supplies its existing
restricted interner, transient state pointer and selected raw find operation;
Namespace calls the interner once, stops on embedded-NUL rejection, otherwise
returns the selected binding. Missing names still become Symbols. A missing or
raw-null binding is a successful lookup with a null value, distinct from an
invalid name. Empty and non-UTF-8 names retain their existing behavior.

This is a prerequisite below the live `import`/`isExported` handoff. The audit
found that Error Value receives the caller's mutable IdApi on each call, and
registration/reinitialization paths can select a table distinct from the saved
execution table. A module-global presence replacement would lose that selection.
Do not fold those consumers into this extraction: IdApi v8, its live slots,
numeric identity operands and the two-argument module initializer stay intact.
No replacement aggregate table is introduced. Public capability migration is
still outstanding; this checkpoint moves the primitive policy and its actual
Runtime consumer, not the public fields.

Namespace factory v4 adds one required operation, `lookup`, at offset 112 in a
120-byte immutable descriptor. There is no new service state or global registry.
It receives the selected `find` explicitly so instrumented storage is respected.
Both callbacks are restricted primitives: no user code, sends, collection or
reentry. Lookup retains no bytes, contexts, Oops or output address. Runtime copies
selection before calling it, leaving no RuntimeState/Environment reference alive
across the callback that mutably borrows RuntimeState. Moved and independent
Runtimes therefore require no captured self pointer.

Keep owner/observation checks and C-string validation at their existing points.
Use the unchanged interner so weak dictionary mirrors, compatibility limits,
allocation, symbol identity and errors retain their behavior. Preserve the raw
Symbol-only find used by callback-free numeric admission. Export and its weak
environment mirror remain subsequent work. Namespace remains separate from Globals.

Validate selected callbacks and ordering, rejection without find/output mutation,
absent-name interning, independent Runtimes, null/nil bindings, live API callback
replacement, numeric admission, roots and reload. Compare separately compiled
historical v3 and current v4 clients. Verify startup through siphon, source-free
compiled caller, saved native Code image and baked executable in a fresh debug
prefix; do not run release validation.


## Validation

- Focused checks and unit tests pass: ABI 6; Namespace 9 with 1 ignored;
  Id 247 with 15 ignored; host 86 with 1 ignored; builder 4. ABI/Namespace
  strict Clippy passes. New tests cover borrowed byte/extent forwarding,
  interner-before-find order, rejection without output mutation, null bindings,
  independent contexts and moved Runtimes. Embedded NUL creates no Symbol;
  missing, empty and non-UTF-8 names preserve interning and identity.
- The independent native oracle passes 2,048 ordered binding cases and 4,096
  independent storage cases, plus the new lookup ABI checks. Wrapped factory
  operations prove Runtime uses the selected lookup and raw find. Replacing
  live IdApi intern/import slots with aborting callbacks does not intercept
  canonical import/presence. Absent-name interning, roots, precise GC, aliases,
  null/nil distinctions and reload remain intact. Separately compiled historical
  v3 and current v4 fixtures produce identical oracle output.
- Error Value passes 114 recognition/construction cases; Integer error policy
  passes 395 cases and the Integer module passes 405. Their copied API, live
  callback, ordering, reentry, GC and reload checks pass. Numeric admission
  still compares all original operands and recovers after restoration.
- Host negative cases reject missing/invalid factories, rejected attachment and
  missing/invalid names services. The C admission probe rejects every missing
  factory operation, including lookup, and retargeting. The independent IdApi v8
  client, both Block admission modes, four Names consumers and nine selected-floor
  discovery cases pass. Standard library: 145 passed, zero failed.
- Dynamic startup through the installed host, source-free compiled caller,
  saved v8 native Code image and baked executable pass. The native probe confirms
  that lookup code belongs to the loaded Namespace dylib. Names and enumeration
  image suites also pass, retaining live replacement, discovery snapshots,
  missing/malformed results, reentry, aliases and precise GC.
- Exact native export deltas are only the Namespace factory/dependency v3-to-v4
  rename. Normal Id and Namespace inventories, relocatable install names,
  system-only linkage and absence of a normal Rust Id-to-Namespace dependency
  pass. The first development inventory observed Artifact's module wrapper from
  feature unification; the final inventory follows a standalone Id build.
- Fresh debug prefix:
  `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81az-prefix.58svsygo`.
  It retains 43 native plus 28 authored libraries. All 1,763 source hashes stayed
  unchanged through installation and checks; only this validation document
  changed afterward. No release validation or full install-smoke run was performed.

Receipts are `.cargo-target/81az-{check,unit,builder,clippy,namespace,
integer-errors,error-value,host-validation,native-audit,inventory,bare-id-build,
floor-discovery,stdlib,namespace-image,names-image,enumeration-image,install}.log`,
the floor-discovery JSON and the source-hash manifest.

## Next boundary

Migrate the live public namespace capabilities and their callers. Preserve the
per-call API choice in Error Value, registration versus saved execution context,
direct Native Loader forwarding and callback-free numeric admission. Removing
isExported must not replace these with a shared implicit selection. Import/export
field removal, including the weak environment mirror on binding, remains work
beyond this primitive extraction.
