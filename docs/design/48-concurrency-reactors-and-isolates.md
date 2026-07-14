# 48 — Concurrency: Reactors, Isolates, and Structured Operations

> **Status:** Exploration, 2026-07-13. This document compares candidate designs;
> it is not yet a decision or implementation plan. The leading direction is a
> small single-image reactor for concurrent I/O plus process-isolated workers for
> CPU parallelism. Structured asynchronous operations may later deepen the
> interface shared by those mechanisms.
>
> Builds on [40 — The Image](40-the-image.md),
> [44 — Toward Self-Hosting](44-toward-self-hosting.md), and
> [47 — CompiledUnit Lifecycle](47-compiled-unit-lifecycle.md). The existing
> `modules/socket.birnam` already separates raw socket operations from its blocking
> accept loop and exposes the beginning of a nonblocking path.

## 1. The question

Birnam needs enough concurrency to make ordinary programs such as web servers
practical and enough parallelism to use multiple cores for expensive work. The
mechanism should preserve the system's defining properties:

- the object floor remains small;
- policy remains late-bound and preferably authored in birnam;
- the fact store retains one coherent history within an image;
- compiled-unit activation remains atomic and understandable;
- images make honest claims about what is and is not durable;
- concurrency can be removed or reimplemented without rewriting ordinary code;
- Linux support can arrive without first making the whole runtime thread-safe.

The motivating workloads divide naturally:

1. **Concurrent I/O.** Many sockets, timers, subprocesses, and other resources may
   be waiting at once. The work between waits is usually short and need not execute
   on several cores.
2. **Parallel computation.** Independent CPU-heavy jobs should execute on several
   cores. Strong data isolation is desirable and fine-grained shared mutation is
   not a goal.

Trying to satisfy both with shared-memory threads would push synchronization into
nearly every part of Birnam. This document instead explores two orthogonal
mechanisms and one higher-level interface.

## 2. Current architectural constraints

These are facts about the present implementation, not permanent language laws:

- The collector conservatively scans one native stack rooted by one stack-bottom
  marker.
- Allocation, interning, dispatch dictionaries, Store indexes, and compiler state
  are process-global and unsynchronized.
- Birnam blocks compile to ordinary synchronous native calls. There is no suspended
  continuation representation and no compiler lowering for `await`.
- Compiled-unit activation changes global dispatch for one image. Its transaction
  is defined for one prepared candidate, not several runtimes or processes.
- Alien calls may block and may hold process-local C pointers or callbacks.
- Image persistence handles heap facts, compiled code, and explicit recovery
  material. Native stacks, file descriptors, PIDs, and scheduler queues have no
  portable encoding.

Any design that introduces shared-memory OS threads must address all of these at
once. A reactor avoids that requirement by running only one turn at a time. An
isolate avoids it by giving each worker a separate runtime.

## 3. Common decisions

The candidate designs share these assumptions unless a later design reopens them.

### 3.1 In-flight execution is process-local

Images do not preserve suspended stacks, readiness registrations, pending replies,
open channels, or partially executed callbacks. Durable work is represented
separately as facts: job descriptions, retry policy, desired worker count,
completion state, and recovery methods.

On boot, recovery code may create new runtime tasks or workers from those durable
descriptions. It must not claim to resume an old native computation.

### 3.2 Concurrency has explicit safe points

Live dispatch changes occur between reactor turns or between isolate messages. A
running turn is never asynchronously interrupted by compiled-unit activation.
Every send in a later turn resolves through the current dispatch generation.

Long-lived behavior should therefore be recorded as a receiver and selector where
possible. A stored receiver/selector pair remains late-bound; a stored block may
retain code and captures from the generation in which it was created.

### 3.3 Isolation means copied values

Separate runtimes do not share pointer identity or a fact store. The initial
transferable set should be deliberately closed:

- nil and booleans;
- integers and floats;
- strings and symbols;
- vectors and maps recursively containing transferable values; and
- explicitly encoded records.

Blocks, arbitrary object identities, cells, entity handles, vtables, Aliens,
sockets, and native pointers do not cross an isolate seam. Domain identity crosses
as a stable value key rather than as an object address.

## 4. Design A: a minimal reactor

### 4.1 Shape

The smallest useful reactor has three scheduling operations:

```birnam
[Reactor post: #step to: job];

[Reactor watch: fd
         for: #read
        send: #readReady:
          to: session];

[Reactor run]
```

`post:to:` queues one zero-argument message for a later turn.
`watch:for:send:to:` installs a one-shot readiness registration for `#read` or
`#write`; when ready, the reactor sends the selector with the descriptor as its
argument. The handler must explicitly re-arm the watch. `run` drains queued turns
and readiness events until no work remains; a server remains alive by keeping its
listener watch armed.

One-shot registrations keep ownership and cancellation explicit. They also prevent
an accidentally retained registration from invoking an obsolete receiver forever.

### 4.2 HTTP example

```birnam
[Reflect defineMethod: #armAccept on: HttpServer as: [:self |
  [Reactor watch: [self fd]
             for: #read
            send: #acceptReady:
              to: self];
  self]];

[Reflect defineMethod: #acceptReady: on: HttpServer as: [:self :fd |
  let cfd := [Socket tryAccept: fd];
  [[cfd >= 0] ifTrue: [| [[self sessionFor: cfd] armRead]]];
  [self armAccept]];

[Reflect defineMethod: #armRead on: HttpSession as: [:self |
  [Reactor watch: [self fd]
             for: #read
            send: #readReady:
              to: self]];

[Reflect defineMethod: #readReady: on: HttpSession as: [:self :fd |
  let result := [self tryRecv];
  [[result isWouldBlock]
    ifTrue: [| [self armRead]]
    ifFalse: [| [self consumeOrClose: result]]]];

[server armAccept];
[Reactor run]
```

Each connection owns its own state. Callbacks are serialized, but slow clients no
longer serialize all connections because waiting happens in `poll`, not in a
blocking `recv`.

CPU work may cooperate by processing a bounded chunk and reposting itself:

```birnam
[Reflect defineMethod: #step on: SearchJob as: [:self |
  [self searchNext: 1000];
  [[self isDone]
    ifFalse: [| [Reactor post: #step to: self]]]];
```

This improves responsiveness but does not create multicore speedup.

### 4.3 Hidden implementation

The reactor implementation owns:

- a FIFO ready queue of receiver/selector records;
- a table of one-shot descriptor registrations;
- timer registrations when timers are added;
- a bounded ready-message batch per turn so queued work and I/O cannot starve one
  another; and
- process-local roots for queued receivers.

The first adapter can be birnam-authored over POSIX `poll(2)` through Alien. A
deterministic in-memory adapter should implement the same seam for tests. Later
kqueue or epoll adapters may replace `poll` without changing application methods.

No additional native stacks, VM opcodes, threads, locks, or continuation objects
are required. Reactor policy—the queue discipline, fairness, registration objects,
cancellation, and recovery—can remain birnam.

### 4.4 Socket work required

The existing socket module anticipated a reactor, but its nonblocking surface is
not complete. It needs:

- target-aware socket constants and `sockaddr` layout as part of Linux support;
- get-and-set nonblocking flags rather than replacing the complete flag word;
- nonblocking accept;
- partial read/write handling; and
- distinct results for data, EOF, would-block, interruption, and error.

The current text-only send/receive path also truncates embedded NUL bytes. Binary
buffers are independent of concurrency but will matter for a serious server and
for isolate message transport.

### 4.5 Properties

The reactor is the smallest design and the easiest to remove. It preserves one
heap, one Store history, and one active dispatch generation. A callback can reach
any global or fact, so isolation is by ownership convention rather than enforced.

A blocking FFI call or long computation stops the entire reactor. Such work must
be shortened, moved to a nonblocking operation, or submitted to an isolate.

## 5. Design B: process-isolated workers

### 5.1 Shape

An isolate is an ordinary Birnam runtime in a clean child process. It boots an
explicit image or source/module entry set and communicates through framed copied
messages.

```birnam
let worker := [Isolate start: #BuildWorker
                       image: "/tmp/worker.image"];

[worker send: #[#warmCache input]];

let reply := [worker request: #[#compile source]];
[reply whenComplete: #compiled: to: coordinator];

[worker stop]
```

The entry is a named object in the worker image:

```birnam
[Reflect defineMethod: #run: on: BuildWorker as: [:self :inbox |
  [true whileTrue: [|
    let envelope := [inbox receive];
    [[envelope isStop]
      ifTrue: [| false]
      ifFalse: [|
        [envelope reply: [self handle: [envelope value]]];
        true]]]]]];
```

Pools, supervision, request correlation, retries, and load distribution are
birnam modules above the small isolate mechanism.

### 5.2 Why processes first

Processes provide true CPU parallelism without synchronizing the current object
floor. Each worker retains one stack, one collector, one Store, one compiler, and
one dispatch environment. A worker crash does not corrupt its siblings or the
coordinator image.

The interface promises value-copy isolation, not process identity. A future
implementation could place several explicit `Runtime` instances in one process if
the global runtime state is ever refactored, without changing ordinary isolate
code. It must not reinterpret the interface as several threads sharing today's
heap and Store.

### 5.3 Hidden implementation

The irreducible adapter needs process and byte-channel capabilities, likely built
from `posix_spawn`, `socketpair` or pipes, `dup2`, `poll`, `waitpid`, and process
termination. A channel frame can begin as:

```text
length | kind | request-id | payload
```

Birnam modules can own:

- frame construction and parsing;
- the transferable-value codec;
- inbox and envelope objects;
- request/reply correlation;
- pools and load balancing;
- crash observation and restart policy; and
- durable job/recovery facts.

No callback should enter a Birnam runtime from a foreign OS thread. The coordinator
receives child events through its reactor or through an explicitly blocking wait.

Process startup and value copying require jobs coarse enough to amortize their
cost. Prestarted pools avoid paying boot cost per request. Mapping compiled code
from stable content-addressed files allows the OS to share read-only code pages
between workers rather than charging every process for unique temporary dylibs.

### 5.4 Web-server shape

A simple multi-process server can bind once, explicitly inherit the listening
socket into a prestarted pool, and let each worker use a blocking accept loop. The
kernel distributes connections among the workers. This gives immediate parallel
request handling without first adding a scheduler.

For high connection counts or mostly idle clients, each isolate can later run the
reactor from Design A. Isolates supply CPU parallelism; the per-isolate reactor
supplies efficient I/O multiplexing.

Inherited descriptors are explicit capabilities, not transferable integer values.
Dynamic descriptor passing can be deferred until a concrete need justifies its
platform-specific machinery.

### 5.5 Images and live evolution

Worker specifications, durable jobs, desired pool size, and retry state may be
image facts. Child PIDs, channels, pending replies, and in-flight execution remain
process-local and must be recreated after boot.

Exactly-once execution is not a primitive. Applications choose at-most-once or
at-least-once behavior using durable task IDs and idempotent handlers.

The default code-upgrade policy is rolling replacement:

1. build or save the new worker baseline;
2. start and health-check a new pool;
3. route new work to it;
4. drain the old pool; and
5. stop it.

Long-lived stateful isolates may instead receive a compiled-unit candidate and
activate it at a mailbox safe point. Activation is atomic within each isolate,
never globally atomic across a pool. The coordinator records which generation
each worker accepted and may request per-worker rollback while valid.

### 5.6 Properties

Isolates offer strong ownership, crash isolation, and multicore execution. Their
costs are IPC, copying, worker memory, startup, and explicit code rollout. They are
best for coarse jobs and per-core server workers, not message-per-arithmetic-step
parallelism.

## 6. Design C: structured asynchronous operations

Design C is not a third execution substrate. It is a possible deeper interface
above the reactor and isolate mechanisms.

```birnam
[Concurrency run: [:scope |
  let request :=
    [[[Async recv: conn]
      thenSend: #requestReceived: to: handler]
      timeoutAfter: 5000]
      ensure: [| [conn close]];

  [scope fork: request]]]
```

An `Operation` is a stackless state-machine object. It registers interest, returns
from the current native call, and receives completion on a fresh ordinary stack
turn. Combinators such as `thenSend:to:`, `onError:`, `ensure:`, and
`timeoutAfter:` build new operations rather than suspending a caller.

A `TaskScope` owns children and resources. It does not finish until its children
finish. A child failure may cancel siblings and become the scope result.
Cancellation is cooperative: it wakes parked operations and is observed at event
turns, I/O waits, timers, and explicit checks. It never asynchronously interrupts
arbitrary native code or FFI.

Parallel jobs enter the same style through an isolate-backed operation:

```birnam
let compile := [workers request: #[#compile source]];

[scope fork:
  [[compile timeoutAfter: 30000]
    thenSend: #installArtifact: to: coordinator]]
```

This interface offers structured lifetimes, cleanup, deadlines, and error
propagation. It is also substantially more policy than the minimal reactor. The
correct set of combinators should be extracted from real reactor and isolate code,
not designed exhaustively in advance.

## 7. Deferred alternatives

### 7.1 Stackful fibers

Fibers would permit natural `yield` or `await` while retaining the current native
compiler, but they require:

- allocating and switching native stacks;
- capturing registers correctly on every target;
- teaching GC to scan every suspended stack;
- defining how old stack frames interact with compiled-unit replacement;
- defining image-save behavior for nonportable suspended execution; and
- preventing blocking FFI from stopping every fiber.

Fibers still do not provide multicore execution. They may become attractive if
stackless operation syntax proves intolerable, but they are not the smallest or
most removable first mechanism.

### 7.2 Compiler-lowered state machines

An `await` syntax could compile a method into an explicit resumable state machine.
This avoids native stack switching but changes the language and both permanent
compilers. It also creates durable-looking continuation objects whose actual image
contract must be specified. Defer it until operation-based code demonstrates both
the need and the desired semantics.

### 7.3 Shared-memory OS threads

Threads sharing the current heap and Store would require synchronization or
ownership protocols across allocation, interning, dispatch, Store indexes, GC,
Codegen, FFI, images, and compiled-unit activation. They would enlarge the
permanent floor and make concurrency difficult to remove.

If in-process parallelism is revisited, the preferred direction is several
isolated `Runtime` instances with copied messages, not several threads mutating one
current runtime.

## 8. Comparison

The reactor has the smallest interface and implementation footprint. It solves
concurrent I/O, preserves one mutable image, and aligns particularly well with
late-bound method replacement. It does not enforce data isolation or use several
cores.

Isolates give the strongest data and failure isolation and provide real parallel
speedup immediately. They preserve the current runtime by placing concurrency
outside it. Their copied-message seam is more expensive, and live code rollout is
explicit rather than automatically shared.

Structured operations improve correctness and ergonomics once programs need
nested lifetimes, cancellation, deadlines, cleanup, and composition across both
mechanisms. Starting there risks designing a broad shallow surface before the
underlying use cases are understood.

The reactor and isolate designs compose rather than compete: a process pool can
run one reactor per worker. The main design risk is forcing them behind an
interface that falsely treats a local block as transferable. Cross-isolate work
must remain an explicit copied message or job description.

## 9. Leading implementation sequence

1. Implement a deterministic reactor adapter and registration model in pure
   birnam.
2. Complete nonblocking socket results and add a `poll` adapter.
3. Port the HTTP server to receiver/selector event turns.
4. Benchmark throughput, latency, memory, fairness, and live replacement under
   load.
5. Add clean child-process spawning, framed channels, and a copied-value codec.
6. Build isolate pools, crash observation, and rolling image replacement.
7. Extract only the structured operation and scope behavior demanded by the first
   real programs.
8. Consider compiler sugar, fibers, or in-process isolated runtimes only after
   measurement identifies a concrete limitation.

If the first target is specifically CPU-bound rather than network throughput,
steps 1–4 and 5–6 may be reversed. Neither sequence requires shared-memory threads.

## 10. Acceptance questions and measurements

Before either design becomes architectural policy, a prototype should answer:

- How many idle and active connections can the `poll` reactor serve before
  kqueue/epoll materially matters?
- What is the overhead of one receiver/selector turn relative to the measured send
  constant?
- Can compiled-unit activation occur between turns with deterministic subsequent
  behavior and no stale long-lived callback blocks?
- What job size amortizes isolate encoding and IPC?
- What are worker startup time and steady-state RSS when code mappings are shared?
- Can a worker crash, timeout, or malformed message leave the coordinator and
  other workers healthy?
- Does image save clearly reject or exclude runtime-only registrations, channels,
  and pending replies while retaining recovery descriptions?
- Can both the reactor adapter and isolate transport be replaced in tests without
  altering application methods?

## 11. Open decisions

1. Whether the first isolate transport uses a small tagged binary codec or an
   intentionally temporary JSON subset.
2. Whether a simple multi-process blocking HTTP server should precede the reactor
   as the fastest parallel-server experiment.
3. Which failure policy a future task scope applies when one child fails.
4. Whether reactor registrations live entirely in process-local objects or have
   reflected, explicitly non-durable fact records for inspection.
5. How an application declares durable recovery descriptions without making
   ordinary transient tasks look persistent.
6. Whether worker code rollout is always rolling replacement initially or also
   exposes compiled-unit activation in the first isolate slice.

