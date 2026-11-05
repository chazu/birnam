> Consolidated on 4 November 2026 into [ARCHITECTURE.md](ARCHITECTURE.md). This earlier record is retained as discussion history; use the consolidated document for current requirements and the milestone documents for implementation proposals. In particular, the 5 November seed decision selects receiver-aware lookup with mechanical send/bind, superseding this historical record's independent send/bind-hook requirement.

**Decision record — 31 October 2026**\
**Status:** Agreed architectural direction; not an implementation specification.

## Purpose

Birnam is an image-based, natively compiled language built on Ian Piumarta's `id` object model. Its defining qualities are **complete semantic malleability**, lightweight concurrency, self-contained source-bearing images, dynamically loadable artifacts, and a path to near-C performance through speculative optimization and deoptimization. The implementation will be restarted in **Odin**, preserving Birnam's existing syntax and concurrency semantics. Stacklessness is **no longer a requirement**.

## Non-negotiable invariants

1. **One object universe.** Every language-visible entity—including values, methods, code, fibers, schedulers, compiler structures, and runtime services—is an `id` object. There is no second, authoritative, hidden object model represented by a fixed Odin tagged union.

2. **Ordinary object pointers.** `id` operates on ordinary object pointers (`oop`), not a universal tagged-value representation. Object payloads may contain arbitrary native data and are opaque to the generic runtime and collector; their interpretation belongs to their implementations. The small bootstrap layout and invocation ABI are implementation mechanisms, not a fixed hierarchy of language-level types.

3. **Messages define semantics.** Language-visible operations are expressible through `id` message sending. This includes **`send` and `bind` themselves**, which are replaceable entries on the relevant object/vtable metaobjects; their behavior must not be permanently frozen into the compiler.

4. **Unrestricted malleability.** Objects implementing parsing, lookup, invocation, scheduling, compilation, memory management, and other language behavior must be inspectable and replaceable from within Birnam. A replacement is **not guaranteed to work**, preserve invariants, keep the process alive, or be recoverable. There is no protected semantic authority outside the object universe.

5. **Stackful concurrency.** Birnam computations may use ordinary native call stacks and are independently suspendable as lightweight fibers. Programmers do not need explicit `yield` or `await` at each suspension point.

6. **Native speed without semantic lock-in.** AOT compilation and speculative optimization may specialize, inline, and unbox operations only while respecting the applicable `id` semantics. Optimized code must have a fallback or invalidation strategy when assumptions change.

7. **Self-hosting destination.** Odin is the bootstrap vehicle, not the permanent implementation language for Birnam's semantics. The compiler and higher-level runtime should eventually be implemented and evolved within Birnam itself.

8. **Namespaces.** Birnam has namespaces as an existing language feature. The reboot preserves their current syntax and semantics; namespace-related entities and operations participate in the same `id` object universe, without a separate hardwired module system.

9. **Go-style M concurrency.** Birnam multiplexes many lightweight stackful fibers over OS worker threads. Fibers execute concurrently and can run in parallel on multiple cores. The runtime handles suspension, resumption, and preemption at compiler-defined safepoints transparently; supported blocking operations park individual fibers rather than whole workers wherever possible. Scheduling policy remains replaceable through `id`; Go's exact G-M-P algorithm and signal-based preemption are not requirements.

10. **Self-contained images.** A complete Birnam image preserves its live object graph, namespaces, definitions, modules, **source code**, and development machinery together. It must not depend on separate source or changes files. Active native stacks and ongoing computations are not part of the persistence guarantee.

11. **Loadable artifacts and optional baking.** Modules and images are dynamically composable native artifacts. A full image remains malleable; a deliberately baked application may omit the compiler, loader, source, and other development/malleability machinery for standalone distribution.

## Object and invocation model

- The minimal `id` seed provides ordinary `oop` values, vtables, interned selectors, allocation, method closures, and an invocation path. A vtable's `lookup` behavior is itself defined through message dispatch.

- `send` and `bind` are replaceable methods in the `id` metaobject graph (on object/vtable vtables as appropriate), **not immutable runtime policy**. A tiny native bootstrap/entry mechanism is necessarily required to break recursive invocation and reach machine code; it must be kept minimal and semantically neutral.

- **Dispatch reference (3 November 2026):** use Piumarta's basic vtable/meta-vtable approach as the starting point. In that implementation, `lookup` lives on the vtable of vtables; native `send`/`bind` glue reaches it and provides the recursion base case. Birnam's requirement to expose `send`/`bind` themselves as replaceable methods remains; their exact placement and entry ABI are still to specify. [Basic implementation, section 3.2 and Appendix A](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf)

- Methods and code are objects. A method closure can refer to compiled machine code, interpreted code, or a native primitive while presenting a common invocation protocol.

- Compiled send sites may use inline caches, guarded direct calls, and inlining. Their assumptions include relevant vtable, lookup, and delegation behavior; changing those assumptions requires cache invalidation or deoptimization. Custom lookup semantics must not be silently bypassed.

- Semantic values remain `oop` pointers. Native unboxing of integers or other values is permitted only when observable identity, mutation, and reconstruction semantics remain correct.

- **Integer representation (agreed, 3 November 2026):** an Integer is an ordinary object whose payload contains its integer value. Its `oop` is an actual object pointer; small integers are not encoded as tagged immediate `oop` words. Integer implementations interpret the payload; generic `id` and GC do not interpret it as an integer. Optimized execution may still use native integer temporaries under the semantic constraints above. Integer width, overflow behavior, and allocation reuse/interning are separate questions not settled by this representation choice.

- **Garbage collection (decisions, 3 November 2026):** roots (native stacks, registers, and saved fiber contexts) and ordinary heap allocations are scanned conservatively. The collector does not require per-vtable payload layouts or pointer maps. Explicitly pointer-free allocations may omit scanning. Allocation is non-moving for the initial implementation. An existing collector is acceptable provided it is wrapped in the `id` API and made available as a Birnam module. The particular library and its integration mechanisms remain undecided; image serialization and relocation require a separate mechanism for opaque payloads.

## Compilation, artifacts, and image lifecycle

- **Compiler pipeline (authored direction agreed, 4 November 2026):** the compiler is written in Birnam and uses a Birnam-authored `Lang` module, which in turn uses a Birnam-authored `PEG` module. PEG provides parsing machinery; Lang owns Birnam's language grammar and source-processing behavior; the compiler uses Lang and lowers into Birnam-owned intermediate representation, initially emitting C. These modules participate in the ordinary `id` object universe. Modified `libtcc` is an **optional, replaceable bootstrap backend** for rapid in-process compilation, not a permanent dependency. A system C compiler can build generated bootstrap code and optimized releases; a later Birnam-native backend may replace C emission entirely.

- **Siphon:** A minimal executable knows the shape of the `id` API and how to load/reload modules. It initially supplies bootstrap `id`, loads the fuller `id` module to replace that implementation, then loads the concurrent runtime, base library, and additional modules. The siphon's behavior remains represented by a replaceable `id` object after startup.

- **Bootstrap `id` handover (agreed, 4 November 2026):** the fuller `id` module takes over compatibly by installing or replacing methods on existing core objects and vtables. Already-created objects, selectors, vtables, and module objects retain their identities. The bootstrap invocation ABI and payload representations remain compatible during handover. Subsequent sends observe the new behavior, with affected caches invalidated; already-entered calls retain their selected implementation. Later representation changes use the explicit compatibility/migration protocol. Handover develops the existing object universe rather than reconstructing it.

- **Module:** An independently dynamically loadable artifact speaking the `id` message API. Loading installs its exported symbols into a designated namespace. **Modules and namespaces are distinct:** one module can contribute to multiple namespaces, and multiple modules can contribute to one namespace.

- **Alien:** A Birnam module providing a message-passing API for wrapping dynamically loaded C libraries, with responsibility for managing foreign-library state and resource lifetimes. Alien supplies general loading, binding, memory, and callback facilities; library-specific wrapper objects express initialization, shutdown, and resource operations through messages. Foreign state remains opaque to generic `id` and GC. The existing implementation and examples inform these behaviors without requiring their Rust machinery. Exact rooting, ownership, callback, blocking-call, and image-restoration protocols remain to be defined; restoring a callable binding does not itself restore the C library's internal state.

- **Loadable inputs (agreed, 3 November 2026):** Birnam can load Birnam modules, Birnam images, and Birnam source.

- **Complete image:** A self-contained native artifact with an entrypoint, carrying the live object graph, definitions, code/modules, namespaces, **source code**, and tools needed for further live development. Source may be filed into the running image and compiled. The modified image can be saved and reloaded without external source/changes files. It need not capture active fiber stacks. The artifact is an executable and/or dynamically loadable library. Where the platform and toolchain support one artifact serving both roles, Birnam should support that form; elsewhere the same logical image has platform-specific executable/loadable encodings. Exact entrypoint behavior remains to specify.

- **Loading into an existing world (agreed direction, 4 November 2026):** loading installs or overwrites selected contents in the current world, preserving the identity of existing objects being updated. Initially, named targets are matched through namespace paths; absent targets are created. Anonymous objects are reconstructed unless an explicit mapping identifies an existing target. Behavior changes install new Method objects into existing vtables and invalidate affected optimized code; already-entered calls and explicitly captured Methods retain their selected implementation. State restoration uses object-defined messages. The replacement scope may range from a simple module to substantial portions of the running system. Selective symbol/module/namespace loading is supported in principle; dependencies are resolved against the host or explicitly included, and mere reference to a dependency does not authorize overwriting it. Exact selection and resolution protocols remain open. Universal `become:`-style identity redirection is outside the initial design.

- **Representation compatibility (agreed, 4 November 2026):** each object implementation exposes an opaque representation identifier denoting agreement on payload storage and interpretation. This is not a generic type catalog or necessarily a version number, and `id` does not interpret the payload. Ordinary loading requires matching representation identifiers and restores state through the object's protocol. A mismatch requires an explicit migration supplied by the implementation; without one, the loader reports incompatibility before publishing the replacement. Migration must account for already-entered calls and Alien-held pointers: it must wait for compatible conditions, preserve their access contract, or decline to proceed. The initial implementation supports representation changes only through implemented and tested migrations. Identifiers enforce declared compatibility, not correctness of arbitrary native code or migrations; unrestricted replacement retains its existing failure risks. Exact identifier ownership, migration messages, and coordination mechanisms remain to specify.

- **Object-defined persistence (agreed, 3 November 2026):** objects describe their persistent state through messages. Their implementations interpret payload contents and supply data and explicit object references; generic `id` does not interpret those contents. Image machinery preserves graph identity, shared references, and cycles, separating object creation from reference connection as needed. Reconstruction uses the corresponding object behavior, which the artifact must retain or resolve. Alien wrappers describe library-specific resource reconstruction rather than persisting process-local handles as valid addresses. The illustrative `saveOn:`, `writeReference:`, and byte-writing messages are not fixed API names; the encoding and exact reconstruction protocol remain open.

- **Baked image:** A standalone distribution executable formed by selecting an application and retaining only its required code and state. It intentionally removes source, compiler, siphon, and unnecessary runtime reflection/malleability machinery. A baked artifact is **exempt from the full-image malleability requirement**; developers may alternatively ship an unbaked, fully malleable image.

- **Module reload and compaction:** New compiled versions load beside old ones, and future bindings resolve to the published versions; existing native activations may finish on old code. The initial simple way to discard obsolete code is to **compact and rebuild/reload the image**, omitting unreferenced versions. Immediate in-process unloading and reclamation are not required.

## Execution and concurrency

- A `Fiber` is an `id` object with a native execution stack, saved CPU context, and scheduling/wait state. `Scheduler`, `Mailbox`, and synchronization behaviors are also ordinary, replaceable `id` objects.

- Native primitives perform context switching, stack allocation/protection, OS-thread management, and operating-system I/O. Their interfaces and policy-bearing implementations are exposed through `id`; replacing them may crash the image.

- **Native interoperability boundary (agreed, 4 November 2026):** generated code, native modules, and the runtime use a C-compatible boundary. Odin's ABI, implicit context, allocator conventions, and other Odin-specific knowledge remain confined to Odin implementation code; they do not become Birnam language semantics or requirements on non-Odin callers. Id's message receivers, selectors, arguments, and results remain Oops. Native implementation adapters establish the necessary Odin context internally, including on callback entry. Computation-specific state follows the fiber; worker-specific machinery belongs to the current worker. Contexts and temporary storage referenced by suspended native frames remain valid until those frames finish. Language-visible execution state remains represented by Birnam objects, with no second semantic authority in Odin context. Exact adapter and context-lifetime mechanisms remain implementation details.

- **Alien blocking calls (target contract agreed, 4 November 2026):** Birnam-aware I/O parks the calling fiber so its worker can run other fibers. Potentially blocking C calls may occupy an OS thread, while the runtime makes execution capacity available elsewhere so unrelated runnable fibers can continue. Alien bindings describe blocking and thread-affinity requirements; operations requiring a particular OS thread retain that association. Callbacks re-enter through the runtime and coordinate with scheduling and collection before executing Birnam code. Callers use ordinary messages without explicit suspension syntax. The initial single-worker implementation may be simpler; worker handoff, capacity limits, and exact binding metadata remain implementation choices.

- Initial scheduling: **one OS worker, stackful fibers, cooperative suspension**, and compiler-inserted checks at relevant loop backedges/call sites. Blocking channel/mailbox/I/O operations park fibers without requiring user-authored `yield` statements.

- The target runtime **must implement M scheduling** across multiple OS workers. Work stealing is optional; the particular scheduling algorithm remains replaceable `id` policy. Shared code and dispatch mutation require coherent pointer publication and cache invalidation, without promising that arbitrary hot replacements remain functional.

- **GC/scheduler reference (3 November 2026):** Go's goroutines and their coordination with garbage collection are a design reference for Birnam's fibers: worker coordination, stack scanning, and foreign-call/callback transitions. Study Go's concurrent collection with stop-the-world phases when evaluating collectors. Birnam's conservative scanning, opaque payloads, and initially non-moving allocation remain agreed constraints; Go's precise scanning and write-barrier machinery are not thereby adopted. Full stop-the-world versus incremental/concurrent collection remains open; the earlier full stop-the-world suggestion was a proposal, not a decision. References: [Go GC](https://go.dev/src/runtime/mgc.go) and [Cgo transitions](https://go.dev/src/runtime/cgocall.go).

- Preserve existing Birnam concurrency primitives and observable semantics. These runtime choices do not authorize a language-syntax redesign.

## AOT compilation, deoptimization, and live code

- **Native compilation is the primary performance path.** The compiler supports both ahead-of-execution artifact builds and incremental compilation within the running image. It aims to emit ordinary native calls and stacks, specialized sends, unboxed computations where valid, and eventually safe-point metadata describing reconstructible logical execution state.

- **Deoptimization is targeted.** It supports debugging/introspection and recovery from invalid speculative assumptions. It is *not* a universal promise that every in-flight activation can be rewritten after arbitrary changes.

- **Invalidated assumptions (agreed, 3 November 2026):** at the next operation affected by an invalid assumption, execution must use valid behavior, deoptimizing if necessary, without replaying completed effects. This includes later sends represented by inlined or specialized code within an already-running method. An already-entered call may finish its selected implementation; method replacement does not otherwise require rewriting active calls. Guards, invalidation, and reconstruction of logical execution state from native frames remain implementation choices.

- Code definitions have stable logical identities and versioned implementations. Publishing a replacement changes future bindings/calls according to the dispatch contract; existing activations may finish on an older code version. Old native code must remain available while return addresses still reference it in normal operation.

- Newly defined or edited code can execute in an interpreted/general form or be rapidly compiled in-process through a temporary C backend. Optimizing C backends and a future native code generator are separate choices; `libtcc` alone does not promise optimized-C performance or precise deoptimization metadata. Deoptimization does not itself generate replacement code.

- The live **image** is authoritative for objects, definitions, source code, and runtime policy. Compiled binaries are realizations of image state, not the source of language meaning.

- Image persistence includes the live object graph and source, but does **not** promise serialization or migration of active native stacks or computations.

## Malleability contract

**Guaranteed capability:** a programmer can reach, inspect, and replace the object or method representing any language-level implementation component, including the scheduler, compiler, `lookup`, `send`, and `bind`.

**Not guaranteed:** continued execution after a replacement; automatic repair or rollback; immediate transplantation of new code into active frames; universal snapshotting of live stacks; or validity of user-supplied machine code and replacement protocols.

**Ordinary-runtime correctness remains a goal:** supported method publication and dispatch should behave coherently in the absence of deliberately incompatible modifications. Allowing dangerous edits does not justify accidental torn pointers, stale caches that never invalidate, or unintended hidden policies.

## Bootstrap trajectory

1. **Odin siphon:** implement minimal `id` bootstrap dispatch/invocation, module loading, allocation, and native execution primitives.

2. **Layered startup:** load a fuller `id` module through the agreed layout-compatible handover on existing objects and vtables; then load the concurrent runtime, standard library, and other modules into the image. Keep the siphon accessible as a replaceable `id` object.

3. **Interpreter bootstrap experiment (agreed, 4 November 2026):** implement a small Odin seed interpreter for enough of Birnam's existing syntax and semantics to execute the Birnam-authored PEG, Lang, and compiler modules. Its initial parser breaks the bootstrap cycle; it is not the permanent language authority. The authored modules initially stay within the supported subset. This route does not use the previous Rust implementation, compiler, or Lang seed. The seed evaluator uses the same `id` object model and need not initially implement optimization, image saving, or multicore fibers. The exact subset and first authored-module handover remain to design.

4. **First native compiler:** execute authored PEG, Lang, and compiler through the seed interpreter to generate C for those modules and the compiler. Build the result with a system C compiler and the native foundation, then use that compiler to rebuild the authored sources. The native compiler continues using the authored Lang/PEG modules. A clean checkout must build without an installed Birnam; whether to retain generated C as an additional bootstrap seed remains open, with regeneration required if that route is adopted.

5. **Self-hosting image:** compile Birnam with Birnam, move remaining higher-level runtime policies into Birnam objects, and develop from the running, source-bearing image. The irreducible machine/OS boundary remains, but no language-level policy is intentionally privileged or frozen. The interpreter bootstrap is an agreed experiment, not a completed implementation.

## Deliberately unresolved implementation details

Exact `send`/`bind` selector placement and bootstrap invocation ABI; native stack strategy and context-switch ABI; collector library selection, collection coordination, bootstrap allocation handover, and code-memory lifetime mechanisms; Alien rooting, ownership, callback, and blocking-call protocols; image serialization/linkage format and opaque-payload relocation; safepoint/deoptimization metadata for C-generated code; eventual native compiler backend; multicore synchronization policy; portable module ABI; and optional live in-process unloading. These choices must preserve the invariants above, but are **not yet settled**.

## Guiding principle

> **Birnam is a self-describing, self-modifying `id` object universe.** Native stacks and AOT code accelerate its execution; they do not define immutable language semantics. The system must make its own implementation replaceable, but it need not protect programmers from the consequences of replacing it.

### Technical lineage

- Ian Piumarta and Alessandro Warth, [*Open, Extensible Object Models*](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf) (2008): the minimal `id` model, extensible lookup, closures, and mixed-mode execution.

- Urs Hölzle, Craig Chambers, and David Ungar, [*Debugging Optimized Code with Dynamic Deoptimization*](https://bibliography.selflanguage.org/_static/dynamic-deoptimization.pdf) (1992): reconstruction of logical execution state from optimized code.
