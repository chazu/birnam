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
- A fresh `mise run r6-acceptance` passed at `44/44`; optimized verify still
  matches only the recorded `sortedModules` fixture exception, and the
  comparator still reports exactly the accepted `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%` crossings.

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
