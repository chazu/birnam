> The decisions below are consolidated in [ARCHITECTURE.md](ARCHITECTURE.md). [The first milestone](docs/first-milestone.md) and [seed invocation proposal](docs/seed-invocation.md) contain the next implementation contracts for review, not additional accepted invariants.

**Review notes on "Birnam Runtime Architecture" (decision record, 31 October 2026)**\
**Status:** Open discussion. Each item is resolved one at a time; resolutions are recorded inline.

Overall: the invariants are coherent and faithful to the Piumarta/Warth lineage, and the malleability contract ("replaceable, not guaranteed to survive") is the right call. The items below are places where decisions pull against each other, or where a deferred "implementation detail" constrains others enough that it should be settled early.

| # | Topic | Status |
|---|-------|--------|
| 1 | GC strategy is load-bearing, not a detail | **Conservative, initially non-moving; module boundary agreed; integration details open** |
| 2 | "No universal tagged values" vs. near-C performance | **Integer representation agreed: ordinary object with integer payload** |
| 3 | What replacing `send` means | **Piumarta vtable approach; exact send/bind exposure and invalidation open** |
| 4 | Deoptimization through generated C | **Behavior agreed; implementation mechanisms open** |
| 5 | Image save as a native build | **Native artifact and object-defined persistence agreed; composition and protocol details open** |
| 6 | How much of the compiler to write in Odin | **Interpreter bootstrap experiment and authored Compiler/Lang/PEG agreed** |
| 7 | Smaller items | Open |

---

## 1. GC strategy is load-bearing, not a detail

The record lists "GC/root scanning and code-memory lifetime" among unresolved details. In practice the root-finding strategy determines:

- **C codegen shape.** Precise roots force generated C to keep live oops in shadow frames visible to the collector; conservative roots let the C compiler allocate registers freely.
- **Fiber stacks.** Go-style copying/growable stacks are effectively ruled out once C code holds raw interior pointers into stack memory. This points toward virtual-memory-reserved stacks with guard pages and lazy commit.
- **Unboxing.** What an unboxed or tagged value means to the collector (see #2).
- **Heap layout.** Invariant 2 allows "arbitrary native data" in object payloads. A precise heap scan then requires every vtable to describe its payload layout; otherwise heap scanning must also be conservative.
- **Code lifetime.** Whether return addresses on native stacks keep compiled code objects alive.
- **Image save and compaction** (see #5).

**Original recommendation (precise-heap portion superseded by the decision below):** conservative scanning of roots (native stacks, registers, saved fiber contexts) plus a precise heap, using a non-moving mark-region collector (Immix-style) with stop-the-world collection at the safepoints already required for preemption. Compaction happens at image save/rebuild time rather than in the running process. Reference: Shahriyar, Blackburn & McKinley, *Fast Conservative Garbage Collection* (OOPSLA 2014), which shows conservative-root Immix competitive with precise collectors.

**Discussion log:**

- **A. Root finding — agreed: conservative.** Native stacks, registers, and saved fiber contexts are scanned conservatively. Generated C stays plain C (tcc-, FFI-, and primitive-friendly); return addresses keep code objects alive.
- **B. Heap scanning — prior art (idst, snapshot `idst-20070918`, `object/id/libid.c`, `object/st80/`):**
  - idst vendors **Boehm GC 6.7** and is *fully* conservative: stacks, registers, **and** every object body. Objects come from `_libid_palloc` → `GC_malloc` (whole body scanned); there are no per-vtable layout descriptors. `USE_GC` off falls back to `calloc` and never frees.
  - The only precision is an **allocation-kind split**: pointer-free storage (string bytes, word arrays, selector names) is a separate side buffer from `_libid_balloc` → `GC_malloc_atomic` (never scanned), referenced from a slot in the scanned object. E.g. `String` holds `_bytes` → atomic buffer.
  - **Weak references** use Boehm disappearing links: `WeakArray` keeps its slots in an atomic buffer and registers each slot with `GC_general_register_disappearing_link`, pointing at the allocation base (`&obj->_vtable[-1]`, because an oop points past its vtable header word — so interior-pointer recognition is required).
  - No finalizers in the core or st80 library. GC entry points (`gc_addRoots`, `gc_gcollect`, weak-link registration) are exposed as function pointers on `struct __libid`, filled via `dlsym` — "replaceable memory management" meant swappable allocation entry points, not a replaceable collector algorithm.
  - Incidentally relevant to #2: idst **tags SmallIntegers** (low bit 1; `(len << 1) | 1`, `v_anInteger >> 1`).

- **B. Heap scanning — agreed, 3 November 2026: conservative.** Object payloads are opaque to the generic runtime and collector. Heap scanning does not require per-vtable field layouts or pointer maps; the collector examines allocation contents for possible references without interpreting their language-level meaning. Explicitly pointer-free allocations remain an option for storage such as byte buffers.

- **C. Allocation and collector integration — agreed, 3 November 2026.** Allocation is non-moving for the initial implementation. An existing collector is acceptable provided it is wrapped in the `id` API and made available as a Birnam module. The particular collector library is not yet selected.
- **D. Foreign libraries — agreed, 3 November 2026.** The Alien module provides a message-passing API for wrapping dynamically loaded C libraries and bears responsibility for managing their state and resource lifetimes, together with library-specific wrapper objects. Foreign state remains opaque to generic `id` and GC. The existing Alien implementation and examples inform the behavior; their Rust machinery is not a requirement for the restart. Exact rooting, ownership, callback, blocking-call, and image-restoration protocols remain to be specified.

  **Existing behavior inspected (3 November 2026):**
  - `rust/birnam-alien/src/runtime.rs` manages library opens/closes, rejects unloading while live wrappers may call the library, distinguishes Alien-owned buffers from external addresses, and retains callback code until release. `rust/birnam-siphon/tests/alien.rs` exercises collection and recursive reentry during callbacks, including a callback releasing its registration while still executing. These describe lifetime requirements, not required registries or retention-token designs.
  - `examples/ffi.birnam` demonstrates buffer allocation/free and callback creation/release. `wrapped/termbox2/termbox2.birnam` and `examples/presentation-repl/terminal.birnam` expose and coordinate terminal initialization/shutdown. The adjacent `alien-twin/src/twin.birnam` wrapper exposes connection open/close and library-specific event release. Library-specific resource semantics belong in these wrappers rather than generic payload inspection.
  - `modules/alien-recipes.birnam` and `scripts/alien-image-test.py` describe/recover foreign method bindings while preserving authored replacements. This is evidence for reconstructing callable bindings, not automatic capture of arbitrary C-library internal state. Library state restoration needs its own explicit policy; the current recipe encoding and Rust image machinery are not prescribed.

- **E. Fiber/collector coordination — reference direction agreed, 3 November 2026.** Birnam's fibers are intended to follow the goroutine model, and Go's GC/scheduler coordination is a useful reference. Examine worker coordination, stack scanning, and foreign-call/callback transitions. Go performs concurrent marking and sweeping with stop-the-world phases; its precise scanning and write barriers are implementation choices to study, not adopted requirements. Birnam retains conservative scanning and opaque payloads. Full stop-the-world collection was proposed for simplicity but has not been agreed; the choice of full stop-the-world versus incremental/concurrent collection remains open. References: [Go GC](https://go.dev/src/runtime/mgc.go), [Cgo transitions](https://go.dev/src/runtime/cgocall.go).

**Resolution:** Conservative scanning of both roots and ordinary heap allocations, initially non-moving allocation, and a collector exposed through the `id` API as a Birnam module are agreed. Payload interpretation belongs to the objects' implementations. An existing collector may supply the implementation; library selection, collection coordination (using Go as a reference), bootstrap allocation handover, Alien lifetime integration, and code-memory lifetime mechanisms remain open. Image serialization and relocation must separately account for opaque payloads; conservative scanning alone does not identify which words may safely be rewritten as pointers.

**Research follow-up, 3 November 2026:** [Collector research](Collector%20Research.md) compares BDWGC, MPS, and MMTk, using Go and Crystal as coordination references. Recommendation: qualify BDWGC first, including normal/incremental modes and the actual fiber/Alien boundary. This is a research recommendation, not a collector selection or implementation result; relevant upstream issue reports and proposed validation are recorded there.

---

## 2. "No universal tagged values" vs. near-C performance

The original review asked whether invariant 2 excluded only a fixed tagged union or also tagged immediate `oop` encodings. The local idst source confirms that SmallInteger uses a low-bit tag: the value is encoded directly in the `oop` word, and dispatch selects a shared tagged-value vtable. Ordinary objects instead have addressable payloads and an associated vtable. Sources: `object/st80/SmallInteger.st` and `object/id/libid.c` in `/Users/chazu/dev/c/idst`.

**Resolution — agreed, 3 November 2026:** use the user's preferred representation. An Integer is an ordinary object with the integer value in its payload, reached through an actual `oop` pointer. Small integers do not use tagged immediate `oop` encodings. The Integer implementation interprets the payload; generic `id` and GC remain unaware of its integer meaning.

This is commonly called a **boxed integer**: the value resides in an object, and references point to that object. It adds no second wrapper object around the payload. Native integer temporaries remain permitted as an optimization when identity, mutation, live replacement, and reconstruction semantics are preserved. Storage/allocation costs must be measured; the representation alone does not establish attainable performance. Integer width, overflow behavior, and allocation reuse/interning remain separate decisions.

---

## 3. What replacing `send` means

**Alternative recorded for consideration, 4 November 2026:** plain Id-style mechanical send/bind with replaceable `lookup`, explicitly permitting lookup to return a method or a method closure carrying data. Binding returns that selected callable Method and sending enters it directly. See [dispatch equivalence](docs/dispatch-equivalence.md) for the ordinary-call simulation and the receiver-aware binding counterexample. The user asked to retain this suggestion, not to adopt it yet; independent send/bind replacement remains under discussion.

**Direction, 3 November 2026:** follow Piumarta's basic vtable/meta-vtable approach. The earlier suggestion of choosing a separate send-policy or closure-invocation architecture was premature.

**Source clarification:** the basic implementation installs `lookup` on `VtableVT`, the vtable of vtables. `send` is native glue and `bind` asks the receiver's vtable to perform `lookup`. The recursive path bottoms out through native table lookup when binding `lookup` on `VtableVT`; the selected method is then called directly. The paper does not install `send` and `bind` themselves as vtable methods. [Piumarta and Warth, section 3.2 and Appendix A](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf)

The local idst source likewise installs `lookup:` on `_vtable_vtable` (`object/id/libid.c:1102`) and implements `_sendv`/`_libid_bindv` as native glue, although its recursion guard differs from the basic paper's specific base case. These are distinct implementations, not evidence that `send`/`bind` already appear as methods in the basic model.

Birnam's existing requirement that `send` and `bind` themselves be replaceable remains in force. Use the same vtable/meta-vtable mechanism to work out that exposure; exact selectors, receivers, and the minimal native entry ABI remain to specify. A second implicit invocation lookup on the selected method is not part of the basic model and is not adopted by this discussion. Cache and optimized-code invalidation must honor replacement; dependency invalidation versus guards remains an implementation choice, not a settled decision.

---

## 4. Deoptimization through generated C

**Resolution — agreed, 3 November 2026:** at the next operation affected by an invalid assumption, execution must use valid behavior, deoptimizing if necessary, without replaying completed effects. Replacing a method does not otherwise require rewriting already-entered calls: those calls may finish their selected implementation, while later sends observe replacement.

An active optimized caller may have inlined another method or specialized a later operation. Permission to finish an already-entered method on its old implementation does not permit a later affected operation to bypass current behavior. An inlined call that has already semantically begun is distinct from one not yet reached.

Guards, dependency invalidation, generated-C bailout blocks, return checks, and logical-frame reconstruction remain implementation choices. The contract requires valid behavior at the affected operation; a later safepoint is insufficient if stale behavior would already have been observed. No universal rewriting of active native stacks is required.

---

## 5. Image save as a native build

**Direction — agreed, 3 November 2026:** Birnam can load Birnam modules, images, and source. An image is a native executable and/or dynamically loadable library with an entrypoint. Support a single executable/loadable artifact on platforms and toolchains that permit it; otherwise provide the appropriate native forms. Full images retain source and development machinery as already agreed.

**Object-defined persistence — agreed, 3 November 2026:** object implementations describe their persistent contents through messages, supplying data and explicit references. Image machinery preserves object identity within the reconstructed graph, aliases, and cycles, separating object creation from reference connection as needed. Reconstruction uses the corresponding object behavior, which the artifact retains or resolves. Generic `id` remains unaware of payload meaning. Alien wrappers supply library-specific reconstruction for foreign state; process-local handles are not restored as valid addresses.

The accepted example was a Pair whose two fields reference the same Integer: the writer records two references to one saved Integer identity, and Integer's implementation writes its numeric payload as bytes. Reconstruction creates the objects and restores both references to the same Integer. `saveOn:`, `writeReference:`, and byte-writing messages were illustrative; exact selectors, encoding, and reconstruction phases remain unspecified. Conservative GC alone does not identify relocatable pointers, and omitting active stacks does not make opaque payloads precisely relocatable.

**Loading direction — clarified, 4 November 2026:** install/overwrite the objects supplied by the loaded image contents. That scope can range from a simple module to substantial portions of the running system. Objects outside the replacement scope are not wholesale replaced. Selective loading of symbols or image parts is desired. This supersedes the assistant's proposal of independent restored graphs with private saved behavior by default. Target identity and representation compatibility were subsequently resolved at the contract level below; exact protocols remain open.

**Replacement and compatibility — agreed, 4 November 2026:** preserve existing target identities while installing selected contents. Initially match named targets by namespace path, creating absent targets; reconstruct anonymous objects unless explicitly mapped to existing ones. Install new Methods in existing vtables, preserving entered/captured Methods and invalidating affected optimized code. Restore state through object-defined messages. Resolve dependencies against the host or explicitly include them; reachability alone does not authorize replacement. Universal `become:`-style identity redirection is outside the initial design.

Each implementation exposes an opaque representation identifier. Matching identifiers permit ordinary loading through the state-restoration protocol. A mismatch requires an explicit migration; without one, report incompatibility before publishing the replacement. Migration must account for active old calls and Alien-held pointers by waiting for compatible conditions, preserving their access contract, or declining to proceed. Initially permit representation changes only through implemented and tested migrations. This catches undeclared representation changes, not false declarations or bugs in arbitrary native code. Exact messages, identifier ownership, target resolution, and coordination remain open; this does not promise transactional rollback of arbitrary initialization effects.

Saving may reuse relocatable compiled code or regenerate it from retained representations before linking the native artifact; recompiling every method on every save is not a requirement. Running the artifact and loading it into an existing Birnam world need defined entrypoint behavior, including which initialization effects run. These proposals specify no mandatory blob format, registry, or new object hierarchy.

**Prior-art research, 4 November 2026:** [Image loading prior art](Image%20Loading%20Prior%20Art.md) compares Self transport, Smalltalk snapshots/packages/parcels, object serialization, and optimized-code invalidation. It motivated the identity-preserving update and explicit migration contracts above. Neither selective loading nor an image's native container alone establishes near-native execution speed.

---

## 6. How much of the compiler to write in Odin

**Direction — agreed experiment, 4 November 2026:** use a small Odin interpreter to run the first compiler written in Birnam. Do not use the previous Rust implementation or its bootstrap tools. This supersedes writing the full initial compiler front end and lowering in Odin.

**Authored module structure — required:** the Birnam-authored compiler uses a Birnam-authored Lang module, and Lang uses a Birnam-authored PEG module. All three run through ordinary `id` objects/messages. The old implementation is a reference for the module relationship, not a source of required Rust machinery.

The seed interpreter has an initial parser and enough evaluation/library support to execute the authored PEG, Lang, and compiler sources, initially written within a subset of the existing language. It does not need native code generation, optimization, image persistence, or multicore fibers to perform that first execution. This breaks the circular dependency: the seed parses/evaluates the authored parser modules, then those modules supply the compiler's source-processing path. Once compiled, the compiler continues using the same authored Lang/PEG modules.

Bootstrap sequence: Odin seed interpreter runs authored PEG/Lang/compiler; the authored compiler emits C for itself and its modules; a system C compiler builds them with the native foundation; the resulting compiler rebuilds the authored sources. This requires no installed Birnam. Exact subset, module initialization, handover, and validation remain to design. Checking in generated C as an alternative seed remains optional and would require a regeneration path. No bootstrap implementation has yet been delivered.

---

## 7. Smaller items

- **Siphon "replacing" bootstrap `id` — agreed, 4 November 2026:** a layout-compatible takeover. The fuller module installs/replaces methods on existing core objects and vtables, preserving all already-created object identities and the bootstrap invocation ABI/payload representations. Subsequent sends observe replacement with affected caches invalidated; entered calls retain their selected implementation. Later representation changes require the agreed explicit migration protocol.
- **Odin interop — agreed with explicit boundary, 4 November 2026:** use a C-compatible boundary for generated code, native modules, and runtime entry. Confine Odin-specific ABI, context, allocator conventions, and features to Odin implementation code. Non-Odin callers need no Odin-specific knowledge. Birnam semantics are unaffected, and Id's receivers, selectors, arguments, and results remain Oops. Odin adapters establish their context internally, including on callbacks. Computation-specific state follows fibers, worker-specific facilities belong to workers, and any context or temporary storage referenced by suspended native frames remains valid until those frames finish. Language-visible execution state remains Birnam objects, not hidden Odin policy. Exact context lifetime and adapter mechanics remain open.
- **M:N and blocking FFI — target contract agreed, 4 November 2026:** Birnam-aware I/O parks fibers and releases workers for other work. Potentially blocking C calls may occupy OS threads; the runtime makes execution capacity available elsewhere so unrelated fibers can continue. Alien bindings describe blocking and thread-affinity requirements, preserving required OS-thread associations. Callbacks re-enter through scheduler/collector coordination before running Birnam. Ordinary calls need no explicit suspension syntax. The single-worker bootstrap may be simpler; worker handoff, capacity limits, and exact binding metadata remain open.
- **Baked images** get closed-world devirtualization almost for free — worth stating as a payoff.
