# 81BK — Native GC settings

> **Status:** Audited against completed 81BJ (`69065fe8`); implementation and
> installed startup/image evidence pending. Debug validation only.

[81D](81d-id-api-partition.md) places collection policy above Id. After
[81BJ](81bj-production-root-provider-registry.md), `gc::MetadataShell` retains
the native stack anchor and two independent flags. Extract the flags and their
automatic-collection predicate into `birnam-gc-settings`. This is an ownership
move, with no new language methods, suppression policy or image representation.

## Existing laws and read points

- Each Runtime starts with automatic collection requested and precise mode off.
  Both setters first perform ordinary owner/observation admission; every nonzero
  C integer enables its flag. They have no collection-phase guard, do not collect
  immediately and do not clear or alter allocation accounting.
- Automatic collection is due exactly when its flag is enabled, the native stack
  anchor is nonzero and bytes since collection are at least `16 << 20`. Id owns
  the anchor and byte count. Ordinary allocation copies this decision before
  collection and before invoking root callbacks.
- The same predicate guards native-tier entry and the fused-numeric, discarded-
  clone, unboxed-numeric, virtual-aggregate, regions and closure-elision paths.
  Their checked/saturating arithmetic, evaluation order, budget inclusion and
  fallback decisions remain at their existing call sites.
- `runtime/tracing.rs::edges` reads precise mode separately for each ordinary
  object's payload, after emitting its vtable edge and testing whether the
  address is itself a registered vtable. It then combines the flag with that
  vtable's layout presence. Trace hooks run later and may change the flag for
  subsequent objects. Do not cache precise mode for a whole collection.
- Storage observation copies precise mode into its owned view while ordinary
  Id access is forbidden. This internal read must stay callback-free and must
  not acquire ordinary public admission. Its coverage flags remain unchanged.

Stack/register capture, the native anchor, storage/accounting, collection phases,
layout/slot selection, roots, hooks, finalizers, weak identities and executable
retirement stay with their existing owners. In particular, pending retirement
still precedes the stack-anchor snapshot in `gc_collect_active`.

## Native consumers and selection

| Consumer | Existing selection and timing |
|---|---|
| System GC `autoGc:` / `preciseGc:` | Prelude's first saved execution API, passed by its installer; preserve live Boolean/nil identities, argument checks, receiver return and original registration slots |
| Host `prepare_execution` | The loaded floor's capability, after setting the live stack anchor and only when precise CLI policy is enabled |
| Image `load_v8` | Saved execution API; disable after successful decoding, before native-unit reconstruction; its guard unconditionally enables on exit |
| Codegen, including `code_bindings.rs` | First saved execution API; preserve all fourteen disable/enable pairs, ten local guards plus the separate binding guard, and the three explicit pairs |
| Relational native-output fixture | Test-only setup; propagate the matching capability explicitly |

Codegen has thirteen pairs in `lib.rs` and one in `code_bindings.rs`. Image has
one pair. These are Boolean assignments, not nested suspension counters. Guard
destruction and early-return ordering must remain unchanged; do not restore the
previous flag or introduce a new nesting abstraction.

Native initialization receives the matching settings capability explicitly.
Prelude and Native Loader retain/forward it with their original execution
selection; System GC's installer receives that same selection. Codegen and Image
capture it at the existing Runtime initialization point. No API-address registry,
aggregate replacement table or production callback-slot loan is introduced.

## Selected ownership contract

GC Settings owns an independent allocation per Runtime containing the two flags.
An inert factory is prepared and pinned before Id initialization; ordinary
siphon module mounting publishes no objects or methods. Factory attachment
accepts the same pointer again and rejects retargeting.

The public capability contains version, extent, context and two setters. A
copied floor descriptor supplies ordinary admission. Each public setter copies
that descriptor, admits without holding a settings borrow, then assigns its
flag. Reentry during admission therefore completes before the outer assignment.
The existing nonzero-to-true conversion is retained.

The factory supplies canonical destruction, internal setters, the due predicate
and precise-mode reading at the original owned address. They invoke no floor,
provider or language callbacks and ignore instrumentation of the public
descriptor. Independent/moved Runtime instances retain their own flags without
requiring publication through the process Runtime. Id performs its existing
admission before internal setters. Internal queries work during collection and
restricted storage observation.

The due query accepts stack readiness and the already computed byte count;
native addresses and heap ownership do not move into the service. Defaults and
the exact 16 MiB law belong to GC Settings. Its descriptor/code live until every
retaining consumer and owning Runtime have finished.

IdApi v16 removes `gcSetAuto` and `gcSetPrecise`: 23 fields, 184 bytes. Native
initialization gains a ninth argument; Native Loader becomes v9 and System GC
installation becomes v2. Existing early value preparation contracts need no
settings capability. Native consumers must be rebuilt; image encoding stays v8.

## Required evidence

Archive `69065fe8` source and the completed 81BJ debug prefix before implementation.
Use their v15 headers and implementation as an independent oracle. Verify default
flags, zero/nonzero conversion, exact threshold boundaries, absent/present/reset
stack readiness, suppression without accounting reset, independent/moved Runtime
ownership, inert factory validation, admission reentry and canonical operation
independence from the mutable public descriptor.

Exercise live precise-mode changes between traced objects, layout absence,
ordinary allocation and native-tier budget consumers, storage observation's
copied flag, owner/observation rejection and mutation during collection callbacks.
Compare System GC installation/reentry transcripts and Codegen/Image suppression
call order and first execution selection, including early returns and reload.

Validate the actual installed module at siphon startup, in source-free compiled
callers, saved native-Code images and baked executables. Check ABI/exports,
dynamic ownership, stdlib and affected recovery/worker paths. Use focused debug
checks; release validation is waived. This document does not declare completion
until those observations exist.
