# Remaining Zig-module migration plan

Status: execution plan for the `rust-libid-floor` branch. This document starts
after the Rust IdApi floor and the opt-in Rust PEG, Lang, TCC, Codegen,
artifact, Store, and Prelude slices have landed. It is deliberately narrower
than Design 62: it describes the Zig modules that still own production
behavior and the order in which their Rust replacements can become canonical.

The goal is not a mechanical translation. Each replacement is a separately
loadable dylib, preserves the frozen IdApi and generated-C ABIs, keeps the
object dictionary and Store authoritative, and remains selectable beside its
Zig oracle until an observation window closes.

## 1. Current boundary and remaining owners

The current repository already contains Rust implementations for:

- the `libid` floor and public IdApi;
- Store/MOP and its Codegen observer/authorizer seam;
- PEG, Lang, TCC, Codegen runtime, artifact inspection, and the image codec;
- the opt-in N7.1 Prelude protocol through arithmetic, Boolean, Error, Block,
  System I/O/environment/output, GC controls, `loadModule:`, and `readLine`.

The remaining Zig-owned production modules are:

| Zig owner | Rust work still required | First safe cutover |
| --- | --- | --- |
| `src/id/collection.zig` | persistent Vector/RRB, Range, Map/HAMT, String/native collection protocol, layouts and tracing | after collection cross-floor and GC gates |
| `src/id/error.zig` | reconcile the Rust Prelude Error core with the standalone Error rung; remove duplicate ownership without changing the self-hosted protocol | after Error identity/DNU and source bootstrap gates |
| `src/id/isolate.zig` | process launch, worker loop, pipes, wire-v1 codec, bounded copied values, termination | after wire corpus and hostile-child gates |
| `src/id/relational.zig` | query engine, rule evaluation, pull, aggregation, time windows, Store integration | after collection and query-performance gates |
| `src/id/alien.zig` | dynamic-library ownership, C layout/signature handling, TCC shims, callbacks, buffers and finalizers | after ABI corpus and callback lifetime gates |
| `src/id/image.zig` | host orchestration around the Rust codec: mapping, swizzling, code records, Store replay, rebuild and save | after compatible-v4 cross-product gates |
| `src/id/main.zig`, project/build helpers | Rust host, source/bootstrap workflow, install and distribution closure | last, after every lower rung is observable |

`src/id/artifact.zig`, `artifact_inspector.zig`, and `artifact_preflight.zig`
must be treated as one artifact boundary. The Rust artifact crate is the
replacement candidate, but host preflight and activation policy remain Zig
until the N6/N10 closure gates prove that moving them does not broaden loader
authority.

The Rust Prelude Error family is already the only Error-family owner in a
process that loads it. The migration must not create a second `ErrorVt`,
factory, or Object DNU owner. The standalone Zig Error dylib is therefore a
compatibility oracle and consolidation target, not an independent new family.

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
5. **MOP consistency law.** Every method mutation updates the exact dictionary
   and the `#primitive` projection atomically: an exact method answers its
   callable primitive, an explicit primitive answers `#primitive`, and an
   absent method answers nil. No observer or authorizer callback may publish a
   contradictory state.
6. **Store authority.** Store facts are the sole source of relational state;
   object dispatch and Store reads are two views of one state. Rust code must
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
   testable through a clean-process selection. A Rust implementation is not
   canonical until focused behavior, forced-GC, cold bootstrap, relocation,
   full verify, R6 performance, and an observation window all pass.

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
6. Run `zig build verify` and `mise run r6-acceptance`; record the current R6
   aggregate and the two explicitly accepted benchmark crossings. Do not
   widen the R6 exception for inventory noise.

**R0 exit gate:** inventory is complete; each selector has a frozen behavior
fixture; Rust and Zig choices are explicit; no default switch has occurred; the
worktree is clean and both gates pass.

## 4. Phase R1 — Collection (first implementation slice)

Collection is first because Relational and Alien use its Vector/Map protocol,
and because its id-native payloads exercise layouts, persistent sharing, and
GC without changing the floor.

### R1.1 Rust crate and payload model

1. Add `rust/birnam-collection` as a `cdylib` over the frozen `birnam-abi`.
   Install it beside `libbirnam_collection.dylib`; give it a unique identity
   and a versioned diagnostic export.
2. Port the smallest owned family first: `VectorVt`/`Vector` and its 32-way
   persistent trie. Keep node payloads as exact OOP slots and install the same
   layout descriptors before allocation.
3. Reuse canonical `Integer`, `String`, `Symbol`, `Block`, `Error`, and System
   objects. Never allocate a second Boolean, Error, or Object family.
4. Port `size`, `at:`, `add:`, `do:`, `inject:into:`, `printString`,
   `indexOf:`, `copyFrom:to:`, and `reversed` with exact count/type/error
   behavior. Leave derived higher-order selectors in Birnam unless the Zig
   oracle proves they are native and the benchmark says they must remain so.
5. Add Rust-only unit tests for trie path-copying, tail growth, structural
   sharing, overflow/empty cases, and deterministic rendering. No unit test
   may call a foreign `send` while holding a Rust borrow or lock.

### R1.2 Range, Map, and String slices

1. Add `Range` with inclusive bounds, empty/descending behavior, `size`, `at:`,
   `do:`, `inject:into:`, `includes:`, `reversed`, and exact Integer values.
2. Add `Map`/`MapVt` as a persistent HAMT with value equality matching the Zig
   contract: SmallInt by value, String by bytes, Symbols/other objects by
   identity. Node payloads must be layout-described OOP slots.
3. Port only the native String methods still owned by the Zig collection rung
   (` ,`, `=`, `size`, `at:`, `byteAt:`, `copyFrom:to:`, case conversion,
   replacement, symbol/number conversion, and printString). Keep the derived
   collection protocol in `stdlib.birnam`.
4. Register root providers for any temporary trie/map node held across a block
   callback. Add forced-GC tests during `do:`/`inject:into:` and after a failed
   callback; prove no shared node is reclaimed early.

### R1.3 Cross-floor and acceptance gates

1. Add one Zig test corpus that loads exactly one selected collection dylib,
   checks exports and selector membership, and runs the behavior matrix.
2. Run the same corpus with the Rust floor and the Zig floor; compare values,
   Error identities, print strings, GC counts/bounds, and ordering.
3. Run the collection GC-layout, Store MOP, Rust Codegen, image heap, and
   installed relocation tests against the selected Rust collection.
4. Run the R6 focused target and full optimized verify. A benchmark change is
   accepted only as a new explicit comparator crossing with a checked-in
   result digest and tradeoff note.
5. Keep Rust opt-in for an observation window covering stdlib bootstrap,
   relational smoke, Alien struct conversion, image save/load, and project
   builds. Only then make one canonical-selection commit.

**R1 exit gate:** Vector, Range, Map, and retained native String behavior pass
the cross-floor corpus, forced-GC and relocation checks, full verify, R6, and
the observation window. Zig remains available for rollback.

## 5. Phase R2 — Error-family consolidation

This phase is a de-duplication and compatibility phase, not a new Error
implementation.

1. Prove that the Rust Prelude Error family and the standalone Zig Error family
   have identical vtable/payload layout, exports, factory selectors, DNU
   propagation, and Object `isError`/DNU behavior.
2. Move any remaining native-only Error core selector to the Rust owner or
   leave it in self-hosted `stdlib.birnam`; do not install it on a duplicate
   family.
3. Run nested propagation, handler, DNU restoration, numeric-fault, GC, image
   export, and reload tests with both preload orders.
4. Delete only the Zig Error implementation after a compatible-image and
   source-bootstrap observation window proves that no installed artifact asks
   for its identity.

**R2 exit gate:** one Error family, one factory, stable kind/message identity,
and no duplicate `ErrorVt`/`Error` export under any supported load order.

## 6. Phase R3 — Isolate

1. Add `rust/birnam-isolate` with a pointer-free codec and the exact `BISO`
   wire-v1 frame/schema already implemented by `src/id/isolate.zig`.
2. Port frame validation first (magic/version/flags/request IDs, maximum
   frame/depth/container/node limits, duplicate map keys, UTF-8 and
   transferable-value rules) and compare every error enum to the Zig corpus.
3. Port copied-value encode/decode with bounded allocation and cycle/depth
   guards. Never send a borrowed Rust reference to a worker or callback.
4. Port worker boot, request/reply, timeout/failure, stop/stopped, and owned
   termination. Keep orchestration and policy in `isolate.birnam`/the host.
5. Add child-process tests for malformed frames, hostile children, truncated
   replies, timeout, double-stop, and forced termination. Run debug and
   ReleaseSafe variants.

**R3 exit gate:** wire-v1 corpus parity, no unbounded allocation or panic across
the boundary, deterministic failure/termination, relocated install pass, and
R6/verify green.

## 7. Phase R4 — Relational

1. Port the query engine over the Rust Store callback surface only after R1.
   The Rust implementation must read live triples from Store and never cache a
   second authoritative fact set.
2. Port clauses left-to-right: constants/logic variables, joins, predicates,
   negation, disjunction, recursive rules, deterministic ordering, then pull
   and time-window reads. Preserve the existing no-reorder semantics.
3. Port aggregation (`count`, `sum`, `min`, `max`, `avg`, `groupBy`) with exact
   empty/type/error behavior and deterministic Map/Vector results.
4. Add cross-floor query fixtures for single/join/negation/rule/pull/asOf and
   the full Store MOP consistency law. Add GC during collector callbacks and
   callback reentry tests.
5. Run the query benchmark corpus. Any regression must be classified against
   the existing R6 allowances; do not hide it behind a broader threshold.

**R4 exit gate:** relational results and Store authority match, query and
collection performance are accepted, full image/project workflows remain green,
and the observation window closes.

## 8. Phase R5 — Alien

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

**R5 exit gate:** ABI corpus parity, callback ownership/lifetime proofs,
deterministic generated C, no unwind across C, and full R6/verify/install pass.

## 9. Phase R6 — Image orchestration

The Rust image crate already owns codec primitives; this phase ports the Zig
orchestration around them.

1. Port the image module's authority checks, one-generation binding, record
   ordering, and atomic save protocol without changing v4 framing.
2. Port heap walking and exact slot swizzling using floor `objectPayloadSize`,
   `slotKind`, exported-name, and vtable APIs. Reject unsupported or ambiguous
   graph shapes before mutation.
3. Port Store fact journaling/replay, compiled-unit code records, mapped dylib
   activation, quiescence, and explicit source rebuild. Compatible frozen v4
   images remain readable; unsupported older formats remain rejected.
4. Run the full cross-product: Zig/Rust writer × Zig/Rust reader, Zig/Rust
   Store, Zig/Rust Codegen, mapped/rebuild paths, corrupt/truncated/duplicate
   authority/multiple-code fixtures, and relocated installed trees.
5. Keep image save/load behind the Rust selection until the observation window
   includes project builds, live replacement, rollback, and release packaging.

**R6 exit gate:** all compatible-v4 cross-products pass with exact rejection
reports, no partial activation, and unchanged R6 allowances.

## 10. Phase R7 — Host and final closure

1. Port `main.zig` startup/REPL/project dispatch and runtime resource
   resolution only after R1–R6 are canonical and observable.
2. Preserve source/bootstrap semantics, configuration policy, installed
   relocation, project check/build/install, docs, diagnostics, and release
   packaging. Host process controls remain host policy; they do not become
   hidden floor behavior.
3. Replace Zig artifact preflight/host helpers only in separate dylib/tool
   slices with explicit identity and digest gates.
4. Run two independent clean Rust bootstraps with no Birnam artifact, full
   verify, installed relocation, R6, source fallback, images, isolates, and
   release rollback.
5. Remove Zig implementations only after the Rust observation window and the
   Design 61 R8 closure. Retain frozen compatibility dylibs where required for
   old images and rollback.

## 11. Commit and review discipline

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

## 12. Critical path and estimate

The critical path is **R0 → Collection → Relational → Alien → Image → Host**;
Error consolidation and Isolate can proceed alongside Collection once their
contracts are frozen. A realistic remaining estimate is:

| Phase | Engineering effort |
| --- | ---: |
| R0 inventory/contracts | 1–2 weeks |
| R1 Collection | 3–5 weeks |
| R2 Error consolidation | 1–2 weeks |
| R3 Isolate | 3–5 weeks |
| R4 Relational | 4–7 weeks |
| R5 Alien | 4–7 weeks |
| R6 Image orchestration | 4–7 weeks |
| R7 Host/final closure | 5–8 weeks plus observation |

This is **25–43 engineering weeks plus observation windows** for one developer,
with the widest variance in Relational, Alien, and Image. The estimate excludes
already-landed Rust floor/compiler work and includes migration-specific parity,
GC, install, and rollback evidence.
