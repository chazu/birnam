# Remaining Zig-module migration plan

Status: execution plan for the `rust-libid-floor` branch. This document starts
after the Rust IdApi floor and the opt-in Rust PEG, Lang, TCC, Codegen,
artifact, Store, and Prelude slices have landed. It is deliberately narrower
than Design 62: it describes the Zig modules that still own production
behavior and the order in which their Rust replacements can become canonical.

The goal is not a mechanical translation. Each replacement is a separately
loadable dylib, preserves the frozen IdApi and generated-C ABIs, keeps the
object dictionary authoritative with Store as its derived mirror, and remains selectable beside its
Zig oracle until an observation window closes.

## 1. Current boundary and remaining owners

The current repository already contains Rust implementations for:

- the `libid` floor and public IdApi;
- Store/MOP and its Codegen observer/authorizer seam;
- PEG, Lang, TCC, Codegen runtime, artifact inspection, and the image codec;
- the canonical N7.1 Prelude protocol through arithmetic, Boolean, Error, Block,
  System I/O/environment/output, GC controls, source `load:`, `loadModule:`,
  and `readLine`. Its Error behavior delegates to the standalone Error rung;
  it does not create an Error family. The Zig Prelude remains a pinned,
  explicit rollback fixture only.

The remaining Zig-owned production modules are:

| Zig owner | Rust work still required | First safe cutover |
| --- | --- | --- |
| `src/id/prelude.zig` | retain the Zig implementation as an explicit rollback fixture; Rust Prelude is now canonical while preserving the Error dylib boundary | R0.5 post-switch observation |
| `src/id/collection.zig` | persistent Vector/RRB, Range, Map/HAMT, String/native collection protocol, layouts and tracing | after collection cross-floor and GC gates |
| `src/id/error.zig` | standalone Rust Error dylib with the existing family/factory/DNU boundary; Prelude imports it lazily | after Error identity/DNU and source bootstrap gates |
| `src/id/isolate.zig` | process launch, worker loop, pipes, wire-v1 codec, bounded copied values, termination | after wire corpus and hostile-child gates |
| `src/id/relational.zig` | query engine, rule evaluation, pull, aggregation, time windows, Store integration | after collection and query-performance gates |
| `src/id/alien.zig` | dynamic-library ownership, C layout/signature handling, TCC shims, callbacks, buffers and finalizers | after ABI corpus and callback lifetime gates |
| `src/id/image.zig` | host orchestration around the Rust codec: mapping, swizzling, code records, Store replay, rebuild and save | after compatible-v4 cross-product gates |
| `src/id/main.zig`, project/build helpers | Rust host, source/bootstrap workflow, install and distribution closure | last, after every lower rung is observable |

The artifact boundary has three deliberately separate owners: the
language-neutral schema/parser in `birnam-artifact`, the pointer-free dynamic
inspector rung, and the host-owned closure/preflight walker. The Rust artifact
crate is the schema/inspector replacement candidate; activation and dependency
closure policy remain host-owned until the final host phase. No artifact crate
may become a second preflight authority.

The Rust Prelude owns the canonical prelude selectors but not the production
Error family. It imports `Error`/`ErrorVt` lazily, exactly as the Zig Prelude
does. A separate Rust Error dylib is required before the Zig Error dylib can be
retired; the current Rust Prelude therefore remains compatible with the
standalone Zig Error owner.

## 2. Non-negotiable invariants

Every phase below inherits these rules.

1. **One floor and one owner.** A process selects exactly one `libid` floor.
   A singleton-owning rung has exactly one active implementation and one
   identity/digest; duplicate initialization aborts or fails closed.
2. **Dynamic-library boundary.** Production rungs remain dynamically loaded
   libraries with `module_init`; no Rust module is linked into the host as a
   hidden second authority.
3. **Frozen ABI.** `IdApi`, Oop representation, vtable layouts, slot-kind
   descriptors, generated-C shims, compiled-unit descriptors, image-v4
   framing, and isolate wire-v1 framing do not change in this migration.
4. **Canonical object dictionary.** Exported objects and vtables are looked up
   by canonical name. A reload extends the existing family; it never mints a
   competing family or singleton.
5. **MOP consistency law.** The live object dictionary is authoritative for
   dispatch. Store is a derived MOP mirror and the supported post-bootstrap
   semantic mutation chokepoint.

   For every direct `(vtable, selector)` binding, the mirror is exactly one of:

   - the exact live Method for a tracked binding;
   - `#primitive` for an unreified direct binding found by backfill; or
   - absent.

   With no direct binding it is absent. It never names a different method, and
   `#primitive` never survives replacement or removal of the direct binding.
   Tracked replacement remains `retract mirror → mutate dictionary → assert
   exact mirror`; tracked removal remains `retract mirror → remove direct
   dictionary entry`. A raw bootstrap/private mutation may degrade the mirror
   only to honest absence. No observer or authorizer callback may publish a
   contradictory state, copy inherited methods into direct slots, or create a
   second Rust-side method registry.
6. **Store authority.** Relational reads use the Store's derived facts, but
   those facts never become authoritative over object dispatch. Rust code must
   not introduce a second global environment or bypass the Store callback
   contract.
7. **GC and reentry.** Every payload has an exact layout/trace description;
   raw pointers are rooted for the callback lifetime; no Rust borrow, lock, or
   mutable heap/state borrow spans `send`, a finalizer, TCC, `dlopen`, or a
   foreign callback. A C ABI callback cannot unwind a panic.
8. **Fail closed.** Invalid counts, types, paths, descriptors, wire frames,
   signatures, image records, and stale generations return the established
   nil/Error result or abort only where the existing contract requires an
   invariant abort. Partial activation is rolled back.
9. **Oracle and observation.** The Zig implementation remains loadable and
   testable through a clean-process selection. After focused behavior,
   forced-GC, cold bootstrap, relocation, full verify, and R6 performance pass
   as pre-switch acceptance, one focused commit may make Rust canonical. The
   Zig oracle cannot be retired until the post-switch observation window meets
   Design 62 §5.4.

## 3. Phase R0 — inventory and contract freeze

Do this before adding a collection or service crate.

1. Generate a checked-in inventory for each remaining Zig module: exported
   names, owned vtables, selectors, payload sizes/layouts, root providers,
   finalizers, native symbols, dynamic dependencies, and tests.
2. Record which selectors are self-hosted in `stdlib.birnam`, which are native,
   and which are intentionally absent. Include negative behavior (nil versus
   Error versus abort) and argument-count rules.
3. Add a per-rung identity manifest containing the Rust dylib name, identity
   string, ABI/digest source, dependency identities, and allowed install paths.
   The manifest is diagnostic metadata only; it cannot become a runtime
   fallback or a second authority.
4. Add cross-floor fixtures that run the same corpus with Zig and Rust
   selections in separate processes. Pin canonical output, Error kind/message,
   vtable/export identity, GC live-count bounds, and deterministic ordering.
5. Add a clean-prefix test proving that no remaining-rung test accidentally
   resolves an installed Zig dylib through cwd, source tree, or ambient loader
   search paths.
6. Apply Design 62 §§5.3–5.4 to every checkpoint and observation window. After
   Design 61 R8 retires the live Zig floor, cross-floor tests use only pinned,
   test-only Zig binary/dylib fixtures with fixed identity and digest in
   isolated prefixes. Those fixtures are never installed or distributed and
   are excluded from clean-bootstrap proofs; do not require a live Zig floor
   to remain available once R8 is complete.
7. Run `zig build verify` and `mise run r6-acceptance`; record the current R6
   aggregate and the two explicitly accepted benchmark crossings. Do not
   widen the R6 exception for inventory noise.

**R0 exit gate:** inventory is complete; each selector has a frozen behavior
fixture; Rust and Zig choices are explicit; no default switch has occurred; the
worktree is clean and both gates pass.

## 4. Phase R0.5 — Prelude completion and canonical switch

This phase completed the Prelude boundary before Collection or Error cutover.
The numbered checklist records the acceptance work that led to the canonical
switch; the Zig implementation remains available only through explicit
`-Dprelude-impl=zig` selection.

1. Inventory the entire `src/id/prelude.zig` surface, including source `load:`,
   `loadModule:`, line input, file/environment I/O, output stringification,
   GC controls, and every exported System/Block/Error family selector.
2. Remove Error-family ownership from the Rust Prelude implementation. It must
   import the existing standalone Zig Error family lazily. During this phase,
   Prelude is tested against that Zig Error owner; Rust Error selection begins
   only after R2 creates its standalone replacement.
3. Add source-bootstrap, self-hosted-handler, relocated-install, and subprocess
   stdin/output gates. The valid `readLine` path must be tested outside the
   in-process dynamic runner.
4. Add an explicit `prelude-impl` selection to the native-rung harness. Record
   identity, digest, selector membership, exports, and the Rust/Zig result
   matrix before changing the canonical install mapping.
5. Run focused parity, full verify, and R6 as the pre-switch acceptance gate.
   Make one canonical Rust-Prelude selection commit, then observe project
   builds, image save/load, module loading, process controls, and release
   packaging. Keep the Zig Prelude as the rollback fixture throughout.

**R0.5 exit gate:** complete Prelude surface parity, one Error owner per
process, cold/source bootstrap and subprocess I/O evidence, explicit selector
selection, pre-switch full verify/R6, one canonical switch commit, and a
post-switch observation window.

### R0.5 completion record (2026-07-24)

- `a14732f` added explicit `-Dprelude-impl=rust|zig` selection, clean-prefix
  selector smoke coverage, and side-by-side canonical/fixture installation.
- `48a719a` completed the Rust Prelude Word protocol and inclusive `to:do:`
  loop; `53d53f2` aligned numeric fault kinds and propagation; `06bbf47` and
  `94025f4` aligned non-Boolean block-loop faults and the parity expectation.
- `8ba03b6` made Rust the canonical Prelude. The Zig Prelude remains a
  fail-closed rollback fixture and is never selected implicitly.
- Post-switch `mise run r6-acceptance` passed with the focused set at `43/43`.
  Optimized verify matched the exact recorded `sortedModules` fixture crash
  exception; no additional failure was accepted. The comparator reported only
  the already accepted `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` crossings (`regressions: 2, missing: 0,
  unusable: 0`).
- The R0.5 gate is complete. Continue observing project builds, image
  save/load, module loading, process controls, and release packaging before
  retiring the Zig fixture.

## 5. Phase R1 — Collection (first implementation slice)

Collection is first because Relational and Alien use its Vector/Map protocol,
and because its id-native payloads exercise layouts, persistent sharing, and
GC without changing the floor.

### R1.1 Rust crate and payload model

Implementation checkpoints (2026-07-24):

- `1d0c72e` added the `rust/birnam-collection` crate and characterized its
  first owned family as a pure persistent-storage kernel. `PersistentVector<T>`
  uses the durable 32-way path-copied trie plus a short tail, preserves old
  versions on append, and covers empty/bounds behavior, tail promotion,
  structural sharing, and growth beyond the first root overflow. The crate is
  both `rlib` and `cdylib`, depends only on the frozen `birnam-abi`, and passes
  locked offline tests and clippy.
- `f7c3934` added the live `IdApi` wrapper and `test-rust-collection`. The Rust
  rung allocates the exact `Vector`/`VectorNode` OOP payloads, registers the
  mixed raw/OOP layouts, exports the family, and exercises persistent indexed
  values, the 32-element tail boundary, `do:`, `printString`, and `,` in
  Debug/ReleaseSafe. The explicit `-Dcollection-impl=zig|rust` install choice
  is present, with Zig still canonical for now.
- `75a49af` reconciled the relocated install smoke with the new workspace
  member. The post-change `mise run r6-acceptance` passed at `44/44` focused
  tests; full optimized verify matched only the recorded fixture exception.
- `17d9e1a` completed the first Vector-native protocol slice with
  `inject:into:`, value-aware `indexOf:`, bounded `copyFrom:to:`, and
  `reversed`, with Rust/Zig floor selection and the locked Rust tests still
  green. A fresh R6 run passed at `44/44` after this change.
- `cc6cb7c` added the Rust Range family: the exact two-raw-word payload and
  layout, owned `Range`/`RangeVt` exports, `Integer to:`, and the complete
  native Range protocol (`size`, `includes:`, `do:`, `inject:into:`, `at:`,
  `reversed`, `printString`, and `,`). The cross-language test now covers
  ascending and empty descending ranges, and `mise run r6-acceptance` passed
  at `44/44` with only the two recorded benchmark crossings.
- `781b909` added the persistent Rust HAMT Map family with the exact raw-header
  plus OOP-entry node shape, mixed `Map` layout, value-aware key equality and
  stable hashing, constructors, folds, key/value iteration, absent fallback,
  and rendering. The expanded cross-language target and a fresh R6 run passed
  at `44/44`; optimized verify and comparator results remained unchanged.
- `b5b2448` added Rust StringBuilder with exact all-raw `{ptr, len, cap}` state,
  one-shot native-buffer finalization, fresh `new`/`clone`, append/byte/size,
  and `asString`/`printString`. The cross-language target and full R6 gate
  remained green at `44/44` with the same verify exception and crossings.
- `4ee08d0` added the retained native String protocol to the Rust rung:
  concatenation/equality, indexing and slicing, split/trim/search, bracket
  depth, replacement, symbol conversion, and integer/number parsing. The
  expanded StringBuilder/Collection cross-language target and full R6 gate
  remained green at `44/44`; benchmark crossings are unchanged.
- `eb3f97f` completed the bootstrap Object/nil protocol in the Rust Collection
  rung: Object now answers `isNil`/`notNil`, nil overrides both and renders as
  `"nil"`, and the cross-language corpus asserts all five behaviors. The
  Rust crate tests, clippy, and Debug/ReleaseSafe collection targets passed.
- `f7cf658` hardened `scripts/r6-acceptance.sh` to require the Rust Collection
  target and its single passing integration test in addition to the aggregate
  `44/44` pin. A fresh full R6 run passed with the exact recorded verify
  exception and unchanged `query.aggregate +5.8%` / `compile.stage1-stdlib
  +11.9%` crossings.
- The pre-switch Rust Collection selection smoke also passes with
  `-Dfloor=rust -Dcollection-impl=rust`: `test-rust-collection`,
  `test-collection`, `test-collection-gc`, and `test-gc-layout-mark` are green.
  The repository default remains Zig Collection until the R1 observation and
  canonical-selection commit.

1. Add `rust/birnam-collection` as a `cdylib` over the frozen `birnam-abi`.
   Its production install name is the canonical `libbirnam_id_collection.dylib`;
   use explicit `libbirnam_id_collection_zig.dylib` and
   `libbirnam_id_collection_rust.dylib` names for side-by-side test fixtures.
   Give the Rust library a unique identity and a versioned diagnostic export.
2. Freeze and enumerate the complete owned surface before porting behavior:
   `CollectionVt`, `VectorNode`, `MapNode`, `StringBuilder`, all collection
   constructors and owner/root-provider paths, Object and nil collection
   methods, Integer `to:`, and every native Vector, Map, and String selector.
   Derived stdlib selectors are recorded as derived and remain self-hosted.
3. Port the smallest owned family first: `VectorVt`/`Vector` and its 32-way
   persistent trie. Keep node payloads as exact OOP slots and install the same
   layout descriptors before allocation.
4. Reuse canonical `Integer`, `String`, `Symbol`, `Block`, `Error`, and System
   objects. Never allocate a second Boolean, Error, or Object family.
5. Port `size`, `at:`, `add:`, `do:`, `inject:into:`, `printString`,
   `indexOf:`, `copyFrom:to:`, and `reversed` with exact count/type/error
   behavior. Leave derived higher-order selectors in Birnam unless the Zig
   oracle proves they are native and the benchmark says they must remain so.
6. Add Rust-only unit tests for trie path-copying, tail growth, structural
   sharing, overflow/empty cases, and deterministic rendering. No unit test
   may call a foreign `send` while holding a Rust borrow or lock.

### R1.2 Collection foundation and ownership

1. Port `CollectionVt` as the shared abstract parent and preserve the exact
   parent graph for Vector, Range, Map, and String. Reuse canonical prototypes,
   owners, reload/import behavior, and the no-`[Collection new]` rule.
2. Port the `VectorNode` and `MapNode` families, their constructors, owner
   links, and root-provider registration. Vector nodes are all-OOP payloads;
   HAMT nodes retain the two raw header words followed by OOP entries.
3. Port `StringBuilder` as a raw `{ptr,len,cap}` payload with explicit
   allocation, growth, `new`/`clone`, `add:`, `addByte:`, `size`, `asString`,
   and `printString` behavior. Register its finalizer/ownership rule so a
   builder's native buffer is released exactly once and never traced as OOP.
4. Port Object/nil `isNil`, `notNil`, and `printString` overrides, Integer
   `to:`, every constructor/factory selector, and the exact absent/error/abort
   behavior for invalid receivers and counts.
5. Add export, layout, reload, forced-GC, and constructor-owner fixtures before
   enabling any concrete collection implementation.

### R1.3 Range, Map, and String slices

1. Add `Range` with inclusive bounds, empty/descending behavior, `size`, `at:`,
   `do:`, `inject:into:`, `includes:`, `reversed`, and exact Integer values.
2. Add `Map`/`MapVt` as a persistent HAMT with value equality matching the Zig
   contract: SmallInt by value, String by bytes, Symbols/other objects by
   identity. Preserve the actual node shape: each HAMT node begins with two
   raw header words followed by its OOP entry slots. Layout descriptors must
   encode that mixed raw/OOP shape exactly; never mark either raw header word
   as an OOP slot or trace it as one.
3. Port only the native String methods still owned by the Zig collection rung
   (` ,`, `=`, `size`, `at:`, `byteAt:`, `copyFrom:to:`, case conversion,
   replacement, symbol/number conversion, and printString). Keep the derived
   collection protocol in `stdlib.birnam`.
4. Register root providers for any temporary trie/map node held across a block
   callback. Add forced-GC tests during `do:`/`inject:into:` and after a failed
   callback; prove no shared node is reclaimed early.

### R1.4 Cross-floor and acceptance gates

1. Add one Zig test corpus that loads exactly one selected collection dylib,
   checks exports and selector membership, and runs the behavior matrix.
2. Run the same corpus with the Rust floor and the Zig floor; compare values,
   Error identities, print strings, GC counts/bounds, and ordering.
3. Run the collection GC-layout, Store MOP, Rust Codegen, image heap, and
   installed relocation tests against the selected Rust collection.
4. Run the R6 focused target and full optimized verify. A benchmark change is
   accepted only as a new explicit comparator crossing with a checked-in
   result digest and tradeoff note.
5. Keep Rust opt-in through the pre-switch acceptance gate, then make one
   canonical-selection commit. Observe stdlib bootstrap, relational smoke,
   Alien struct conversion, image save/load, and project builds after that
   commit; the Zig collection remains the rollback fixture.

**R1 exit gate:** `CollectionVt`, `Vector`, `VectorNode`, `Range`, `Map`,
`MapNode`, `StringBuilder`, Object/nil methods, Integer `to:`, every retained
native String/Vector/Map selector, all exports and mixed layouts pass the
cross-floor corpus, forced-GC and relocation checks, pre-switch full verify/R6,
one canonical switch commit, and the post-switch observation window. Zig
remains available for rollback.

## 6. Phase R2 — Standalone Error dylib

Error remains its own dynamic rung. The Rust Prelude imports the family; it
does not subsume the rung or mint a second family.

1. Add `rust/birnam-error` as `libbirnam_id_error.dylib` with the existing
   Error/ErrorVt/ErrorFactoryVt exports, exact two-OOP payload, factory
   selectors, DNU propagation, and Object DNU swap.
2. Remove the Rust Prelude's private Error-family creation path. Its numeric,
   loop, I/O, and DNU helpers must resolve the already-exported Error family
   and fail closed if the required family is unavailable.
3. Prove Zig Error, Rust Error, and both Prelude implementations have identical
   vtable/payload layout, exports, factory selectors, DNU propagation,
   numeric-fault kinds/messages, and reload behavior in isolated processes.
4. Run nested propagation, self-hosted handlers, image export, source
   bootstrap, forced-GC, and both preload-order tests. A preload order that
   would create duplicate families is a failed gate, not a tolerated mode.
5. Run the standalone Error parity, preload-order, GC, image, and source gates
   as the pre-switch acceptance. Make one canonical Rust Error selection
   commit, then observe the complete Prelude, Collection, image, and project
   workflows. Retain the Zig Error dylib as the pinned rollback fixture.

**R2 exit gate:** one Error family per process, one dynamic Error owner, stable
kind/message identity, and no duplicate `ErrorVt`/`Error` export under any
supported load order; pre-switch acceptance, one canonical switch commit, and
post-switch observation are recorded.

### R2.1 implementation checkpoint (2026-07-24)

- `eef950a` added `rust/birnam-error` as a standalone `rlib`/`cdylib` over the
  frozen `birnam-abi`. It owns the exact two-OOP `{kind, message}` payload,
  OOP-default layout, Error/ErrorVt/ErrorFactoryVt exports and reload path,
  factory selectors, propagating Error DNU, Object DNU swap, and the Rust
  identity `rust-error-r2-core`.
- `973990e` added explicit `-Derror-impl=zig|rust` side-by-side installation
  and the `test-rust-error` cross-language target. Debug/ReleaseSafe tests pass
  on both floors, including payload size/layout, factory, DNU propagation, and
  exact `doesNotUnderstand` kind/message behavior. Zig remains canonical.
- `867a68a` reconciled install metadata and required dylib fixtures for the
  new workspace member. `test-install` and `test-native-rung-harness` pass with
  the Rust Error fixtures present.
- `79a84cd` extended the Rust Error corpus to reload Rust twice and then load
  the Zig Error fixture, asserting stable `ErrorVt`, `ErrorFactoryVt`, factory,
  payload size, and export identity across the mixed reload path.
- `bc8c119` runs the same corpus in isolated Rust→Zig and Zig→Rust preload
  orders, rejecting duplicate-family behavior in either supported order.
- `3edc2b6` added both Rust Error preload-order runs to the R6 latest-tests
  pin and hardened the gate to require both root-level integration results.
- `8b363f9` added an explicit Rust Prelude run against
  `libbirnam_id_error_rust.dylib`, asserting `rust-error-r2-core`; the R6 pin
  now includes the Rust Error owner matrix.
- `329efca` extended both Rust Error preload-order runs with five-deep
  propagation and precise-GC churn, retaining the same vtable, kind, and
  message after collection.
- A fresh `mise run r6-acceptance` passed at `48/48`; optimized verify still
  matches only the recorded `sortedModules` fixture exception, and the
  comparator still reports exactly the accepted `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` crossings.

### R2.2 canonical switch checkpoint (2026-07-24)

- `6a19b9b` made Rust Error the canonical `libbirnam_id_error.dylib` owner
  (`-Derror-impl=rust`), while retaining `libbirnam_id_error_zig.dylib` as the
  explicit rollback fixture and preserving `-Derror-impl=zig`.
- The post-switch focused Prelude/Error, selection, and relocated-install
  checks passed. A fresh `mise run r6-acceptance` remained green at `48/48`,
  with the exact existing verify exception and the two accepted benchmark
  crossings unchanged.
- The explicit post-switch `test-cross-floor-errors`,
  `test-cross-floor-images`, and `test-remaining-rung-cross-floor` cells all
  passed, including mapped/rebuilt image boots under both floors.

## 7. Phase R3 — Isolate

1. Add `rust/birnam-isolate` with a pointer-free codec and the exact `BISO`
   wire-v1 frame/schema already implemented by `src/id/isolate.zig`.
2. Freeze and audit the host-facing C ABI before porting worker behavior:
   `birnam_isolate_worker_driver`, `birnam_isolate_worker_boot_error`,
   `birnam_isolate_drain`, and `birnam_isolate_shutdown`. Record exact
   signatures, symbol ownership, buffer/handle ownership, worker and host
   lifetimes, and reentry rules. The existing Zig host must operate against
   the Rust Isolate implementation in an explicit selection before the host
   itself is switched.
3. Port frame validation first (magic/version/flags/request IDs, maximum
   frame/depth/container/node limits, duplicate map keys, UTF-8 and
   transferable-value rules) and compare every error enum to the Zig corpus.
4. Port copied-value encode/decode with bounded allocation and cycle/depth
   guards. Never send a borrowed Rust reference to a worker or callback.
5. Port worker boot, request/reply, timeout/failure, stop/stopped, and owned
   termination. Keep orchestration and policy in `isolate.birnam`/the host.
6. Add child-process tests for malformed frames, hostile children, truncated
   replies, timeout, double-stop, and forced termination. Run debug and
   ReleaseSafe variants.
7. Treat wire, ABI, hostile-child, relocation, full verify, and R6 results as
   the pre-switch acceptance. Make one canonical Rust Isolate selection commit,
   then observe host/project/image workflows and retain the Zig Isolate as the
   rollback fixture.

**R3 exit gate:** wire-v1 corpus parity, no unbounded allocation or panic across
the boundary, deterministic failure/termination, relocated install pass,
pre-switch full verify/R6, one canonical switch commit, and post-switch
observation.

### R3.1 implementation checkpoint (2026-07-24)

- `e9d7f96` added `rust/birnam-isolate` as a pointer-free `rlib` owning the
  exact BISO wire-v1 header and scalar/vector/map value codec. It preserves
  network byte order, request-id rules, float bits, UTF-8/symbol checks,
  map-key restrictions, duplicate detection, depth/node/container limits, and
  fail-closed malformed input errors without depending on `IdApi` or worker
  state.
- The crate's six unit tests, formatting check, locked offline test, and
  clippy `-D warnings` pass. A whole-workspace Cargo test still reaches the
  pre-existing `birnam-codegen` stack-overflow stress test; that failure is
  outside this Isolate slice.

### R3.2 host ABI checkpoint (2026-07-24)

The existing Zig Isolate exports are frozen for the Rust port with the
following C-shaped signatures (`Oop` is the nullable object pointer carried by
the frozen `IdApi` ABI):

```c
int  birnam_isolate_worker_driver(IdApi *api, int fd, Oop receiver, Oop selector);
void birnam_isolate_worker_boot_error(
    IdApi *api, int fd, const char *kind_z, const uint8_t *message, size_t message_len);
void birnam_isolate_drain(void);
void birnam_isolate_shutdown(void);
```

`worker_driver` owns the inherited channel fd and runs until stop, protocol
failure, or transport failure; `receiver` and `selector` are process-local
OOPs borrowed for the call and never cross the channel. `worker_boot_error`
borrows the NUL-terminated kind and byte message only for synchronous record
encoding; it does not retain either pointer or close the fd. `drain` is a
host-thread, idempotent sweep of orphaned/reap-pending registry entries and may
be called before ordinary sends. `shutdown` is host teardown, idempotent, and
owns termination/reaping of every remaining registry process and fd. Rust may
change implementation details, but not these symbols, call conventions,
ownership, lifetime, or reentry rules.

### R3.3 wire probe and R6 gate checkpoint (2026-07-24)

- `831a6f9` added the first installed Rust Isolate `cdylib` probe. The Zig
  integration test loads the Rust fixture through the floor adapter, asserts
  identity `rust-isolate-r3-codec`, and verifies the exported header encoder's
  exact BISO bytes, invalid request-id rejection, and null-output rejection.
- `ff07369` added the Rust Isolate fixture to relocated Cargo metadata and
  install-smoke expectations. `f6cb648` restored the canonical Zig
  `libbirnam_id_isolate.dylib` alongside the explicit Rust fixture after a
  clean-prefix run caught the missing Zig install; both implementations are
  now present in every install/bootstrap surface.
- Clean-prefix `test-install`, `test-lang-selection`, and the optimized
  native-rung harness pass. The revised `mise run r6-acceptance` passes with
  exactly `49/49` focused tests, including the one-test Rust Isolate target;
  optimized verify still matches only the recorded sortedModules fixture
  exception, and the digest-pinned benchmark comparison still reports exactly
  the accepted `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`
  crossings with zero missing or unusable cases.

R3 remains pre-switch: the Rust fixture is probeable and installable, while
worker lifecycle behavior, hostile-child coverage, and canonical selection
remain the next implementation slices.

### R3.4 dynamic frame-validation checkpoint (2026-07-24)

- `333fecd` extends the installed Rust Isolate probe with a pointer-checked
  `birnam_isolate_rust_decode_header` C ABI. It returns decoded kind,
  request-id, and payload length through caller-owned storage and maps the
  existing fail-closed codec errors without borrowing or retaining input.
- The cross-language probe now covers exact round-trip fields, bad magic, and
  null input rejection. Rust unit tests, package clippy, and Debug/ReleaseSafe
  `test-rust-isolate` all pass; the R6 focused boundary remains `49/49`.

### R3.5 owned frame transport checkpoint (2026-07-24)

- `b372c59` adds the owned Rust `Frame` transport wrapper around the frozen
  header codec. It enforces exact payload length, empty-payload rules for
  ready/stop/stopped frames, truncation/trailing rejection, and copies payload
  bytes so no worker can retain a borrowed transport buffer.
- Seven isolated Rust tests and clippy pass. This is the transport seam for
  the upcoming worker loop; no `IdApi` or process state is introduced yet.

### R3.6 control-plane skeleton checkpoint (2026-07-24)

- `bc69774` adds the frozen `birnam-abi` dependency and exports the audited
  worker symbols with the exact C-shaped signatures. The Rust control-plane
  skeleton sends `ready`, accepts one `stop` and replies `stopped`, and emits
  the exact `birnam.boot-error.v1` record without retaining channel or payload
  ownership. `drain` and `shutdown` are explicit no-ops because this fixture
  has no host-side registry yet.
- Nine Rust unit tests and clippy pass, including socketpair handshake,
  stop/reply, and boot-error record tests. Request frames intentionally fail
  closed until the IdApi/OOP value bridge is ported; the crate remains
  unselected and has no canonical-switch claim.

### R3.7 installed host-symbol checkpoint (2026-07-24)

- `b12ef26` extends the dynamic Rust Isolate probe to require all four frozen
  host symbols: worker driver, boot-error writer, drain, and shutdown. The
  probe invokes the lifecycle no-ops only; socketpair behavior remains covered
  by the Rust unit tests.
- Debug and ReleaseSafe dynamic probes pass. This closes symbol-surface drift
  before the next slice ports the IdApi/OOP value bridge; it does not select
  the Rust Isolate or claim worker request parity. A fresh full
  `mise run r6-acceptance` also passes at `49/49`; the verify exception and
  two benchmark crossings remain unchanged.

### R3.8 scalar OOP bridge checkpoint (2026-07-24)

- `b2cac5f` adds a pointer-audited `IdApi` scalar encoder. It maps nil,
  booleans, Integer, Float, String, and Symbol OOPs to the exact BISO scalar
  records, retaining no floor pointers and returning `NonTransferable` for
  unsupported object types. The caller-owned output buffer reports its needed
  size before a short-buffer failure.
- The dynamic corpus covers signed integer bits, negative zero, embedded-NUL
  strings, symbols, unsupported vtables, and short buffers. Debug/ReleaseSafe
  probes plus the nine Rust unit tests and clippy pass; vectors/maps and worker
  request dispatch remain the next seam.

### R3.9 collection OOP bridge checkpoint (2026-07-24)

- `f9b5774` extends the Rust encoder through the frozen Collection protocol:
  `VectorVt`/`MapVt`, `size`, `at:`, `keys`, and `at:put:`. It retains only an
  active-address stack for cycle detection, bounds every container at the
  wire limit, validates map keys, and never introduces a Rust-private
  collection representation.
- The dynamic probe explicitly loads the Rust Collection fixture, then checks
  exact vector and map bytes alongside scalar cases in Debug and ReleaseSafe.
  The Rust nine-test/clippy suite remains green; decode-to-OOP and worker
  request dispatch are still pending.

### R3.10 OOP decode bridge checkpoint (2026-07-24)

- `c2baa52` adds the pointer-audited Rust decoder from BISO values back into
  live floor OOPs through the frozen `IdApi`. Scalars reconstruct with the
  canonical constructors/intern table; vectors and maps are rebuilt only via
  the live Collection selectors, with bounded depth/node/container counts,
  map-key validation, duplicate rejection, and trailing-byte rejection.
- The installed probe covers Integer, Float, embedded-NUL String, Symbol,
  Vector, Map, malformed trailing bytes, and unknown tags. Rust unit tests,
  clippy, and Debug/ReleaseSafe dynamic Isolate probes pass. Worker request
  dispatch, hostile-child lifecycle behavior, and canonical Rust selection
  remain pending.

### R3.11 worker request-dispatch checkpoint (2026-07-24)

- `4f98d8b` replaces the one-frame Rust control skeleton with the frozen worker
  loop: repeated Request frames decode one copied argument, dispatch through
  the borrowed process-local receiver/selector, and emit exact `ReplyOk`,
  `ReplyError`, or `ProtocolError` records before continuing or stopping.
  The channel descriptor remains caller-owned and no OOP or byte buffer is
  retained after the synchronous call.
- The dynamic corpus loads the Rust Prelude and Error owners, then exercises
  two owner-thread requests plus `stop/stopped`; the socket client runs on a
  helper thread because the floor rejects foreign-thread API entry. Rust
  tests/clippy and Debug/ReleaseSafe probes pass. Hostile-child lifecycle,
  timeout/termination parity, and canonical Rust selection remain pending.
- `10d0876` updates the R6 lifecycle pins for the two-test Isolate target and
  the resulting focused aggregate `50/50`. A fresh `mise run r6-acceptance`
  passes; optimized verify retains only the exact recorded fixture exception,
  and the comparator remains at the two accepted crossings
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`.

### R3.12 worker error-corpus checkpoint (2026-07-24)

- `0e3041b` extends the owner-thread worker probe with an unsupported-result
  `ReplyError` record and a malformed request `ProtocolError` record, checking
  the application/protocol schemas, kinds, and codec error names before the
  worker terminates. Debug and ReleaseSafe probes pass.
- `daccb77` advances the R6 pins to the current focused aggregate `51/51` and
  Isolate target `3` tests. A fresh full R6 run passes with the same exact
  verify exception and only `query.aggregate +5.8%` plus
  `compile.stage1-stdlib +11.9%` accepted in the benchmark comparator.

### R3.13 Rust Isolate module-family checkpoint (2026-07-24)

- `afcde0c` adds Rust `module_init` and the exported `IsolateVt`, `Isolate`,
  `IsolateHandleVt`, and `IsolateFailureVt` family shapes with the frozen
  payload/layout and failure selectors. The bridge initially returned a
  structured `rustWorkerHostPending` failure while the host lifecycle was
  still absent; no canonical host switch was claimed.
- The installed probe now verifies module initialization, family exports, and
  the structured pending failure in Debug and ReleaseSafe. The Rust worker
  transport/error corpus remains green; host lifecycle parity and canonical
  selection are still pending.
- A post-bridge `mise run r6-acceptance` rerun passes at `51/51` with the
  Isolate target pinned at `3` tests. The optimized verify boundary and the
  two accepted comparator crossings are unchanged: `query.aggregate +5.8%`
  and `compile.stage1-stdlib +11.9%`.

### R3.14 Rust Isolate host-registry checkpoint (2026-07-24)

- The Rust bridge now owns a process-local registry with opaque handle payloads,
  Unix socketpair launch, inherited worker-fd setup, ready-handshake
  validation, bounded request transport, stop/reap, finalizer orphaning, and
  drain/shutdown cleanup. Requests reconstruct replies through the frozen
  `IdApi`; no receiver, selector, or borrowed OOP is retained in the registry.
- The focused Debug and ReleaseSafe probe now rejects a missing image with the
  concrete `imageUnavailable` boot failure, and builds a fixture image to
  exercise Rust host `start`, ready state, one copied-value request, and
  `stop`/exit against a live worker. The probe still uses the installed Zig
  worker as the child while canonical Rust selection remains a later gate.
- The focused R6 aggregate consequently advances from `51/51` to `52/52`,
  with the Isolate target pinned at `4` tests; the benchmark and optimized
  verify boundaries remain unchanged.
- The subsequent full `mise run r6-acceptance` passes at `52/52` with all
  scoped target pins green and the accepted comparator crossings unchanged:
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`.

### R3.15 Rust Isolate host error-parity checkpoint (2026-07-24)

- Rust host replies now decode the frozen `birnam.application-error.v1`
  record back into the original floor `kind`/`message`, preserve the request
  id, and distinguish deadline, protocol, transport, and child-exit failures.
  Socket timeout handling treats both timed-out and nonblocking retry errors
  as the same bounded deadline condition.
- The lifecycle corpus now starts the installed fixture, round-trips a value,
  stops it, then starts the built-in sleep worker and asserts the exact
  `timeout/deadline` failure and exited state in Debug and ReleaseSafe. An
  abrupt-child exit corpus and canonical Rust selection remain pending.

### R3.16 Rust Isolate abrupt-exit checkpoint (2026-07-24)

- Host transport failures now retain the child exit status and request id in
  the returned Failure object. The lifecycle corpus starts the built-in crash
  worker and asserts `exit/transport`, a non-nil exit status, and clean
  registry reaping in both optimization profiles.
- The Rust host lifecycle/error corpus is now green against the installed Zig
  worker. Canonical Rust worker selection and a Rust-owned child boot corpus
  remain deliberately outside this fixture boundary.

### R3.17 Rust Isolate canonical-selection checkpoint (2026-07-24)

- `build.zig` now exposes explicit `-Disolate-impl=zig|rust` selection,
  installs both `libbirnam_id_isolate_zig.dylib` and
  `libbirnam_id_isolate_rust.dylib`, and maps only the selected implementation
  to canonical `libbirnam_id_isolate.dylib`. The default remains Zig until the
  full selection matrix is accepted.
- Rust’s built-in `_isolateSleep:` fixture now matches the Zig timing hook;
  canonical Rust install smoke and the Rust Isolate corpus pass in Debug and
  ReleaseSafe, including Rust host plus Rust worker, timeout, and crash paths.
- This is a selection/rollback checkpoint, not yet the canonical switch:
   full `-Disolate-impl=rust` install relocation and R6 matrix acceptance were
   the remaining pre-switch evidence and are recorded in R3.18 below.

### R3.18 Rust Isolate canonical-R6 acceptance checkpoint (2026-07-24)

- The explicit Rust selection path now passes the relocated install smoke and
  the full `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` gate. The run
  completed `93/93` build steps and `52/52` focused tests, including all `4`
  Rust Isolate lifecycle tests, the optimized verify boundary, and the
  accepted benchmark comparator crossings (`query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%`).
- The gate accepts the latest lifecycle corpus through the same explicit
  implementation selector used by the install matrix. This closes the
  pre-switch Rust Isolate acceptance evidence.

### R3.19 Rust Isolate canonical switch and observation checkpoint (2026-07-24)

- `ec9d58b` makes Rust Isolate the default canonical implementation for
  `libbirnam_id_isolate.dylib`; the Zig implementation remains installed as
  `libbirnam_id_isolate_zig.dylib` and selectable with
  `-Disolate-impl=zig` for rollback.
- The switched default passes relocated `test-install` (`113/113` steps) and
  the full default `mise run r6-acceptance`: `93/93` build steps, `52/52`
  focused tests, Rust Isolate `4/4`, optimized verify, and the exact accepted
  benchmark crossings (`query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%`) with no additional regressions.
- The host/project/image observation window is therefore closed for the
  Isolate switch; future R3 work retains the explicit Zig artifact and the
  implementation selector as the rollback seam.

### R4.1 authority-free relational kernel checkpoint (2026-07-25)

- `4019091` adds the new `birnam-relational` Rust crate, which evaluates a borrowed Store triple
  snapshot with a deterministic left-to-right conjunction join and distinct
  projection. Its input facts remain owned by the Store; it owns only copied
  query bindings and never crosses an IdApi callback or stores a second fact
  authority.
- The first corpus covers constant matching, shared-variable joins, repeated
  variables, empty results, result-order preservation, no invented bindings,
  and scalar integer equality/deduplication. Classification of live Oops and
  the dynamic `module_init` wrapper remain subsequent slices.
- `3d3739a` updates the relocated Cargo-manifest acceptance for the new member;
  the subsequent Rust-default R6 rerun passes `93/93` build steps and `52/52`
  focused tests with the existing benchmark allowances unchanged. The full
  workspace Cargo test still has the pre-existing deep-codegen stack-overflow
  abort; the new crate's locked offline test and clippy gates are green.

### R4.2 relational value-law checkpoint (2026-07-25)

- `f1fa537` upgrades kernel terms and triples to classified identity, integer,
  float, and byte-string values. Matching is identity-only for objects and
  value-based for scalar classes; projected results retain Store order while
  deduplicating by that same law. The corpus is now `6/6` tests green under
  locked offline Cargo test and clippy.
- The switched-default R6 rerun remains green at `93/93` and `52/52`; no
  benchmark allowance changed. The pending adapter must classify live Oops
  without retaining borrowed pointers and must continue to source all facts
  from Store's `liveTriples*` protocol.

### R4.3 IdApi classification checkpoint (2026-07-25)

- `e873dc9` adds the one-way classifier used by the future adapter. It copies
  SmallInt, Float, and String payloads into `Value` classes, maps all other
  non-null Oops to identity addresses, treats null as identity zero, and
  rejects malformed string buffers before copying. The IdApi table is neither
  retained nor crossed by a returned Rust reference.
- The classifier compiles through the workspace and the Rust-default R6 gate
  remains green at `93/93` and `52/52`, with relocated install and native-rung
  harness passing. A dynamic Rust Relational module and Store callback bridge
  are still intentionally pending.

### R4.4 Rust Relational probe and current R6 pin (2026-07-25)

- `a508683` adds the explicit Rust Relational `cdylib` probe. The installed
  boundary calls `module_init` twice, checks identity
  `rust-relational-r4.3-kernel`, and verifies live `SmallInt`, `Float`, and
  byte-string classification, short-output rejection, symbol identity, and
  null-output rejection. The Zig Relational dylib remains canonical; the Rust
  dylib is an explicit rollback/probe artifact until the Store callback bridge
  is complete.
- The R6 gate revision `2a61a8b` advances the focused aggregate to `53/53`
  and pins the current optimized closure at `259/262` steps, `459/465` tests,
  and 6 skipped. Its only accepted failure remains the existing
  `modules/run-tests.birnam` `collection.zig:336:29` `vSortBy` segmentation
  fault; the current output is pinned to the preceding native-Rust assertion,
  so a changed command, stack, assertion, summary, or additional failure
  fails closed.
- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passes from this
  checkpoint. The digest-pinned comparator still reports zero missing or
  unusable cases and only the accepted `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` crossings. The Store callback bridge and
  canonical Rust Relational selection remain pending.

### R4.5 Rust Relational Store callback checkpoint (2026-07-25)

- `d64d9f9` adds the Rust-owned Store callback adapter. It invokes the frozen
  `liveTriplesDo:` protocol through IdApi, classifies each callback triple into
  caller-owned pointer-free records, bounds output capacity, and rejects
  malformed callback arguments. A temporary callback block is rooted through
  the floor root-provider seam for the synchronous send; no Store fact, IdApi
  pointer, or borrowed Oop is retained after the call. The explicit module
  identity is now `rust-relational-r4.4-bridge`.
- The dynamic Rust Relational target loads the Rust Store provider, asserts a
  fresh fact, verifies identity and integer classification in the returned
  live-triple stream, and checks short-buffer rejection. The focused target is
  `2/2`; the Zig Store rollback cross-product remains a subsequent fixture.
- `bae02de` advances the R6 focused pin to `54/54` and requires the two-test
  callback target. `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passes:
  optimized verify remains the exact `modules/run-tests.birnam`
  `collection.zig:336:29` fixture exception at `259/262` steps,
  `460/466` tests, and 6 skipped; the digest-pinned comparator retains zero
  missing or unusable cases and only `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%`. Canonical Rust Relational selection and
  the Zig Store rollback cross-floor fixture remain pending.

### R4.6 Rust Relational × Zig Store rollback checkpoint (2026-07-25)

- `53c7810` adds a separate `test-rust-relational-zig-store` process using
  the same callback corpus against the installed `libbirnam_store_zig.dylib`.
  The Rust Store and Zig Store providers are never loaded into one process;
  each retains sole ownership of its Store object and callback surface.
- Both Rust Relational processes pass `2/2`: Rust Store current-live
  enumeration and the Zig Store rollback oracle produce the same classified
  asserted triple, and both reject a zero-capacity output buffer. The focused
  R6 boundary is now `100/100` build steps and `56/56` tests, including both
  cross-floor targets.
- The follow-up full R6 run passes with optimized verify pinned at exactly
  `260/263` steps, `462/468` tests, and 6 skipped, still accepting only the
  existing `run-tests.birnam`/`vSortBy` fixture. The comparator remains zero
  missing/unusable with only the two recorded crossings. Full query parity,
  historical windows, and canonical Rust Relational selection remain open.

### R4.7 Rust Relational historical-window checkpoint (2026-07-25)

- `720c31b` adds the Rust `liveTriplesAsOf:do:`, `liveTriplesSince:do:`, and
  `liveTriplesBetween:and:do:` bridge. The adapter validates modes and
  SmallInt transaction bounds, preserves the synchronous rooted callback
  lifetime, and returns pointer-free classified records without taking Store
  ownership. The corpus now checks snapshot, lower-bound, half-open interval,
  and invalid-mode behavior.
- The Zig Store bridge fixture exposed a boolean ABI mismatch: its history
  policy compared against interned `"true"`/`"false"` symbols even though the
  frozen IdApi supplies canonical `trueObj`/`falseObj`. `720c31b` aligns the
  Store markers with those singletons, and `f35c0d9` updates the Zig relational
  history fixture to assert `trueObj`. This preserves one boolean family across
  Rust and Zig providers and makes both Store implementations obey the same
  history contract.
- Both callback processes pass `3/3` in Debug/ReleaseSafe, including the
  historical corpus; Zig Store remains an explicit rollback provider and is
  never loaded with Rust Store in one process. The full Rust crate checks,
  tests, and clippy gates are green.
- `f35c0d9` advances the focused R6 boundary to `58/58`; `3c665a9` pins both
  focused and optimized verify invocations to `-Dfloor=rust` so prior explicit
  Zig-floor cache state cannot alter the selected floor. The accepted optimized
  closure is exactly `260/263` steps, `464/470` tests, and 6 skipped, with only
  the recorded `run-tests.birnam`/`vSortBy` fixture exception. The digest-pinned
  comparator remains at zero missing/unusable cases and exactly the accepted
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%` crossings.
- Full query parity, recursive/pull/aggregate cross-floor coverage beyond the
  Store-window bridge, and canonical Rust Relational selection remain open.

### R4.8 Rust Relational one-clause projection checkpoint (2026-07-25)

- `94272c9` adds an explicit, non-canonical Rust Relational probe that gathers
  the current `liveTriplesDo:` stream from Store, classifies it into owned Rust
  values, evaluates one triple clause through the authority-free kernel, and
  returns a caller-owned classified projection. Null term positions represent
  distinct logic variables; non-null terms use the frozen identity/scalar value
  law. The callback snapshot is bounded, synchronous, and never retained as a
  second Store authority.
- The cross-floor corpus now covers a matching identity projection, scalar
  constant matching, distinct empty results, output-capacity rejection, and an
  invalid projection index against both Rust Store and the Zig Store rollback
  provider. Both targets pass `4/4`; the probe identity is
  `rust-relational-r4.5-project`, while canonical `findAll:where:` installation
  remains deliberately pending.
- `a0d4dea` advances the R6 focused boundary to `60/60` and requires both
  relational targets to report exactly `4` passing tests. The full gate passes
  with optimized verify pinned at `260/263` steps, `466/472` tests, and 6
  skipped, accepting only the existing `run-tests.birnam`/`vSortBy` fixture;
  the digest-pinned comparator still reports zero missing/unusable cases and
  exactly `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%`.
- Multi-clause joins, predicates/negation/disjunction, rule fixpoints,
  aggregation, pull, historical query windows, and canonical Rust Relational
  selection remain open.

### R4.9 Rust Relational two-clause join checkpoint (2026-07-24)

- `daa22d5` extends the explicit, non-canonical projection probe to a bounded
  two-clause conjunction. Both clauses are evaluated left-to-right against the
  synchronous, owned Store snapshot; null positions use the same position-based
  logic variables across clauses, so a shared entity joins the two patterns.
  The Rust kernel remains the only evaluator and the Store remains the sole
  authority; the callback snapshot is not retained or exposed as a second
  Store.
- The cross-floor corpus now proves one matching two-clause join against both
  the Rust Store and the Zig Store rollback provider. Each relational target
  reports `5/5`, including the prior single-clause projection, scalar,
  empty-result, capacity, and invalid-index cases. The probe is intentionally
  bounded and does not claim arbitrary query-variable encoding or canonical
  `findAll:where:` selection.
- `f3b68d9` advances the R6 focused boundary to `62/62` and requires both
  relational targets to report exactly five passing tests. The full gate passes
  with optimized verify pinned at `260/263` steps, `468/474` tests, and 6
  skipped, accepting only the existing `run-tests.birnam`/`vSortBy` fixture;
  the digest-pinned comparator still reports zero missing/unusable cases and
  exactly `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%`.
- Predicates/negation/disjunction, rule fixpoints, aggregation, pull,
  arbitrary query-variable encoding, and canonical Rust Relational selection
  remain open.

### R4.10 Rust Relational numeric-predicate checkpoint (2026-07-24)

- `94f976f` adds a bounded predicate probe after one triple binder. It filters
  a position-based bound variable with a constant right operand, supports the
  six frozen comparison operators (`<`, `<=`, `>`, `>=`, `=`, `~=`), promotes
  Integer/Float pairs for ordering, and keeps equality type-strict. The Rust
  kernel evaluates copied values only; it does not send borrowed Oops or retain
  a floor reference. Unsupported ordering operands simply drop the row, as the
  existing relational predicate path does for comparison faults.
- The cross-floor corpus covers integer ordering, mixed numeric promotion,
  strict Integer-versus-Float equality, invalid operators, and rejection of a
  constant predicate position against both Rust Store and the Zig Store
  rollback provider. Both relational targets report `6/6`. This remains an
  explicit probe: the right operand is constant, the binder precedes the
  predicate, and no planner reordering, arbitrary operand shapes, or canonical
  `findAll:where:` installation is claimed.
- `eacd5e1` advances the R6 focused boundary to `64/64` and requires both
  relational targets to report exactly six passing tests. The full gate passes
  with optimized verify pinned at `260/263` steps, `470/476` tests, and 6
  skipped, accepting only the existing `run-tests.birnam`/`vSortBy` fixture;
  the digest-pinned comparator still reports zero missing/unusable cases and
  exactly `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%`.
- Negation, disjunction, planner reordering, arbitrary predicate operands,
  rule fixpoints, aggregation, pull, and canonical Rust Relational selection
  remain open.

### R4.11 Rust Relational negation checkpoint (2026-07-24)

- `ea3c512` adds a bounded negation-as-failure probe after one positive
  binder. The negative clause shares already-bound variables by position;
  negative-only variables are scoped to the existence check and never escape
  the projection. The Rust kernel still evaluates copied values only, keeps
  left-to-right clause order, and does not claim arbitrary negated-variable
  encoding or canonical `findAll:where:` installation.
- The cross-floor corpus now covers a matching negated fact, an absent
  negated fact, a shared-variable mismatch, and negative-only-variable
  scoping against both Rust Store and the Zig Store rollback provider. Both
  relational targets report `7/7`.
- `de89c3f` advances the R6 focused boundary to `66/66` and requires both
  relational targets to report exactly seven passing tests. The full gate now
  accepts the current native-rung/install-smoke fixture signature in both the
  focused harness and optimized verify: two `collection.zig:336:29`
  `vSortBy` crashes from `scripts/install-smoke.sh: line 92`, with the exact
  nested `111/113` and final `260/264` summaries. The optimized closure is
  `260/264` steps, `472/478` tests, and 6 skipped; no other failure is
  accepted. The digest-pinned comparator remains at zero missing/unusable
  cases and exactly the accepted `query.aggregate +5.8%` plus
  `compile.stage1-stdlib +11.9%` crossings.
- Disjunction, planner reordering, arbitrary predicate operands, rule
  fixpoints, aggregation, pull, and canonical Rust Relational selection
  remain open.

### R4.12 Rust Relational disjunction checkpoint (2026-07-24)

- `9beb696` adds a bounded two-branch disjunction probe. Each positive branch
  evaluates from the same empty solution set over one synchronous Store
  snapshot; branch order is preserved and projected values are deduplicated by
  the existing value law. The kernel remains authority-free and does not
  reorder or compose arbitrary nested branches.
- The cross-floor corpus covers a city union, a union across different
  attributes, branch-order preservation, projection deduplication, output
  capacity rejection, and invalid projection rejection against both Rust
  Store and the Zig Store rollback provider. Both relational targets report
  `8/8`.
- `cf7280d` advances the R6 focused boundary to `68/68` and requires both
  relational targets to report exactly eight passing tests. The full gate
  passes with optimized verify pinned at `260/264` steps, `474/480` tests,
  and 6 skipped, accepting only the two exact `install-smoke.sh: line 92`
  `collection.zig:336:29` fixture crashes recorded in R4.11. The
  digest-pinned comparator remains at zero missing/unusable cases and exactly
  the accepted `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%`
  crossings.
- Planner reordering, arbitrary predicate operands, nested/compound
  disjunctions, rule fixpoints, aggregation, pull, and canonical Rust
  Relational selection remain open.

### R4.13 Rust Relational recursive-edge checkpoint (2026-07-24)

- `14b2e7c` adds a bounded recursive-edge probe. It copies one binary edge
  relation from Store, computes a deterministic finite transitive closure with
  value-law set membership, and projects either the root or its reachable
  targets. The probe has no rule-definition registration, rule-cache
  persistence/rebuild, mutual-rule network, or second Store authority; it is
  an explicit seam for the later full rule engine.
- The cross-floor corpus covers a three-hop parent chain, root projection,
  target ordering, finite-fixpoint termination, output capacity rejection, and
  rejection of unsupported endpoint/projection shapes against both Rust Store
  and the Zig Store rollback provider. Both relational targets report `9/9`.
- `5e0e495` advances the R6 focused boundary to `70/70` and requires both
  relational targets to report exactly nine passing tests. The full gate passes
  with optimized verify pinned at `260/264` steps, `476/482` tests, and 6
  skipped, accepting only the two exact `install-smoke.sh: line 92`
  `collection.zig:336:29` fixture crashes. The digest-pinned comparator remains
  at zero missing/unusable cases and exactly the accepted
  `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%` crossings.
- Full rule-definition semantics, mutual recursion, rule-cache rebuild,
  arbitrary recursive bodies, aggregation, pull, and canonical Rust
  Relational selection remain open.

### R4.14 Rust Relational aggregate checkpoint (2026-07-24)

- `8961d37` adds a bounded aggregate probe over one triple-pattern projection.
  It folds the already-distinct value stream with `count`, `sum`, `min`,
  `max`, and `avg`, preserves Integer/Float promotion for numeric sums and
  averages, keeps strict value typing, and rejects unsupported/non-numeric
  folds. Empty count/sum return integer zero; empty min/max/avg return an
  explicit nil status rather than an ambiguous identity record.
- The cross-floor corpus covers count over entities and distinct scalar
  values, distinct sum, min/max/average, empty count/sum, empty nil behavior,
  invalid operation, and nonnumeric rejection against both Rust Store and the
  Zig Store rollback provider. Both relational targets report `10/10`.
- `c5ca8be` advances the R6 focused boundary to `72/72` and requires both
  relational targets to report exactly ten passing tests. The full gate passes
  with optimized verify pinned at `260/264` steps, `478/484` tests, and 6
  skipped, accepting only the two exact `install-smoke.sh: line 92`
  `collection.zig:336:29` fixture crashes. The digest-pinned comparator remains
  at zero missing/unusable cases and exactly the accepted
  `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%` crossings.
- Grouped aggregation, query time-window composition, pull, full rule
  semantics, and canonical Rust Relational selection remain open.

### R4.15 Rust Relational query-window checkpoint (2026-07-24)

- `0615ac7` adds a bounded projection probe over the frozen Store transaction
  selectors: current, `asOf`, `since`, and half-open `between`. It reuses the
  synchronous rooted callback bridge and feeds each copied snapshot through
  the same authority-free one-clause kernel; history remains Store policy, so
  the fixture explicitly opts the overwritten attribute into `history=true`.
- The cross-floor corpus covers an earlier value at `asOf`, the changed value
  at `since`, the same half-open interval at `between`, invalid modes, and
  SmallInt-bound rejection against both Rust Store and the Zig Store rollback
  provider. Both relational targets report `11/11`.
- `8629a91` advances the R6 focused boundary to `74/74` and requires both
  relational targets to report exactly eleven passing tests. The full gate
  passes with optimized verify pinned at `260/264` steps, `480/486` tests,
  and 6 skipped, accepting only the two exact `install-smoke.sh: line 92`
  `collection.zig:336:29` fixture crashes. The digest-pinned comparator remains
  at zero missing/unusable cases and exactly the accepted
  `query.aggregate +5.8%` plus `compile.stage1-stdlib +11.9%` crossings.
- Pull composition, grouped aggregation, full rule-definition semantics, and
  canonical Rust Relational selection remain open.

### R4.16 Rust Relational pull probe checkpoint (2026-07-25)

- `eccd004` adds an authority-free Rust pull kernel and dynamic probe. The
  adapter snapshots Store through the existing rooted callback, reads each
  attribute's `cardinality:` schema through the frozen Store protocol, and
  copies a bounded flat pull specification into Rust-owned values. Nested
  references and `:many` leaves are returned as caller-owned flattened
  attribute paths, not Rust Map/Vector objects, so the probe does not create a
  second collection or fact authority. Malformed node kinds, child ranges,
  nulls, excessive depth, and insufficient output capacity fail closed.
- The pure kernel corpus covers deterministic nested `:many` reference order,
  path preservation, and scalar value copying. The cross-floor dynamic corpus
  covers the same nested pull against Rust Store and the Zig Store rollback
  provider, capacity rejection, and malformed-spec rejection. Both relational
  targets report `12/12`.
- `aa14450` advances the R6 focused boundary to `76/76` and requires both
  relational targets to report exactly twelve passing tests. The full
  `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` gate passes with the
  focused aggregate at `100/100` steps and `76/76` tests; optimized verify is
  green, and the benchmark comparator remains at zero missing/unusable cases
  with exactly `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`
  accepted for this snapshot.
- Canonical `pull:spec:` installation, grouped aggregation, full
  rule-definition semantics, and canonical Rust Relational selection remain
  open. This probe deliberately preserves the Zig Relational implementation as
  the behavioral authority and rollback path.

### R4.17 Rust Relational grouped-aggregate checkpoint (2026-07-25)

- `6341cbc` adds the authority-free grouped aggregate kernel and dynamic probe.
  Grouped probe patterns carry explicit variable identities so independent
  group/value positions cannot be conflated by the earlier position-based
  projection ABI. The adapter copies a synchronous Store snapshot, evaluates
  two positive clauses left-to-right, preserves first group appearance, dedups
  values under the frozen value law, and returns a flat caller-owned result
  array rather than a Rust Map.
- The pure kernel and cross-floor corpus cover distinct `sum` and `count`
  results for two groups, duplicate values, deterministic group order,
  capacity rejection, and invalid aggregate operation. Both Rust Store and Zig
  Store rollback targets report `13/13`.
- `560af6f` advances the R6 focused boundary to `78/78` and requires both
  relational targets to report exactly thirteen passing tests. The full R6 gate
  passes with optimized verify green, and the comparator remains at zero
  missing/unusable cases with exactly `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` accepted.
- Canonical grouped `Map` materialization, full rule-definition semantics, and
  canonical Rust Relational selection remain open; no Store or collection
  authority has moved in this probe.

### R4.18 Rust Relational rule-engine checkpoint (2026-07-25)

- `1aee57d` adds an authority-free bounded rule probe with pointer-free rule
  descriptors. Triple clauses and named rule calls are copied into Rust-owned
  values, evaluated left-to-right to a finite least fixpoint, and projected
  into caller-owned classified values. Recursive and mutually recursive rules
  converge without host-language recursion; duplicate tuples preserve
  definition/order semantics. Malformed descriptors, null/count mismatches,
  and insufficient output capacity fail closed.
- The pure Rust corpus covers recursive ancestor reachability and a mutually
  recursive even/odd relation. The cross-floor corpus runs the same rules
  against Rust Store and the Zig Store rollback provider, including capacity
  rejection. Both relational targets report `14/14`.
- `4624e76` advances the R6 focused boundary to `80/80` and requires both
  relational targets to report exactly fourteen passing tests. The full R6
  gate passes with optimized verify green, and the comparator remains at zero
  missing/unusable cases with exactly `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` accepted.
- Canonical `defineRule:` Store reification, rule-cache rebuild/invalidation,
  arbitrary rule-body and parameter semantics, canonical grouped/pull result
  materialization, and canonical Rust Relational selection remain open. This
  probe deliberately preserves the Zig Relational implementation as the
  behavioral authority and rollback path.

### R4.19 Rust Relational rule-cache lifecycle checkpoint (2026-07-25)

- `1f5fce0` factors the Rust rule kernel through an explicit query-scoped
  `RuleCache`. `rebuild` clears all derived tuples before reseeding definitions
  and recomputing the finite least fixpoint; `query` reads only that rebuilt
  cache. This makes stale snapshot rows impossible at the Rust seam while
  retaining Store as the sole fact authority.
- The Rust unit corpus now has `16/16` passing tests, including a snapshot
  replacement test that rebuilds `ancestor` from `1 → 2` to `1 → 3` and proves
  the old tuple is absent. The cross-floor dynamic corpus remains `14/14` for
  both Rust Store and Zig Store because this is an internal kernel lifecycle
  refactor; the accepted R6 gate counts therefore remain `80/80`, `14/14`, and
  optimized `486/492`.
- Canonical Store rule-fact reification, image-triggered rebuild wiring,
  arbitrary rule-body/parameter semantics, canonical grouped/pull result
  materialization, and canonical Rust Relational selection remain open. The
  cache is intentionally rebuilt per supplied snapshot and is not persisted
  as a second authority.

### R4.20 Rust Relational rule-body semantics checkpoint (2026-07-25)

- `7032f71` extends the pure rule kernel with ordered predicate filters,
  negation-as-failure, and disjunction branches. Nested rule bodies reuse the
  same left-to-right binding discipline and current partial relation cache;
  predicates never invent bindings, negated variables never escape, and branch
  order remains deterministic.
- The Rust unit corpus now reports `17/17`, including a mixed predicate,
  negation, and disjunction rule that returns only the expected entities in
  source order. The dynamic FFI remains intentionally bounded to the existing
  triple/call descriptor kinds, so the cross-floor targets stay `14/14` and R6
  remains pinned at `80/80`, `14/14`, and optimized `486/492`.
- Canonical Store rule-fact reification, descriptor support for these richer
  body forms, image-triggered cache rebuild wiring, arbitrary parameter
  semantics, canonical grouped/pull result materialization, and canonical
  Rust Relational selection remain open.

### R4.21 Rust Relational predicate-rule descriptor checkpoint (2026-07-25)

- `d3938a8` adds bounded `RuleClauseNode.kind == 2` predicate decoding. The
  descriptor maps the six frozen comparison symbols by interned identity and
  evaluates the predicate only after earlier body clauses have bound its
  operands; malformed operators and arities fail closed.
- The existing recursive/mutual cross-floor fixture now also defines a numeric
  threshold rule against both Rust Store and Zig Store. Both dynamic targets
  remain `14/14`; the Rust pure corpus remains `17/17`, and the accepted R6
  boundary stays `80/80`, `14/14`, optimized `486/492`.
- Descriptor forms for nested negation/disjunction, canonical Store rule-fact
  reification, image-triggered cache rebuild wiring, arbitrary parameter
  semantics, canonical grouped/pull materialization, and canonical Rust
  Relational selection remain open.

### R4.22 Rust Relational nested-rule descriptor checkpoint (2026-07-25)

- `462b610` adds a separate tree-shaped rule ABI for nested bodies, preserving
  the flat descriptor layout already in use. Bounded nodes represent
  negation, disjunction, and explicit branch containers; depth, child ranges,
  term arity, and malformed branch shapes fail closed. The adapter reuses the
  same left-to-right pure kernel and caller-owned output records.
- The existing cross-floor rule fixture now covers a mixed numeric predicate,
  negated blocker, and two disjunction branches through the tree ABI. Both
  dynamic targets remain `14/14`; the Rust pure corpus remains `17/17`, and
  R6 stays pinned at `80/80`, `14/14`, optimized `486/492`.
- Canonical Store rule-fact reification, image-triggered cache rebuild wiring,
  arbitrary parameter semantics, canonical grouped/pull materialization, and
  canonical Rust Relational selection remain open.

### R4.23 Rust Relational Store-rule reification checkpoint (2026-07-25)

- `570f74a` adds an explicit Store-facing rule probe. It snapshots
  `__rule-name__` facts alongside triples, walks each canonical pair's
  parameter/body Vectors through the frozen `size`/`at:` protocol, maps live
  LogicVars by their `name`, and rebuilds Rust-owned definitions before
  querying. The probe supports stored triple, recursive call, and predicate
  clauses; malformed vectors, missing LogicVar exports, unsupported body forms,
  and invalid operators fail closed. No Store facts or Rust cache survive the
  synchronous call.
- The cross-floor corpus now creates two `defineRule:` definitions and proves
  the reified recursive result against both Rust Store and Zig Store. Both
  dynamic targets remain `14/14`; the Rust pure corpus remains `17/17`, and R6
  remains pinned at `80/80`, `14/14`, optimized `486/492`.
- Nested tree descriptors cover richer explicit bodies separately. Canonical
  Rust Store method installation, image-triggered reification/rebuild wiring,
  arbitrary stored negation/disjunction parameter semantics, canonical
  grouped/pull materialization, and canonical Rust Relational selection remain
  open.

### R4.24 Rust Relational nested Store-rule checkpoint (2026-07-25)

- `a9cbf27` extends canonical Store reification to recursively parse stored
  `#not` and `#or` clauses in addition to triples, predicates, and rule calls.
  Special-form dispatch precedes three-term predicate/triple decoding so
  `[#or ...]` cannot be misclassified as a triple; nested branches reuse the
  same variable map and fail closed on malformed shapes.
- The cross-floor corpus defines and queries a stored rule combining a numeric
  predicate, negation, and disjunction. Both dynamic targets remain `14/14`;
  the Rust pure corpus remains `17/17`, and the current R6 acceptance remains
  `80/80`, `14/14`, optimized `486/492`.
- Image-triggered cache rebuild wiring, canonical Rust Store method
  installation, arbitrary parameter/variable-scope semantics, canonical
  grouped/pull materialization, and canonical Rust Relational selection remain
  open.

### R4.25 Store rule-cache rebuild regression checkpoint (2026-07-25)

- `3a0202f` extends the Store-facing rule corpus to invoke the canonical Zig
  `rebuildRules` selector before both Rust reification queries. Recursive and
  nested stored definitions still produce the same results, proving the Rust
  probe reconstructs from durable rule facts rather than depending on Zig's
  in-memory cache.
- Both dynamic targets remain `14/14`; no R6 count change is required. The
  latest accepted boundary remains `80/80`, `14/14`, optimized `486/492`, with
  only the existing two benchmark allowances.
- Image-load-triggered rebuild, canonical Rust Store method installation,
  arbitrary parameter/variable-scope semantics, canonical grouped/pull
  materialization, and canonical Rust Relational selection remain open.

### R4.26 Rust Relational entity-method bridge checkpoint (2026-07-25)

- `e11cb9d` installs the Rust `entity` callback on the live Store vtable when
  Rust Relational is loaded after Store. It reuses the exported `EntityVt`
  when present, publishes an equivalent empty-payload vtable only for a
  Store-first load without one, and allocates each result through the frozen
  IdApi. A classifier-only load before Store remains a deliberate no-op; it
  does not invent a private Store authority or require load-order changes.
- The cross-floor rule corpus now asserts that two Rust-installed `entity`
  sends produce distinct live objects. Both dynamic targets remain `14/14`;
  the Rust pure corpus remains `17/17`, and the accepted R6 boundary remains
  `80/80`, `14/14`, optimized `486/492` with only the two existing benchmark
  allowances.
- Canonical Rust `findAll:`/full Store method installation, image-triggered
  rebuild, arbitrary parameter/variable-scope semantics, canonical
  grouped/pull materialization, and canonical Rust Relational selection remain
  open.

### R4.27 Rust Relational query-body projection checkpoint (2026-07-25)

- `5a56ade` exposes a pure `project_rule_body` seam that rebuilds a
  query-scoped rule cache from the supplied Store snapshot, evaluates a mixed
  body (triple, rule call, predicate, negation, and disjunction), and projects
  distinct results in body order. The cache is discarded after the call, so
  Rust still owns no durable facts or independent rule authority.
- The Rust unit corpus remains `17/17`; this slice changes no dynamic target
  count and does not claim canonical `findAll:where:` ownership. The next
  adapter work must parse the live LogicVar/Vector query shape, preserve the
  Zig planner's binder-before-predicate behavior, and materialize the result
  through the shared Vector protocol before any selector override.
- Canonical `findAll:`/full Store method installation, image-triggered rebuild,
  arbitrary parameter/variable-scope semantics, canonical grouped/pull
  materialization, and canonical Rust Relational selection remain open.

### R4.28 Rust Relational `findAll:where:` bridge checkpoint (2026-07-25)

- `7f7676f` installs a bounded Rust `findAll:where:` callback alongside the
  Rust `entity` bridge once Store, Collection, and LogicVar contracts are
  present. The callback parses the live LogicVar/Vector query shape, copies
  current Store triples and durable `__rule-name__` facts, rebuilds its
  query-scoped rule cache, and materializes results through the shared Vector
  `new`/`add:` protocol. The growing result Vector is rooted across floor sends;
  no Rust fact or rule cache survives the call.
- The cross-floor canonical query probe covers a three-edge parent query and
  preserves Store order. Both dynamic targets remain `14/14`; the Rust pure
  corpus remains `17/17`, and the accepted R6 boundary remains `80/80`,
  `14/14`, optimized `486/492` with only the two existing benchmark
  allowances.
- This is not full Relational selection: `asOf`/`since`/`between`, aggregate,
  grouped, pull, image-triggered rebuild, arbitrary parameter/variable-scope
  semantics, and the canonical Rust Relational switch remain open.

### R4.29 R6 acceptance after the `findAll:where:` bridge (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after
  `7f7676f`: `100/100` build steps and `80/80` focused tests, with both Rust
  Relational dynamic targets at `14/14`.
- The optimized comparator reported exactly the two existing allowances:
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`; all other
  benchmark cases were accepted. No gate threshold was broadened.
- This acceptance closes the current bridge observation seam, but not the R4
  exit gate. The remaining method family and canonical-selection work must
  retain this same R6 boundary and rollback evidence.

### R4.30 Rust Relational binder-planner parity checkpoint (2026-07-25)

- `6231e6a` adds the binding-aware clause planner to the Rust rule/query body
  evaluator. Predicates and negations are deferred until all mentioned
  variables are bound; binders retain source order among ready clauses, and a
  no-ready fallback matches the Zig behavior. The cross-floor canonical probe
  deliberately writes `gt` before its score triple and still returns only the
  high-scoring entity.
- Rust unit tests remain `17/17`; both dynamic targets remain `14/14`. This
  preserves the R6 boundary recorded in R4.29; no benchmark allowance or gate
  threshold changed.
- Window selectors, aggregate/group/pull method bridges, arbitrary
  parameter/variable-scope semantics, image-triggered rebuild, and canonical
  Rust Relational selection remain open.

### R4.31 Rust Relational query-window bridge checkpoint (2026-07-25)

- `8aae48d` generalizes the Rust `findAll:where:` adapter across the frozen
  Store history selectors: `asOf:`, `since:`, and `between:and:`. It validates
  nonnegative SmallInt watermarks and bounds, reads the corresponding
  Store-owned live/history callback, reifies rules from that same window, and
  returns the shared Vector result family.
- The cross-floor window probe observes the expected 2/1/1 as-of/since/between
  cardinalities. Rust unit tests remain `17/17`; both dynamic targets remain
  `14/14`; the R6 benchmark allowances and thresholds are unchanged.
- Aggregate/count/sum/min/max/avg, grouped aggregate, pull, image-triggered
  rebuild, arbitrary parameter/variable-scope semantics, and canonical Rust
  Relational selection remain open.

### R4.32 Rust Relational scalar-aggregate bridge checkpoint (2026-07-25)

- `dd10317` installs Rust `count:where:`, `sum:where:`, `min:where:`,
  `max:where:`, and `avg:where:`. The adapter queries through the same
  Store-owned Vector path, copies values through the frozen classifier, and
  applies the pure Rust aggregate law; it does not depend on prelude `+` or `/`
  selectors being installed in the host floor.
- The cross-floor corpus exercises all five selectors over a predicate-first
  query and preserves the expected distinct result (`count 1`, numeric value
  `20` for the other four). Rust unit tests remain `17/17`; both dynamic
  targets remain `14/14`.
- Grouped aggregate, pull, image-triggered rebuild, arbitrary
  parameter/variable-scope semantics, and canonical Rust Relational selection
  remain open.

### R4.33 Rust Relational grouped-aggregate bridge checkpoint (2026-07-25)

- `e754c76` adds the pure rule-body grouping seam and installs
  `groupBy:aggregate:of:where:` on Store when the shared Map/Vector/LogicVar
  contracts are present. It rebuilds rules from the Store snapshot, groups in
  first-appearance order, folds distinct values with the Rust aggregate law,
  and materializes a shared Map without retaining Rust authority.
- The cross-floor corpus exercises a predicate-first grouped sum and observes
  `planner_high → 20`; Rust unit tests remain `17/17`, both dynamic targets
  remain `14/14`, and no R6 threshold or allowance changed.
- Pull materialization, image-triggered rebuild, arbitrary
  parameter/variable-scope semantics, and canonical Rust Relational selection
  remain open.

### R4.34 Rust Relational pull-materialization checkpoint (2026-07-25)

- `5bd0e57` installs `pull:spec:`. The adapter parses the shared Vector pull
  tree, resolves `:many` through Store's `cardinality:`, copies the Store
  snapshot once, and recursively materializes leaf and reference results using
  the shared Map/Vector families. It does not retain a Rust snapshot or bypass
  Store authority.
- The cross-floor corpus now verifies a scalar leaf and a nested reference
  pull (`planner_high` score `20`, nested friend score `5`). Rust unit tests
  remain `17/17`; both dynamic targets remain `14/14`.
- Image-triggered rebuild, arbitrary parameter/variable-scope semantics, and
  canonical Rust Relational selection remain open; pull GC/reentry stress must
  still be added before the R4 exit gate.

### R4.35 R6 acceptance after Relational selector-family bridge (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after
  `5bd0e57`: `100/100` build steps, `80/80` focused tests, and both Rust
  Relational dynamic targets at `14/14`.
- The optimized comparator remains unchanged from the prior accepted
  snapshot: only `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`
  regressions, with no new missing or unusable cases. The gate thresholds were
  not broadened.
- Relational GC/reentry stress, image-triggered rule-cache rebuild, arbitrary
  parameter/variable-scope semantics, and the single canonical Rust selection
  commit remain open.

### R4.36 Pull auto-GC stress checkpoint (2026-07-25)

- `05463b3` runs the nested Rust `pull:spec:` bridge with automatic GC enabled
  and allocation pressure before the materialization call. Both dynamic
  targets remain `14/14`, proving the current callback/root seam survives this
  bounded pull stress case.
- This does not close all reentry/GC obligations: larger result vectors,
  grouped Map growth, callback reentry, and image-triggered rule-cache rebuild
  still need dedicated stress fixtures before canonical selection.

### R4.37 Grouped Map auto-GC stress checkpoint (2026-07-25)

- `012333e` adds allocation pressure and automatic GC while the Rust grouped
  aggregate callback grows a 64-entry Map. Both dynamic targets remain
  `14/14`, proving the callback's active-root seam survives this Map-growth
  stress case.
- Broader query-result vectors, nested pull growth, callback reentry, and
  image-triggered rule-cache rebuild remain open R4 hardening obligations.

### R4.38 Canonical rule-cache rebuild checkpoint (2026-07-25)

- `588a89d` defines canonical stored ancestor rules, invokes the canonical Zig
  `rebuildRules` selector, and then runs the Rust-installed `findAll:where:`
  query against the rebuilt Store. The result still contains the expected
  `bob`, `ann`, and `kim` facts, proving that the Rust callback reconstructs
  durable rule definitions from Store rather than depending on Zig's in-memory
  query cache.
- Both dynamic targets remain `14/14`; this is a cache-rebuild observation
  seam, not a new gate count. Broader query-result-vector growth, callback
  reentry, and canonical Rust Relational selection remain open.

### R4.39 R6 acceptance after canonical rule-cache rebuild coverage (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after `588a89d`:
  `100/100` focused build steps, `80/80` focused tests, and both Rust
  Relational dynamic targets at `14/14`.
- The optimized comparator remains the exact accepted snapshot: only
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%` regressions,
  with no missing or unusable cases. The R6 gate accepted the latest test set
  without broadening thresholds or the recorded fixture exception.

### R4.40 Query-result Vector auto-GC checkpoint (2026-07-25)

- `188124d` runs the canonical Rust `findAll:where:` bridge against 64 live
  Store facts while automatic GC is enabled. The callback returns a complete
  64-entry shared Vector, complementing the nested pull and grouped Map growth
  fixtures.
- The Rust Relational target and its Zig-Store rollback target both remain
  `14/14`; callback reentry and canonical Rust Relational selection remain
  open hardening work.

### R4.41 R6 acceptance after query-result Vector auto-GC coverage (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after `188124d`:
  `100/100` focused build steps, `80/80` focused tests, and both Rust
  Relational dynamic targets at `14/14`.
- The optimized comparator remains unchanged: exactly the accepted
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%` regressions,
  with zero missing or unusable cases. The gate continues to accept the
  existing fixture exception without threshold broadening.

### R4.42 Relational canonical-selection checkpoint (2026-07-25)

- `fbf2ec9` adds the fail-closed `-Drelational=rust|zig` build choice. Both
  implementations are installed under explicit `_rust`/`_zig` fixture names,
  and the selected implementation is copied to canonical
  `libbirnam_id_relational.dylib`; Zig remains the default rollback choice.
- `test-relational-selection` builds both choices in isolated prefixes and
  verifies receipt identity, byte-for-byte canonical/fixture equality, and
  rejection of unknown implementations. No runtime Store authority is changed
  by the selection smoke.

### R4.43 R6 acceptance after Relational canonical-selection coverage (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after `8fc1697`:
  `103/103` focused build steps, `80/80` focused tests, and both Rust
  Relational dynamic targets at `14/14`.
- The optimized comparator remains the exact accepted snapshot: only
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%` regressions,
  with zero missing or unusable cases. The new selection target is now part of
  the fail-closed R6 boundary; no benchmark threshold was broadened.

### R4.44 Canonical Rust Relational mixed-contract acceptance (2026-07-25)

- `bdc58e3` delays the Rust Relational `db` export until Store, collection, and
  the complete selector bridge are installed; `7bb002b` keeps the Zig rollback
  fixture's own Store/Relational path explicit. This prevents a partial Rust
  export from making rollback bring-up import an incomplete contract.
- `c7d4d18` makes Zig Relational reuse an already-published LogicVar family even
  when `db` is not yet exported, and gives Rust Relational the shared
  LogicVar/Entity/collector methods and GC-rooted named-variable cache. Mixed
  Rust-Relational/Zig-Relational loading therefore preserves one canonical
  `LogicVar`/`LogicVarVt` family instead of minting competing vtables.
- The complete `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` gate passes:
  `103/103` focused build steps, `80/80` focused tests, both Rust Relational
  dynamic targets at `14/14`, optimized verify success, and exactly the two
  accepted comparator crossings (`query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%`) with zero missing or unusable cases. This
  closes the current R4 acceptance seam; post-switch project/image observation
  remains the next Relational checkpoint.

### R4.45 Rust Relational canonical switch and first observation (2026-07-25)

- `8c0fd13` changes the default `-Drelational` mapping to Rust. The explicit
  `-Drelational=zig` rollback path and installed
  `libbirnam_id_relational_zig.dylib` fixture remain available; the canonical
  install still carries both implementation receipts.
- The default-path ReleaseSafe `zig build verify -Disolate-impl=rust
  -Dfloor=rust` passes without a relational override: `266/266` build steps,
  `486/492` tests with the documented six skips, the 343-test self-hosted
  suite, project/image/REPL and relocation smokes, Rust/Zig lower-rung
  matrices, and relational/prelude/TCC selection smokes. No new benchmark
  allowance was introduced; the R6 comparator remains pinned to its two
  accepted crossings.
- This begins the Relational post-switch observation window. Keep the Zig
  Relational and Store fixtures explicit in cross-floor image/project tests;
  do not retire either rollback artifact until the observation requirements in
  Design 62 §5.4 are met.

## 8. Phase R4 — Relational

1. Port the query engine against the frozen Store object/message protocol only
   after R1. Exercise the Rust Relational implementation against both the live
   Rust Store and the live-or-pinned Zig Store rollback fixture; the boundary is
   not a Rust-private callback surface. Read live triples from Store and never
   cache a second authoritative fact set.
2. Port clauses left-to-right: constants/logic variables, joins, predicates,
   negation, disjunction, recursive rules, deterministic ordering, then pull
   and time-window reads. Preserve the existing no-reorder semantics.
3. Port aggregation (`count`, `sum`, `min`, `max`, `avg`, `groupBy`) with exact
   empty/type/error behavior and deterministic Map/Vector results.
4. Add cross-floor query fixtures for single/join/negation/rule/pull/asOf and
   all `liveTriplesDo:`/`AsOf`/`Since`/`Between` variants, using both Store
   implementations. Prove the full Store MOP consistency law, rule-cache
   rebuild after image load, GC during collector callbacks, and callback
   reentry.
5. Run the query benchmark corpus and the Rust-Relational × Zig-Store rollback
   combination. Any regression must be classified against the existing R6
   allowances; do not hide it behind a broader threshold.
6. Treat parity, rollback, benchmark, full verify, and R6 as the pre-switch
   acceptance. Make one canonical Rust Relational selection commit, then close
   the post-switch observation window with project/image workflows; retain the
   Zig Relational and Store fixtures for rollback.

**R4 exit gate:** relational results and Store authority match across Rust and
Zig Store combinations, query and collection performance are accepted,
pre-switch full verify/R6 passes, one canonical switch commit lands, and the
post-switch image/project observation window closes.

## 9. Phase R5 — Alien

### R5.1 Authority-free signature grammar checkpoint (2026-07-25)

- `55893c6` adds the `birnam-alien` Rust crate as an authority-free parser
  for the frozen Alien signature grammar. It accepts the scalar, pointer,
  nested aggregate, and fixed scalar-array forms used by the Zig floor,
  registers nested aggregates post-order, ignores only the grammar's ASCII
  readability whitespace, and checks array counts for overflow.
- The parser has no `IdApi`, allocator, dynamic-loader, TCC, callback, or C
  ABI state. Five unit tests cover valid nesting and whitespace plus empty,
  malformed, top-level-array, overflow, and non-UTF-8 inputs; locked offline
  tests and `clippy -D warnings` pass.
- This is deliberately only the pure grammar seam. Loader ownership,
  classification, generated TCC shims, wrapped calls, buffers, finalizers,
  and callbacks remain in the numbered R5 steps below and no canonical Alien
  selection has been made.

### R5.2 Owned loader-state checkpoint (2026-07-25)

- `c7a80c8` adds the Rust `LibraryRegistry`, which owns one closeable entry
  per successful `dlopen`, preserves requested paths as replay keys, resolves
  bare names through an explicit library directory, and provides both
  handle-scoped and process-default `dlsym` lookup.
- Empty paths, embedded NULs, missing handles, empty symbols, and unresolved
  symbols fail closed before crossing the platform loader. Repeated opens are
  balanced one close at a time; registry drop closes any remaining owned
  references. The crate tests cover these policies against the host C library.
- The registry remains authority-free: no Oop, `IdApi`, TCC unit, wrapper
  address, callback root, or GC state is stored here. A later Alien integration
  slice must keep the registry alive for every patched wrapper and must not
  close a library while a live wrapper can still call it.

### R5.3 ABI metadata checkpoint (2026-07-25)

- `13280f1` adds pure Rust classification over the parsed signature tree:
  frozen integer, unsigned, float, string-pointer, pointer, and void classes,
  aggregate indices, and homogeneous-float metadata for one-to-four float or
  double members, including nested aggregates and fixed scalar arrays.
- The classifier does not calculate C layout, alignment, hidden sret
  parameters, or a platform calling convention. Those decisions remain with
  the existing TCC compiler, so Rust cannot become a second ABI authority.
  Malformed future trees classify conservatively rather than inventing a
  top-level array ABI.
- Ten locked offline crate tests and `clippy -D warnings` pass across the
  parser, loader, scalar classes, HFA cases, nested aggregates, and general
  aggregate cases. TCC shim generation and IdApi integration remain separate
  R5 steps.

### R5.4 R6 workspace-gate repair and acceptance (2026-07-25)

- Adding `birnam-alien` to the Cargo workspace exposed a stale hard-coded
  package list in `scripts/install-smoke.sh`. `6e881dd` updates that gate to
  enumerate the new member; no production dylib or implementation selection
  is implied by this workspace-only crate.
- The corrected install target passes `114/114` steps, including relocated
  source/closure, project, image, isolate, and missing-resource diagnostics.
  The full `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` rerun passes
  `103/103` focused steps and `80/80` tests, with both Relational variants at
  `14/14`, the optimized verify and clean native-rung harness green, and zero
  missing or unusable benchmarks.
- The comparator remains exactly the existing two accepted crossings:
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`. This repairs the
  gate's workspace inventory without weakening any floor, MOP, Store, ABI, GC,
  or dynamic-library invariant.

### R5.5 Deterministic generated-C declaration checkpoint (2026-07-25)

- `275917b` adds a pure Rust generator for post-order aggregate typedefs and
  C function prototypes. It emits valid scalar, pointer, nested-aggregate,
  and fixed-array field syntax with stable formatting, while rejecting invalid
  C identifiers, malformed aggregate references, and top-level arrays before
  TCC is called.
- This slice emits declarations only. It introduces no IdApi pointers,
  allocator, TCC state, wrapper address, callback, or GC authority; TCC remains
  responsible for the platform ABI and hidden sret convention. Twelve locked
  offline crate tests plus `clippy -D warnings` pass.
- The next R5 step can add the runtime preamble and wrapper body against this
  stable declaration output, then compile it through the existing Rust TCC
  boundary.

### R5.6 Scalar wrapper-shim checkpoint (2026-07-25)

- `0b8e09c` extends the deterministic generator with scalar-only method-ABI
  bodies. The generated shim maps the receiver to the first C argument,
  indexes remaining arguments, checks exact method arity, unboxes the frozen
  scalar classes, frees temporary string arguments after the call, and boxes
  integer/unsigned/float/string/pointer/void results through the existing
  `birnam_*` runtime declarations.
- Aggregate arguments and returns fail explicitly with
  `UnsupportedAggregateShim`; no Vector layout, IdApi pointer, callback, or GC
  authority has been introduced. Fourteen locked offline crate tests and
  `clippy -D warnings` pass.
- The generated body is still not installed or selected as Alien runtime code.
  Aggregate Vector marshalling and compilation through the existing Rust TCC
  boundary remain the next R5 seams.

### R5.7 Aggregate Vector marshalling checkpoint (2026-07-25)

- `6bd52ee` adds deterministic generated-C helpers for every parsed aggregate:
  `__unmN` checks Vector `size` and each `at:` element before untagging nested
  structs, scalar fields, or fixed arrays; `__mkN` builds fresh Vectors through
  `new`/`add:` and recursively handles nested aggregates and arrays.
- Aggregate fields containing StringPointer or Void are rejected before code
  emission, and aggregate references are validated against the parsed
  post-order table. The helper source carries no IdApi, allocator, callback,
  or GC state; a wrapper will still decide how a failed unmarshal answers nil.
- Sixteen locked offline crate tests and `clippy -D warnings` pass. The next
  seam is composing these helpers into a complete aggregate wrapper and then
  compiling generated C through the existing TCC boundary.

### R5.8 Complete generated wrapper composition checkpoint (2026-07-25)

- `c440d9c` adds `emit_shim`, composing the scalar method-ABI body with the
  aggregate `__unmN`/`__mkN` helpers. Receiver-first arguments, exact method
  arity, scalar temporary cleanup, aggregate shape failure-to-nil, and
  aggregate return construction are now represented in one deterministic
  generated unit.
- The source still only declares the frozen `birnam_*`/Vector protocol; it is
  not installed as an Alien dylib and does not own IdApi, TCC, callback, or GC
  state. Seventeen locked offline crate tests and `clippy -D warnings` pass.
- The next bounded step is compiling this generated unit through the existing
  Rust TCC crate and checking the returned `__shim` address, before any
  callback or live IdApi method installation is attempted.

### R5.9 Rust-TCC generated-source validation checkpoint (2026-07-25)

- `9098317` exposes the existing embedded Rust TCC owner as an `rlib` for
  tests and adds a non-production validation bridge. It registers inert
  runtime/foreign symbols, compiles and relocates generated scalar Alien C,
  and verifies that TCC returns a non-null `__shim` address.
- `birnam-alien` depends on `birnam-tcc` only under `[dev-dependencies]`; the
  production Alien crate does not link a second TCC implementation or acquire
  IdApi authority. The bridge is not an object selector or C export.
- Eighteen locked offline Alien tests and `clippy -D warnings` pass, including
  the end-to-end generated `strlen` wrapper compilation. Aggregate generated
  source remains source-tested only until a Vector/IdApi integration fixture
  supplies the real factory and runtime shims.

### R5.10 Aggregate Rust-TCC validation checkpoint (2026-07-25)

- `bc88dda` extends the non-production Rust-TCC bridge test to a complete
  aggregate wrapper. The generated nested Vector unmarshal/marshal helpers,
  receiver-first aggregate call, scalar argument, and aggregate result now
  compile, relocate, and expose `__shim` through the same embedded TCC owner.
- This test caught and fixed two generator defects before the checkpoint was
  accepted: aggregate field unmarshal emitted an invalid declaration instead
  of an assignment, and integer aggregate boxing omitted a closing parenthesis.
  The source generator now has a real compile/relocate check rather than only
  substring assertions for this path.
- Nineteen locked offline Alien tests, the Rust TCC tests, and both crates'
  `clippy -D warnings` pass. The bridge remains test-only; live Vector/IdApi
  integration, wrapper installation, callbacks, and canonical Alien selection
  are still future R5 steps.

### R5.11 Rust Alien live loader-owner checkpoint (2026-07-25)

- `57fd716` converts `birnam-alien` to a `cdylib`/`rlib` over the frozen
  `birnam-abi` and adds its first live module boundary. `module_init` now
  creates or reloads the canonical `AlienVt`/`Alien` family, installs
  `libraryDir:`, `dlopen:`, and balanced `dlclose:` selectors, exports the
  Rust identity, and keeps the object authority in the process IdApi.
- The runtime owns one process registry for native handles, uses the pure
  loader's explicit bare-name directory policy, answers successful opens and
  closes with the canonical booleans, and maps close failures to the existing
  Error factory when that family is present. It does not yet install generated
  TCC wrappers, callbacks, raw buffers, or a second GC/Store authority.
- Native loader handles are stored as opaque addresses behind a mutex rather
  than asserting Rust `Send` for `NonNull<c_void>`; the platform remains the
  sole owner of their semantics. Nineteen locked offline tests, release build,
  and `clippy -D warnings` pass. The next seam is wiring the generated shim
  into this live owner while retaining the existing Rust TCC authority.

### R5.12 Rust Alien live wrapper checkpoint (2026-07-25)

- `a6f46c8` drives the existing TCC object through IdApi for
  `wrap:signature:as:on:`: Rust Alien copies the signature/function names out
  of host Strings, parses and generates the deterministic unit, resolves the
  function through its loader registry, then performs `new`, `compile:`,
  `bindRuntime`, `addSymbol:value:`, `relocate`, and `install:as:on:`. TCC
  remains the sole compiler, relocation, and platform-ABI authority.
- The generated scalar/aggregate paths preserve receiver-first method ABI,
  exact arity, Vector-factory binding, and shape-failure-to-nil behavior. Error
  construction is attempted only through the exported Error factory; missing
  Error remains fail-closed. Loader locks and borrowed argument/string spans
  are released or copied before any reentrant IdApi send.
- `b0672a1` adds the `test-rust-alien-live` fixture gate and installs the Rust
  Alien dylib under an explicit `libbirnam_id_alien_rust.dylib` name while Zig
  remains the canonical Alien fixture. The gate loads Rust TCC and Rust Alien,
  balances a real platform `dlopen`/`dlclose`, wraps libc `strlen`, and dispatches
  the generated method successfully on both Zig/Debug and Rust/ReleaseSafe
  selected floors. The full Alien callback, buffer, replay, GC-lifetime, and
  canonical-selection gates remain future R5 work.

### R5.13 Rust Alien aggregate live checkpoint (2026-07-25)

- `1511a28` extends the live fixture with the Rust Collection owner and a real
  aggregate wrapper for libc `div` (`{ii} i i`). The test dispatches the
  receiver-first method, verifies the Rust-generated `__unm1`/`__mk0` path, and
  checks the returned Vector shape and quotient/remainder values.
- The same gate still balances the loader handle and scalar `strlen` wrapper;
  it passes with Zig/Debug and Rust/ReleaseSafe selected floors. Rust Alien,
  Rust Collection, and Rust TCC are all loaded as explicit fixtures; no
  canonical Collection or Alien selection has changed.
- This proves live aggregate marshalling against a real Vector factory, but it
  does not yet cover nested/array ABI corpus parity, callbacks, raw buffers,
  replay records, forced-GC roots, unload safety for installed wrappers, or the
  pre-switch/full R6 acceptance required by the R5 exit gate.

### R5.14 Rust Alien callback ownership checkpoint (2026-07-25)

- `f0a72ca` adds deterministic Rust callback generation. The generated C boxes
  each C argument, sends the arity-matched `value` selector to a patched
  `__cbBlock`, and unboxes scalar or aggregate results. Aggregate callbacks
  reuse the same Vector `__mkN`/`__unmN` helpers as wrappers; string returns are
  rejected because their C-side ownership is undefined.
- The Rust live owner registers one process-wide IdApi GC root provider and
  stores only integer addresses for each `(block, TCC unit, callback address)`
  entry. The provider copies roots before marking, and no registry lock or Rust
  borrow crosses a callback/reentrant `send`. `releaseCallback:` removes the
  entry before marking its TCC unit `disposable`; callers explicitly promise not
  to invoke the released address again.
- The live fixture exports scalar and aggregate callback harnesses, forces GC
  churn, verifies callback dispatch and Vector marshalling, then checks release
  and double-release failure. It passes `2/2` tests with Zig/Debug and
  Rust/ReleaseSafe selected floors. Rust callback identity remains the existing
  `rust-alien-r5-loader` fixture identity until the complete Alien owner is
  ready for canonical selection.

### R5.15 Rust Alien raw-buffer checkpoint (2026-07-25)

- `cb284d6` adds the Rust raw-memory protocol: `alloc:`/`free` plus typed
  `peekIntAt:`, `peekByteAt:`, `peekPtrAt:`, `peekDoubleAt:`,
  `peekStringAt:`, and matching `poke*:at:` methods. Allocation is zeroed with
  the platform allocator; typed operations intentionally preserve the frozen
  unsafe Word contract and use unaligned reads/writes like the Zig owner.
- The implementation keeps the IdApi/Store authority boundary intact: raw
  addresses are carried only as Words, strings are copied into managed values,
  and all failure construction goes through the existing Error factory. No
  Rust borrow or mutex guard crosses a reentrant send.
- The live gate now allocates one buffer, round-trips signed integer, double,
  byte/string, and pointer values, and frees it explicitly. It passes `3/3`
  tests with Zig/Debug and Rust/ReleaseSafe selected floors. Buffer bounds and
  pointer provenance remain deliberately caller-owned unsafe obligations, as
  they are in the Zig protocol.
- `7652859` folds the live target into R6 acceptance as a required sequential
  sub-gate. The focused matrix remains `80/80` tests; the separate live target
  is asserted at `3/3` to avoid racing its Cargo install against clean-prefix
  selection scripts. With the canonical Zig Alien install restored, full R6
  passes with only the two already-recorded benchmark tradeoffs.

### R5.16 Rust Alien live ABI-corpus checkpoint (2026-07-25)

- `649bb94` extends the live Rust Alien fixture with an initial ABI corpus:
  signed scalar identity, opaque pointer identity, floating-point addition,
  void return, and a fixed-array aggregate (`{l3i}`) returned through a real
  Vector value. The fixtures are retained explicitly so ReleaseSafe cannot
  dead-strip the exported symbols. This complements the existing scalar,
  aggregate, callback, GC-churn, and raw-buffer cases without changing the
  frozen receiver-first ABI or moving compiler/Vector authority into Rust.
- The Rust-floor live target passes `4/4` tests in ReleaseSafe. `f05925d`
  updates R6 to assert that exact count in its sequential Rust Alien sub-gate;
  the focused matrix remains `80/80`, and the full R6 acceptance continues to
  pass with the existing benchmark tradeoffs. The corpus is intentionally an
  initial parity slice; hostile signatures, broader platform ABI shapes,
  unload-after-wrapper, and child-abort coverage remain in the R5 exit work.

### R5.17 Rust Alien hostile-input checkpoint (2026-07-25)

- `c1ad6b4` loads the standalone Rust Error owner into the Rust Alien live
  fixture and exercises malformed/empty/unclosed/unsupported signatures,
  missing symbols, and embedded-NUL names. Every failure is checked as a
  non-crashing, kinded Error (`badSignature`, `symbolNotFound`, or
  `typeMismatch`). The missing-symbol mapping was corrected to preserve the
  established Alien distinction from `loadFailed`.
- `9b9f4c7` raises the sequential R6 Rust Alien sub-gate to `5/5`. Debug and
  ReleaseSafe live targets pass, while the focused matrix remains `80/80` and
  the full R6 comparator still records only the two accepted benchmark
  tradeoffs. Broader platform ABI shapes, unload-after-wrapper, and
  child-abort coverage remain in the R5 exit work.

### R5.18 Rust Alien unload-safety checkpoint (2026-07-25)

- `2eb82f7` records installed Rust wrappers as live native-code users. When a
  tracked library has any live wrapper, `dlclose:` refuses to release that
  handle with `#inUse` instead of leaving a TCC trampoline pointing into
  unmapped code. The guard is deliberately conservative because a wrapper's
  process-owned `dlsym(RTLD_DEFAULT)` lookup does not expose a portable
  library identity; it never closes an unrelated handle by guessing.
- The live fixture proves the guard by opening the platform C library after a
  `strlen` wrapper is installed and requiring the kinded refusal. Debug and
  ReleaseSafe pass `6/6`; `3855938` makes that count an R6 requirement. The
  existing explicit unsafe buffer/callback release contracts remain caller
  obligations, while broader platform ABI shapes and child-abort coverage stay
  in the R5 exit work.

### R5.19 Rust Alien broader ABI-shape checkpoint (2026-07-25)

- `f430ab2` extends the live corpus with homogeneous float and double pairs,
  a mixed integer/double aggregate, and a large three-word aggregate return.
  These cases exercise Vector marshalling across HFA/general aggregate classes
  and the platform-sensitive by-value return path while leaving layout,
  alignment, and hidden return-parameter decisions to TCC.
- The Debug and ReleaseSafe live targets pass `7/7`; `dbf8880` raises the
  sequential R6 Rust Alien requirement accordingly. The focused matrix remains
  `80/80`, with full R6 still limited to the two accepted benchmark tradeoffs.
  Larger hostile ABI matrices, callback shape parity, and child-abort coverage
  remain in the R5 exit work.

### R5.20 Rust Alien hostile-callback checkpoint (2026-07-25)

- `8737309` adds live callback failure coverage for string returns, malformed,
  empty, and unclosed signatures, plus the canonical `nil` object as a block.
  The last case fixed a real boundary gap: Rust previously rejected only a
  null pointer, not the host's nil Oop. All cases now answer kinded errors and
  never enter TCC or generated callback code.
- `5b02793` raises the sequential Rust Alien R6 sub-gate to `8/8`. Debug and
  ReleaseSafe pass, the focused matrix remains `80/80`, and full R6 continues
  to accept only the two recorded benchmark crossings. Child-abort and wider
  callback ABI-shape coverage remain in the R5 exit work.

### R5.21 Rust Alien callback ABI-shape checkpoint (2026-07-25)

- `09d8411` adds a live callback corpus for double and float scalar
  marshalling, `void` returns, and aggregate returns. The fixtures route
  through generated TCC callbacks and the existing Rust registry, so the
  coverage exercises native ABI classification in both directions rather than
  only testing parser/code-generator strings.
- The Rust Alien live target passes `9/9` in both Debug and ReleaseSafe;
  `4264760` raises the sequential R6 requirement to that exact count. The
  focused matrix remains `80/80`, and the existing two benchmark allowances
  are unchanged. Child-abort coverage remains the final explicit R5 exit
  seam before the pre-switch acceptance run.

### R5.22 Rust Alien child-abort checkpoint (2026-07-25)

- `c285450` extends the established Rust FFI abort smoke to the Alien cdylib.
  Feature-gated hooks prove that both a Rust panic and the Alien fatal-invariant
  helper terminate in the child with SIGABRT and the required diagnostic in
  Debug and Release profiles; production builds do not expose the hooks.
- `b4cf834` makes `test-rust-ffi-abort` a required focused R6 target and pins
  both Alien profile summaries. The Rust Alien live target remains `9/9`, the
  focused semantic matrix remains `80/80`, and no benchmark allowance changed.
  This closes the explicit child-abort seam; the remaining R5 work is the
  pre-switch full verify/R6 acceptance and canonical selection decision.

### R5.23 Rust Alien canonical selection (2026-07-25)

- `7ff15fe` makes the Rust Alien dylib the installed canonical
  `libbirnam_id_alien.dylib`. The Zig implementation remains available as
  `libbirnam_id_alien_zig.dylib`, while the Rust fixture remains independently
  addressable as `libbirnam_id_alien_rust.dylib`; rollback therefore does not
  require rebuilding either owner.
- Post-switch focused observation passes: the existing Alien corpus is
  `43/43` and the Rust live-owner corpus is `9/9` in ReleaseSafe. The full
  cross-layer verify/R6 run is the next required observation boundary; no
  benchmark allowance or threshold is changed by the selection.

### R5.24 Rust Alien post-switch acceptance (2026-07-25)

- The post-switch `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` run
  passes with Rust canonical under the installed `Alien` name. The focused
  replacement boundary remains `80/80`, the required Rust FFI/Alien abort
  target passes, and the sequential Rust Alien live target remains `9/9`.
- The optimized cross-layer verify retains the recorded install-smoke fixture
  exception only; the benchmark comparator reports exactly the two accepted
  crossings (`query.aggregate` and `compile.stage1-stdlib`), with no missing
  or unusable cases. Zig remains an explicit rollback fixture and no selection
  or performance threshold was broadened.

### R6.1 Rust Image dynamic-shell checkpoint (2026-07-25)

- `42f63c5` converts `birnam-image` to a `cdylib`/`rlib` pair over the frozen
  `IdApi` and adds the first dynamic shell. It preserves the zero-payload
  `ImageSaveReport` family, exports `ImageCollectorVt`, installs the complete
  System selector surface, and retains the borrowed v4 codec as the lower
  framing authority. Save/load/bake selectors intentionally answer nil until
  their orchestration authorities are ported, so this shell cannot publish a
  partial image.
- `98adc7d` installs explicit Zig and Rust Image fixtures, and `1aac0d7`
  adds the Debug/ReleaseSafe live shape gate (`1/1`). `82f13ab` makes that
  target a required focused R6 input. Rust Image is not canonical; the next
  slice ports the atomic save transaction behind this exact surface.

### R6.2 Rust Image report-state checkpoint (2026-07-25)

- `ffc71b4` replaces the shell's constant `never`/zero/empty report with a
  process-owned, mutex-protected `SaveReport` state matching the frozen
  `ImageSaveReport` selectors and status vocabulary. `saveImage:` now validates
  arity, argument-buffer shape, and String-path type before it fails closed;
  each rejection resets counters and records a bounded diagnostic detail.
  A well-typed path still rejects with `Rust Image save orchestration is not
  installed`, because this slice does not yet have authority to walk the heap,
  write v4 records, or publish a replacement image.
- The Rust Image live gate now proves both malformed-input and well-typed
  fail-closed report transitions in Debug and ReleaseSafe. The Rust unit suite
  remains green (`13/13`). Rust Image is still not canonical; no filesystem
  mutation is introduced by this slice.

The next R6 slice may add the temp-file/atomic-publish transaction only after
the Rust implementation has heap/fact preflight authority. It must retain this
report state on every rejection and leave an existing destination byte-for-byte
unchanged when preflight or publication fails.

### R6.3 Rust Image v4 writer checkpoint (2026-07-25)

- `bc41140` adds `ImageWriter`, a checked allocation-owning framing layer over
  the frozen v4 bytes. It emits the canonical header plus caller-encoded facts,
  heap, authority, and code records; enforces one authority, authority-before-
  code, one code record, and generation-digest equality; and refuses to finish
  without an authority. The code record retains the existing checksum field and
  does not recompute or weaken it.
- The writer round-trips through the borrowed `Image::parse` path, and the
  ordering/error corpus is covered by the Rust unit suite (`15/15`). It does
  not walk the live heap, resolve exports, mutate Store state, write files, or
  publish an image. `saveImage:` therefore remains the report-state shell and
  still fails closed.

The next bounded slice can connect this writer to a pure preflight result (with
no heap mutation) and then exercise temp-file publication against a preserved
destination. Only after those checks are live should `saveImage:` gain file
authority.

### R6.4 Rust Image atomic-publication checkpoint (2026-07-25)

- `b2d9a5d` adds `publish_atomic`, which writes complete bytes to a collision-
  safe same-directory temporary file, flushes the file, and renames it into
  place. It never truncates the destination first; failed writes remove their
  temporary file and preserve an existing destination. The helper accepts only
  already-built bytes and has no runtime or heap authority.
- The Rust unit suite now covers replacement and rename-failure cleanup
  (`17/17`). The dynamic shell remains intentionally disconnected from this
  helper until heap/fact preflight can produce a complete v4 byte vector.

The next R6 slice should port a pure preflight result that supplies the writer
with exact authority/facts/heap payloads. Only then may `saveImage:` call this
publisher, with the report counters and rejection detail set before any
publication attempt.

### R6.5 Rust Image field-encoding checkpoint (2026-07-25)

- `1e6ffad` adds `FieldEncoder` for every frozen v4 tagged value: singleton
  markers, signed integers, lossless float bits, symbols/strings/export names,
  heap relocation ids, and null-Oop slots. Length-delimited values are bounded
  to the on-disk `u32` envelope and all output remains caller-owned bytes.
- The encoder round-trips through the existing borrowed `Fields` decoder for
  the complete tag corpus. The Rust unit suite is green (`18/18`). No Oop is
  classified implicitly, no heap is walked, and no Store or filesystem state is
  changed; the dynamic shell remains fail-closed.

The next bounded step can use this encoder in a pure fact-record builder, then
feed those records to `ImageWriter` and `publish_atomic` under an explicit
preflight result. Runtime object classification must remain a separate seam.

### R6.6 Rust Image facts-builder checkpoint (2026-07-25)

- `88d3525` adds `FactsWriter`, which emits the v4 basis/count envelope and
  appends each operation atomically as `op`, `tx`, and three classified fields.
  It reuses `FieldEncoder` for all value tags and rejects count overflow before
  changing the payload.
- The facts builder round-trips through the borrowed `Facts` decoder, including
  signed integers, symbols, booleans, and float-bit preservation. The Rust unit
  suite is green (`19/19`). It has no Store access and cannot infer an Oop's
  durability or recovery contract; those decisions remain in the live preflight
  layer.

The next bounded slice should introduce that explicit preflight classification
for one store fact at a time, with rejection reasons mapped into `SaveReport`
before any writer or publisher call.

### R6.7 Rust Image store-fact preflight checkpoint (2026-07-25)

- `527f89e` wires the Rust Image collector to the existing `Store
  imageFactsInto:` protocol for preflight only. It classifies null/singleton,
  integer, float-bit, symbol, string, and exported-object values through the
  Rust field encoder; unsupported compound values are rejected as whole facts.
  Accepted/rejected counts are reflected in `ImageSaveReport`, while the
  encoded facts are discarded until heap authority exists.
- The live Rust Image gate now brings up the Store, drives a real fact stream,
  and proves nonzero preflight accounting in Debug and ReleaseSafe. The Rust
  codec suite remains green (`19/19`). `saveImage:` still never writes or
  renames a file: it reports either unsupported fact values or the missing
  Rust heap preflight authority.

The next bounded slice is heap preflight for one exported-vtable object shape,
including full layout validation and relocation discovery. It must not be
enabled for `saveImage:` until the object walk and its rejection accounting are
complete.

### R6.8 Rust Image raw-heap preflight checkpoint (2026-07-25)

- `ed712e6` adds a relocation-aware `HeapPreflight` for the first supported
  shape: objects whose vtable (or owned parent vtable) is exported and whose
  payload contains only exact raw/code slots. It validates payload alignment,
  slot descriptors, vtable relinking names, and relocation ids, then emits the
  frozen heap payload for inspection. Unsupported or pointer-bearing layouts
  reject the whole fact before any image bytes are published.
- The Rust Image live test exports a zero-payload fixture vtable, stores a real
  fixture object, and drives the Rust collector through Store. Debug and
  ReleaseSafe pass; the codec suite remains green (`19/19`). The produced heap
  and facts payloads are still discarded because authority binding, full Oop
  graph traversal, and writer/publisher orchestration are not yet connected.

The next bounded step should extend this preflight to one exact Oop-slot shape,
including recursive relocation discovery and cycle termination, while keeping
unknown layouts and unsupported capabilities fail-closed.

### R6.9 Rust Image recursive Oop-slot checkpoint (2026-07-25)

- `36e3a1b` extends `HeapPreflight` to exact Oop slots. It assigns relocation
  ids before descending, recognizes inline scalar/name values, recursively
  discovers nested exported-vtable objects, and emits tagged Oop fields while
  preserving cycle termination through the relocation map. Raw and code slots
  retain their prior layout rules; unknown slot kinds still reject.
- The live Rust Image fixture now stores a one-slot parent pointing at the
  exported zero-payload fixture, proving recursive discovery in Debug and
  ReleaseSafe. The codec suite remains green (`19/19`), and no file is written
  because writer/publisher authority is still deliberately disconnected.

The next bounded step should add a cycle fixture and make heap rejection counts
explicit in `SaveReport` before connecting the complete preflight byte vector to
the writer.

### R6.10 Rust Image cycle/rejection-accounting checkpoint (2026-07-25)

- `c9ed401` makes heap preflight slot accounting explicit. Code slots increment
  `recoveredSlots`; a nested Oop that cannot be encoded increments
  `rejectedSlots`; and both counters are copied into the stateful
  `ImageSaveReport` alongside the fact counts. Root objects that have no
  serializable vtable remain whole-fact rejections, while a serializable parent
  with an unsupported nested child reports the rejected heap slot and fails
  closed.
- The live Rust Image fixture now includes a two-object Oop cycle and an
  exported parent pointing at an unexported child. The cycle proves relocation
  assignment-before-recursion terminates; the rejected child proves the report
  exposes nonzero heap rejection accounting and the exact heap-preflight detail.
  Debug and ReleaseSafe focused gates pass, and the Rust codec suite remains
  green (`19/19`). `saveImage:` still does not write or rename a file.

The next bounded step can connect the complete preflight facts/heap byte vectors
to `ImageWriter` without publication. It must preserve these counters and keep
the writer input immutable until authority and code records are available.

### R6.11 Rust Image writer-preflight checkpoint (2026-07-25)

- `ed440f2` connects the collected Rust facts and heap payloads to the checked
  `ImageWriter` in memory. The bridge uses the target architecture tag and the
  frozen runtime-shim ABI hash, accepts facts/heap records only after the live
  preflight has produced them, and reports a distinct writer rejection if the
  framing layer refuses either payload. It never calls `finish` without the
  authority record, so no incomplete image bytes are published.
- The live focused test first proves the cycle/rejection path, retracts the
  unsupported parent, then proves a fully supported facts/heap stream reaches
  the writer and stops at the explicit authority/code boundary. Debug and
  ReleaseSafe pass; the codec suite remains green (`19/19`); `saveImage:` still
  performs no filesystem mutation.

The next bounded step can introduce a pure authority digest input and code-unit
placeholder to exercise `ImageWriter::finish` entirely in memory. The dynamic
selector must remain fail-closed until that authority is derived from the
canonical generation and the code record has a real mapped/rebuild contract.

### R6.12 R6 acceptance after Rust Image writer preflight (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after `ed440f2`
  and `fb2b741`. The focused matrix remains `81/81`, Rust Alien live remains
  `9/9`, the optimized verify/install/harness legs pass, and the full acceptance
  reports no missing or unusable cases.
- The comparator still has exactly the two existing snapshot allowances:
  `query.aggregate` `+5.8%` and `compile.stage1-stdlib` `+11.9%`. No new
  allowance was added for the Rust Image writer-preflight bridge. `saveImage:`
  remains fail-closed and performs no filesystem mutation.

### R6.13 Rust Image complete-composer checkpoint (2026-07-25)

- `672f41a` adds the pure `compose_complete_image` seam. It accepts already
  preflighted facts/heap payloads plus explicit architecture, ABI, authority
  digest, code checksum, and code-unit inputs; it emits a complete checked v4
  byte vector through `ImageWriter::finish`. The seam does not derive authority,
  inspect runtime objects, or publish bytes, so the dynamic shell's authority
  boundary is unchanged.
- The parser-compatibility test now exercises this complete-composer path with a
  synthetic digest and code unit; the Rust codec suite remains green (`19/19`).
  `saveImage:` still stops before this seam until the canonical generation and
  mapped/rebuild code-unit contract are available.

The next bounded step should derive an authority digest from the live generation
without publishing, then compare that digest against the existing Zig image
authority fixture. A synthetic digest must remain test-only.

### R6.14 Rust Image live-authority probe checkpoint (2026-07-25)

- `506e3b8` adds a fail-closed probe for the canonical `Codegen
  currentGenerationDigest` selector. It accepts only the live String form with
  exactly 64 bytes, copies no process pointer into Rust state, and uses the
  result only to distinguish “generation authority unavailable” from the later
  code-capture/rebuild boundary. Missing `Codegen`, noncanonical generations,
  wrong vtables, malformed lengths, and null byte buffers all remain rejected.
- The focused Rust Image test now proves the no-Codegen path reports
  `Rust Image generation authority is not available` after a supported
  facts/heap preflight. The pure composer test also checks exact 64-byte digest
  validation; Debug and ReleaseSafe pass and the codec suite remains green
  (`19/19`). No digest is exposed as a new selector, and `saveImage:` still
  performs no filesystem mutation.

The next bounded step should exercise this probe in a setup with the canonical
Zig Codegen generation initialized, compare the returned bytes with the Zig
image authority fixture, and retain the Rust shell's no-publication boundary.

### R6.16 R6 acceptance after live-generation probing (2026-07-25)

- `BIRNAM_R6_ISOLATE_IMPL=rust mise run r6-acceptance` passed after `506e3b8`
  and `4020411`. The focused matrix remains `81/81`, Rust Alien live remains
  `9/9`, optimized verify/install/harness legs pass, and there are no missing or
  unusable acceptance cases.
- The benchmark comparator is unchanged and still accepts only
  `query.aggregate` `+5.8%` and `compile.stage1-stdlib` `+11.9%`. The probe is
  diagnostic-only; it adds no filesystem, Store, codegen, or publication
  authority to Rust Image.

1. Port library handle ownership and `dlopen`/`dlsym`/`dlclose` policy with
   explicit path and dependency validation.
2. Port signature parsing and C ABI classification (scalar, pointer, float,
   nested struct, fixed scalar arrays, HFA/sret cases) as a pure Rust layer.
3. Port TCC shim generation/compilation through the existing Rust TCC boundary;
   generated C remains deterministic and retains the frozen `birnam_*` symbols.
4. Port wrapped calls and result marshalling, then raw buffers and finalizers.
5. Port callbacks last. Root `(block, unit)` pairs through the runtime root
   provider, reject release/double-release/unknown addresses, and prove no Rust
   reference or lock crosses a callback or reentrant send.
6. Run the complete ABI-shape corpus, hostile signatures, callback lifetime,
   GC, library unload, and child-abort tests under both floors.
7. Treat ABI, callback, GC, install, full verify, and R6 results as the
   pre-switch acceptance. Make one canonical Rust Alien selection commit, then
   observe project/image workflows and retain the Zig Alien as rollback.

**R5 exit gate:** ABI corpus parity, callback ownership/lifetime proofs,
deterministic generated C, no unwind across C, pre-switch full R6/verify/install,
one canonical switch commit, and post-switch observation.

## 10. Phase R6 — Image orchestration

The Rust image crate already owns codec primitives; this phase ports the Zig
orchestration around them.

1. Convert the current Rust `birnam-image` library from its `rlib` shape to
   the canonical `libbirnam_id_image.dylib` `cdylib`, preserving `module_init`,
   the System image selectors, the `ImageSaveReport` family, collector/layout
   registration, and `bakeExe:`. Add an exact export, selector, family, and
   payload-layout parity gate before testing codec cross-products.
2. Port the image module's authority checks, one-generation binding, record
   ordering, and atomic save protocol without changing v4 framing.
3. Port heap walking and exact slot swizzling using floor `objectPayloadSize`,
   `slotKind`, exported-name, and vtable APIs. Reject unsupported or ambiguous
   graph shapes before mutation.
4. Port Store fact journaling/replay, compiled-unit code records, mapped dylib
   activation, quiescence, and explicit source rebuild. Compatible frozen v4
   images remain readable; unsupported older formats remain rejected.
5. Run the full cross-product: Zig/Rust writer × Zig/Rust reader, Zig/Rust
   Store, Zig/Rust Codegen, mapped/rebuild paths, corrupt/truncated/duplicate
   authority/multiple-code fixtures, and relocated installed trees.
6. Use the export/layout and cross-product results as the pre-switch acceptance,
   then make one canonical Rust image selection commit. Observe project builds,
   live replacement, rollback, and release packaging after that commit.

**R6 exit gate:** all compatible-v4 cross-products pass with exact rejection
reports, no partial activation, pre-switch full verify/R6 passes, one canonical
switch commit, and a post-switch observation window with unchanged R6
allowances.

## 11. Phase R7 — Host and final closure

1. Port `main.zig` startup/REPL/project dispatch and runtime resource
   resolution only after R1–R6 are canonical and observable.
2. Preserve source/bootstrap semantics, configuration policy, installed
   relocation, project check/build/install, docs, diagnostics, and release
   packaging. Host process controls remain host policy; they do not become
   hidden floor behavior.
3. Replace the Zig artifact preflight helper with an internal Rust-host module
   that consumes the existing pointer-free inspector protocol and host-owned
   closure rules. There is no separate preflight dylib/tool slice and no
   second runtime preflight authority.
4. Run two independent clean Rust bootstraps with no Birnam artifact, full
   verify, installed relocation, R6, declared embedded bootstrap-source
   recovery only, images, isolates, and release rollback as the pre-switch
   acceptance. Runtime resolution must never fall back to a checkout, cwd,
   source tree, or manifest.
5. Make one focused Rust-host canonical-switch commit. Then run the
   post-switch release, installed/project, image, isolate, and rollback
   observation window under Design 62 §5.4.
6. Remove Zig host implementations only after that observation window and the
   Design 61 R8 closure. Retain frozen compatibility dylibs where required for
   old images and rollback.

## 12. Commit and review discipline

Every implementation slice lands as a small commit after its focused tests and
R6 pass. The commit records the selected identity/digest and docs checkpoint;
unrelated cleanup is not mixed in. Before each canonical-switch commit:

1. inspect the diff and generated install tree;
2. run cross-floor and forced-GC tests;
3. run `zig build verify -Doptimize=ReleaseSafe --summary all`;
4. run `mise run r6-acceptance` and record the exact comparator output;
5. run from a second working directory/prefix; and
6. preserve a rollback command selecting the Zig oracle.

If a test exposes an invariant conflict, stop the phase and write a design
decision. Do not reinterpret a failed parity test as permission to change the
floor, MOP law, Store authority, ABI, image format, wire version, or dynamic
library boundary.

## 13. Critical path and estimate

The critical path is **R0 → R0.5 Prelude → Collection → Relational → Alien →
Image → Host**; standalone Error and Isolate can proceed alongside Collection
once their contracts are frozen. A realistic remaining estimate is:

| Phase | Engineering effort |
| --- | ---: |
| R0 inventory/contracts | 1–2 weeks |
| R0.5 Prelude completion/switch | 1–2 weeks |
| R1 Collection | 3–5 weeks |
| R2 Standalone Error dylib | 1–2 weeks |
| R3 Isolate | 3–5 weeks |
| R4 Relational | 4–7 weeks |
| R5 Alien | 4–7 weeks |
| R6 Image orchestration | 4–7 weeks |
| R7 Host/final closure | 5–8 weeks plus observation |

This is **26–45 engineering weeks plus observation windows** for one developer,
with the widest variance in Relational, Alien, and Image. The estimate excludes
already-landed Rust floor/compiler work and includes migration-specific parity,
GC, install, and rollback evidence.
