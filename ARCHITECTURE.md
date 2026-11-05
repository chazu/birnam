# Birnam architecture

Consolidated 4 November 2026 from the agreed design discussion. This document specifies the end product and its architectural boundaries. Milestone proposals and research do not silently add requirements. There is no implemented reboot runtime yet.

## Purpose

Birnam is an image-based, natively compiled, prototype-based language built on Piumarta's `id` model. The running image is authoritative for objects, definitions, source, and runtime policy. Native artifacts realize that state. Birnam preserves its existing language syntax, namespaces, and observable concurrency semantics while replacing the implementation with an Odin bootstrap and increasingly Birnam-authored components.

Stacklessness is no longer required. The target combines live semantic replacement, lightweight stackful fibers, source-bearing images, loadable native artifacts, and optimized execution approaching native performance where the workload permits. Performance remains an empirical goal, not a delivered property.

## One object universe

Every language-visible value, method, code object, fiber, compiler structure, namespace, and runtime service is an `id` object. Receivers, selectors, message arguments, and results are Oops. There is no second authoritative value system implemented as an Odin tagged union.

An Oop is an ordinary object pointer. Payloads may contain arbitrary native data, and only their implementations interpret that data. Generic dispatch and collection do not classify payloads through a fixed catalog of language types. Allocation boundaries and bootstrap calling metadata are implementation mechanisms, not payload meaning.

Integers are ordinary objects with integer contents in their payloads. There are no tagged immediate SmallIntegers. Optimized execution may use native temporaries when identity, mutation, replacement, and reconstruction semantics remain correct. Integer width, overflow, and interning are separate questions.

Objects interact through messages. Implementations may use native algorithms for their own work, including bulk buffer operations. Native helpers do not authorize bypassing another object's replaceable behavior. Oop-only interaction does not require decomposing whole buffers into per-byte messages.

## Dispatch and replacement

Use Piumarta's vtable/meta-vtable approach. In the basic reference implementation, `lookup` is installed on the vtable of vtables and native sending/binding glue provides a recursion base case. Birnam additionally requires `send` and `bind` themselves to be exposed as replaceable behavior in the metaobject graph. Their exact placement and the minimal native entry ABI remain to specify.

Lookup selects a Method and execution enters that selected Method directly. There is no implicit second lookup of `apply` or `invoke` on it. An explicit Method or Block invocation remains an explicit message. Returning an object does not implicitly execute it.

Subsequent sends observe supported publication of new behavior. Already-entered calls and explicitly captured Methods retain their selected implementation. Methods and executable code remain valid while running or suspended computations need them. Method creation is distinct from dictionary installation.

Optimized sends may use caches, direct calls, inlining, and unboxing. At the next operation affected by an invalid assumption, execution must use valid behavior, deoptimizing if needed, without replaying completed effects. An entered method's right to finish its implementation does not allow future sends inside it to bypass replacements. Guards, dependency invalidation, and frame reconstruction are mechanisms to choose and validate.

All language-level implementation components are inspectable and replaceable, including parsing, compilation, scheduling, collection, lookup, sending, and binding. Arbitrary replacements need not work, be recoverable, or keep the process alive. Ordinary supported operations must still preserve identity, aliases, effect order, errors, and coherent publication; unrestricted malleability does not excuse implementation bugs.

## Memory and native boundaries

Scan native stacks, registers, saved fiber contexts, and ordinary heap allocations conservatively. Allocation is initially non-moving. Explicitly pointer-free storage may use unscanned allocations. No per-vtable pointer map is required by the collector.

An existing collector is acceptable when wrapped through the `id` API and available as a Birnam module. BDWGC is the first candidate to evaluate, not a qualified dependency. Collection scheduling, stack integration, code lifetime, and bootstrap allocation compatibility need evidence. Full stop-the-world versus incremental/concurrent collection remains open. Go informs scheduler/collector coordination; Crystal provides a conservative-collector/fiber integration reference.

Use a C-compatible native boundary. Odin-specific calling conventions, implicit context, allocator conventions, and features remain inside Odin code. Non-Odin callers need no Odin-specific knowledge, and Birnam gains no Odin-specific language semantics. Adapters establish Odin context internally. Fiber-owned state follows the computation, worker-owned machinery belongs to its worker, and contexts or temporary storage referenced by suspended native frames remain valid until those frames finish.

Alien provides message-based wrapping of dynamically loaded C libraries. Alien and library-specific wrapper objects manage foreign state, handles, buffers, callbacks, and resource lifetimes. The collector alone does not discover every reference retained by C. Exact rooting and callback protocols remain to specify. Existing Rust mechanisms are not requirements.

## Execution

Fibers have native stacks, saved execution state, and scheduling/wait state represented through objects. Scheduling and synchronization policies remain replaceable. Native primitives provide context switching, stack protection, OS threads, and OS I/O.

The initial concurrency implementation may have one worker and cooperative suspension, with compiler-inserted checks. The target multiplexes many fibers over multiple workers and permits parallel execution across cores. Work stealing and Go's exact scheduling or signal mechanisms are not requirements. Programmers do not need explicit `yield` or `await` at each suspension point.

Birnam-aware I/O parks a fiber so its worker can do other work. Potentially blocking C calls may occupy OS threads while the runtime provides capacity elsewhere for unrelated runnable fibers. Alien bindings describe blocking and thread-affinity requirements; required OS-thread associations are preserved. Callbacks coordinate with scheduling and collection before re-entering Birnam. Capacity limits and handoff mechanisms remain to design.

## Modules and images

Birnam loads modules, images, and source. A module is an independently loadable native artifact speaking the `id` API and publishing exported symbols into namespaces. Modules and namespaces are distinct: either can span several of the other.

A complete image is a self-contained native executable and/or dynamically loadable library with an entrypoint. It includes the live object graph, definitions, namespaces, code/modules, source, and development machinery. Where the toolchain/platform supports one artifact being both executable and loadable, support that form. Otherwise use appropriate executable/loadable encodings of the same logical image. Full images need no separate source or changes files. Active native stacks and ongoing computations are not part of the persistence guarantee.

Objects describe persistent contents through messages. Their implementations supply data and explicit references; image machinery preserves graph identity, sharing, and cycles, separating creation from reference connection as needed. Reconstruction behavior must be retained or resolved. Alien wrappers describe how foreign resources are reconstructed. Raw process-local handles are not restored as valid addresses. Conservative collection does not itself provide precise relocation information. The exact persistence selectors and encoding remain open.

Loading into an existing world installs selected contents and preserves the identities of existing target objects. Initially match named targets by namespace path and create absent targets. Reconstruct anonymous objects unless explicitly mapped to existing ones. Publish new Methods into existing vtables and restore state through object-defined messages. The scope may be a small module or much of the running system. Selective symbol/module/namespace loading is supported in principle; dependencies are resolved against the host or explicitly included. Referencing a dependency alone does not authorize replacing it. Universal `become:`-style identity redirection is outside the initial design.

Each implementation exposes an opaque representation identifier declaring compatible payload storage and interpretation. Ordinary loading requires a matching identifier. A mismatch requires an explicit implemented and tested migration; absent one, report incompatibility before publishing the replacement. Migration must preserve active callers' and Alien users' access contracts, wait for compatible conditions, or decline. Identifiers catch undeclared changes, not false declarations or incorrect native code. Exact coordination is unresolved, and arbitrary initialization effects have no promised transactional rollback.

Saving may reuse relocatable code or regenerate it before linking. New code versions may coexist with old ones. Compacting and rebuilding an image is the initial permitted route for discarding obsolete code; immediate in-process unloading is not required.

A baked application retains the code/state required for its application and may omit source, compiler, loader, and development or malleability machinery. It is exempt from full-image malleability. Developers may instead distribute a fully malleable image. Baking optimizations depend on which assumptions the artifact actually closes; they are not automatically free.

## Authored compiler and bootstrap

The compiler is Birnam-authored and uses Birnam-authored Lang, which uses Birnam-authored PEG. PEG owns parsing machinery; Lang owns the language grammar and source-processing behavior. The compiler lowers to Birnam-owned intermediate representation and initially emits C. `libtcc` is an optional replaceable backend; system C compilers can build bootstrap artifacts and optimized releases. A later native backend may replace C emission.

The accepted bootstrap experiment uses a small Odin interpreter for enough existing Birnam syntax and semantics to run those authored modules. Its initial parser breaks the cycle without becoming the permanent language authority. No previous Rust implementation, compiler, or Lang seed is used. A clean checkout must build without an installed Birnam.

The development sequence is seed interpreter → authored PEG → authored Lang → authored compiler producing C → native compiler rebuilding the authored sources. The native compiler continues using the authored modules. Checking in generated C as another bootstrap seed is optional and requires a regeneration path. Self-hosting is not evidence of performance, image, or multicore acceptance.

The siphon supplies initial Id dispatch, allocation, loading, and machine primitives. A fuller Id module takes over by installing/replacing methods on existing objects and vtables, preserving object identities and compatible payloads/invocation ABI. Subsequent sends see replacement; entered calls retain their implementation. Later representation changes use the migration contract. The siphon remains reachable as a replaceable object. The target startup loads fuller Id, concurrency, base library, and additional modules; the first interpreter milestone need not already implement that entire lifecycle.

## Remaining implementation decisions

The first milestone must concretize dispatch entry and the bootstrap language subset without weakening the contracts above. Its proposed details live in [the milestone](docs/first-milestone.md) and [invocation proposal](docs/seed-invocation.md).

Later work must choose and qualify the collector build/mode, native stack/context-switch implementation, safepoint and invalidation machinery, executable-code lifetime, Alien/native resource protocols, image entrypoints/encoding/selective installation, representation identifiers and migrations, native artifact ABI, and optional unloading. These are explicit open areas, not features already implemented.

## References and provenance

- [Decision discussion](Birnam%20Runtime%20Architecture%20%E2%80%94%20Review%20Notes.md) and [earlier architecture record](Birnam%20Runtime%20Architecture%282%29.md).
- [Collector research](Collector%20Research.md) and [image-loading research](Image%20Loading%20Prior%20Art.md).
- Piumarta and Warth, [Open, Extensible Object Models](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf).
- Hölzle, Chambers, and Ungar, [Debugging Optimized Code with Dynamic Deoptimization](https://bibliography.selflanguage.org/_static/dynamic-deoptimization.pdf).
