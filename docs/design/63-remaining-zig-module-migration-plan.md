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
