# 81BK — Native GC settings

> **Status:** Completed against 81BJ (`69065fe8`), including installed dynamic
> startup, source-free compiled callers, saved images and baked executables.
> Debug validation only; release validation was waived.

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
checks; release validation is waived.

## Completed implementation and evidence

`birnam-gc-settings` owns the flags and predicate through GC Settings v1. Id keeps
the stack anchor, accounting and existing read/admission points. All native
initializers and consumers now carry the explicit capability; IdApi v16 removes
the two setter slots without a compatibility table. Historical ABI fixtures are
unchanged, and current fixtures describe v16, Native Loader v9 and System GC v2.

The completed debug checks are:

- Workspace/all-target checking, workspace build and affected all-feature
  checking passed. ABI, Id and GC Settings tests passed: 275 tests, with 15
  existing Id timing tests ignored. Coverage includes independent/moved Runtime
  ownership, public-descriptor instrumentation, admission reentry, live precise
  changes between traced objects and restricted storage observation.
- The focused `scripts/gc-settings-test.sh` gate passed. Five stdout/stderr/exit
  comparisons against the archived `69065fe8` implementation matched exactly:
  normal operation, both owner rejections and both observation rejections.
  Nineteen existing native-consumer transcripts also matched, including all
  three System GC installation/reentry modes. All 71 current C fixtures passed
  syntax checking.
- All 20 host factory/capability admission cases passed. The installed v16 C
  client/module probe passed, as did all 145 stdlib tests. Nine floor-discovery
  cases passed.
- The final debug installation contains 46 native libraries and 28 compiled
  Birnam modules. All 74 export inventories match the previous installation plus
  the declared changes; all 30 native manifests match. Id and GC Settings link
  only system libraries, and GC Settings uses its `@rpath` install name.
- The final installed GC Settings image suite passed dynamic startup,
  source-free compiled execution, saved v8 native-Code images and baked
  execution. It checked setter ownership, threshold admission, callback-time
  mutation, nested collection, roots, aliases and reload. The actual image-backed
  worker matched the old installation byte for byte on 12 wire-value round trips
  and orderly shutdown. An external harness waited for debug startup without
  changing the production handshake deadline.

Local receipts are under `.cargo-target/81bk-*`. The final prefix is recorded in
`81bk-final-prefix-path`; its installation source hashes are in
`81bk-final-source-hashes.json` (1,978 files unchanged during installation).
`81bk-final-{inventory,host-validation,native-audit,idapi,stdlib,gc-settings-image,worker-wire}.log`
record the final installed checks. Subsequent edits only record this evidence
and correct the native-smoke script's expected library count to 74.

Eight adjacent image suites also passed: Root Registry, System GC, Method
Observation, Native Loader, Namespace, Namespace Enumeration, Selector
Enumeration and Block Loop. These ran on the preceding debug prefix, before
correcting the new library's install name. The final prefix was rebuilt and
received the GC Settings, worker, admission, ABI, inventory and stdlib checks
above; the adjacent image results are not presented as reruns of that prefix.
