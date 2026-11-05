# Collector research for Birnam

3 November 2026. Source and documentation review; no collector was built, benchmarked, or integrated. Recommendations below are proposals. The agreed architecture still leaves the collector library and collection schedule open. Upstream master branches and issue status can change; pin a version before implementation.

**Recommendation: evaluate BDWGC (Boehm–Demers–Weiser) first.** Use Go to inform fiber behavior and collection coordination, and Crystal as a concrete reference for connecting conservative collection to stackful fibers. Confidence is high in this shortlist, moderate in an eventual Boehm selection, and unestablished for Birnam's latency or throughput.

The existing contract is conservative roots and heap scanning, opaque payloads, initially non-moving allocation, a collector wrapped through `id` as a Birnam module, and Alien responsibility for foreign state. Many fibers must eventually run across multiple OS workers. Existing Rust structures are not implementation requirements.

| Candidate | Evidence and fit | Assessment for Birnam |
|---|---|---|
| **BDWGC** | Conservative C allocator/collector with ordinary and pointer-free allocations, interior-pointer support, and thread integration. [Project documentation](https://github.com/bdwgc/bdwgc/blob/master/README.md) | Closest fit to the chosen representation and C backend. Evaluate first. |
| **Memory Pool System (MPS)** | Its non-moving AMS pool supports ambiguous references, but requires scan/skip callbacks, limits references to allocation bases or supported headers, and is explicitly documented as unsuitable for production use. [AMS documentation](https://memory-pool-system.readthedocs.io/en/latest/pool/ams.html) | A possible development project, not the simplest existing collector to adopt. |
| **MMTk** | Offers non-moving MarkSweep and ConcurrentImmix; the latter has a moving full-collection fallback and a SATB write barrier. [Collector status](https://www.mmtk.io/status) | Keep as a later alternative if measurements justify deeper runtime integration. |
| **Go runtime GC** | Precise concurrent marking and sweeping, write barriers, and stop-the-world transitions integrated with goroutines. [Runtime algorithm](https://go.dev/src/runtime/mgc.go) | A coordination reference; adapting its runtime is a different undertaking from wrapping a collector library. |

MPS is not inherently incompatible with payload opacity: a conservative scanner could use allocation boundaries without understanding language fields. The practical obstacles are its pool contracts and readiness. Its other non-moving scanned pool, AWL, does not permit ambiguous references inside its objects, so it is not an immediate replacement for AMS here. MPS lists macOS and Linux ARM64 support. [AWL](https://memory-pool-system.readthedocs.io/en/latest/pool/awl.html), [platforms](https://memory-pool-system.readthedocs.io/en/latest/guide/overview.html)

MMTk likewise should not be rejected as inherently precise-only: its valid-object metadata explicitly supports conservative root and object scanning. But a Birnam binding would need to supply object/root scanning, runtime coordination, and a suitable non-moving configuration. Its binding model includes a Rust component; that would be a new dependency choice, not a reason to reuse Birnam's old Rust machinery. [Conservative scanning support](https://docs.mmtk.io/api/mmtk/util/metadata/vo_bit/index.html), [scanning interface](https://docs.mmtk.io/api/mmtk/vm/scanning/trait.Scanning.html), [binding architecture](https://docs.mmtk.io/portingguide/portability.html)

**What Go contributes.** Go pauses globally to change GC phases, scans individual goroutine stacks while those goroutines are stopped, and performs most tracing while application code runs. Allocating goroutines can assist GC. Its C-call transitions allow GC and other goroutines to proceed; callbacks re-enter runtime execution before running Go. These are useful operational patterns, independently of Birnam's scanning strategy. [Go GC](https://go.dev/src/runtime/mgc.go), [Cgo transitions](https://go.dev/src/runtime/cgocall.go)

**What Crystal contributes.** Crystal's official coroutine article describes Boehm integration for fibers on multiple threads: expose suspended stacks, update the active stack bounds for each worker, and prevent collection from observing a partially completed context switch. The current source retains that arrangement, including a lock around the switching/collection boundary. This establishes a concrete precedent, not proof that Birnam can copy the code unchanged or obtain Go's pause behavior. [Crystal article](https://crystal-lang.org/2022/02/16/bdw-gc-coroutines-support/), [current integration](https://github.com/crystal-lang/crystal/blob/master/src/gc/boehm.cr)

**Boehm's collection modes need separate evaluation.** Default marking stops the application; sweeping is largely performed on demand. Incremental mode interleaves marking with application work and uses dirty-page information to revisit modified memory. This can preserve opaque payloads without requiring language-level pointer maps. It is not equivalent to Go's continuously scheduled concurrent marker. [Algorithm description](https://www.hboehm.info/gc/gcdescr.html)

Parallel marking means multiple collector threads cooperate; it does not by itself mean application threads run during marking. Upstream documents an interaction between incremental time limits and an uninterruptible parallel marker. Measure both modes instead of assuming their benefits combine. [Scalability notes](https://github.com/bdwgc/bdwgc/blob/master/docs/scale.md)

Incremental collection depends on the build/platform's mutation tracking. Page protection can interact with debugging and native system calls writing into managed buffers. The public API's pause target is not a hard bound. It would be premature to promise low tail latency from merely enabling incremental mode. [Project documentation](https://github.com/bdwgc/bdwgc/blob/master/README.md), [current public API](https://github.com/bdwgc/bdwgc/blob/master/include/gc/gc.h)

**Proposed responsibility boundary.** These are integration requirements inferred from the sources and Birnam's decisions, not a fixed selector or data-layout design:

1. The collector module owns allocation, tracing, reclamation, and its native collector state. Its Birnam-facing behavior is reached through `id`. Establish compatible allocation from bootstrap onward so loading the module does not strand earlier objects. Exposing a module does not automatically provide migration between unrelated live heaps.
2. The fiber scheduler exposes every live stack region and saved register context, including suspended fibers and fibers moving between workers. Context-switch publication and stack retirement must be coordinated with collection. Do not scan inaccessible guard pages or assume a stack is safe to unmap merely because a fiber stopped running.
3. The collector integration chooses how workers are stopped. Compiler safepoints useful for scheduling do not imply Boehm will use them for GC; its platform-specific thread suspension must be reconciled with the scheduler. Avoid introducing two independent world-stop protocols without a demonstrated need.
4. Alien and library wrappers keep managed objects and callback code alive for as long as C can use them. They coordinate library unloading, resource release, and callbacks, including calls from foreign-created threads. A blocked native thread and a parked Birnam fiber are distinct: GC inactivity alone does not free a worker to run another fiber.
5. Generated-code ownership must remain explicit. A return address into an OS executable mapping is not automatically a reference to a GC-managed code owner. Initially retaining code until rebuild is already permitted; eventual reclamation needs direct evidence that active and suspended calls keep their code alive.

The Alien boundary has a concrete trap: ordinary C `malloc` storage is not scanned by Boehm, and thread-local storage is not guaranteed to be scanned. A C library retaining the only pointer therefore needs a root/retention arrangement supplied by the wrapper. Conservative collection does not automatically discover every C-held reference. [Project documentation](https://github.com/bdwgc/bdwgc/blob/master/README.md)

Boehm provides `GC_do_blocking` and `GC_call_with_gc_active` for inactive native work and callback re-entry, plus APIs for thread registration and coroutine stack bounds. Inactive frames are excluded from scanning and cannot freely manipulate the managed heap. These are candidate mechanisms behind Alien's contract, not an instruction to mark every C call inactive. The relevant APIs already exist in the released 8.2.12 header; master has evolved further. [Release API](https://github.com/bdwgc/bdwgc/blob/v8.2.12/include/gc.h)

**Specific upstream reports to include in qualification.** These are reports reviewed online, not failures reproduced locally:

- [Issue 985](https://github.com/bdwgc/bdwgc/issues/985), open when inspected, reports premature finalization across `GC_do_blocking` in the macOS ARM64 Homebrew 8.2.12 build when the only live reference is in a saved register. This directly affects the proposed native-call boundary. Do not qualify an installed package by version number alone.
- [Issue 984](https://github.com/bdwgc/bdwgc/issues/984), open when inspected, reports incremental marking accessing a retired thread stack under particular lazy-stack/dirty-tracking configurations. Its reproducer concerns OS threads; applying the lesson to fiber stack retirement is an integration inference.
- [Issue 951](https://github.com/bdwgc/bdwgc/issues/951), open when inspected, reports a race between library unloading and static-root enumeration. Alien must coordinate actual unmapping with collection as well as callable-method lifetime.

**Suggested evaluation, before choosing a collector or default mode:** build a small Odin/C harness on Apple Silicon, then the intended Linux target. Use a pinned collector build and actual stack switching. This is proposed follow-up work, not part of this research's completed evidence.

- Force collection with the only reference on a suspended stack, in a saved register, or through an interior pointer; then resume and verify the object.
- Exercise multiple workers, fiber migration, stack reuse, and retirement during collection.
- Exercise C-retained references, blocking calls, foreign-thread callbacks, recursive callbacks, and release while a callback remains active.
- Exercise method replacement with active/suspended old code, plus library close during collection.
- Compare normal and incremental modes on identical workloads. Record configuration, throughput, GC CPU, memory use, maximum/p95/p99 pauses, and runnable-fiber latency. Include many idle fibers, large live heaps, and mutation-heavy workloads.
- Check reclamation over repeated bounded workloads, allowing for conservative false retention; a test that merely survives while leaking everything is insufficient.

The research supports **Boehm as the first candidate to qualify**. It does not settle a latency target, endorse a particular package build, or require copying Go's collector, Crystal's lock scheme, or the previous Rust runtime.
