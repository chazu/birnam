# 48 — Concurrency: Reactors, Isolates, and Structured Operations

> **Status:** I1a retained; I1b and the L2 mailbox bridge withdrawn, 2026-07-17.
> This document retains the concurrency and isolate design history, including the
> accepted corrections from the
> [combined adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/48a-live-evolution-isolate-combined-adversarial-review.md).
> The leading direction remains a
> small single-image reactor for concurrent I/O plus process-isolated workers for
> CPU parallelism. Structured asynchronous operations may later deepen the
> interface shared by those mechanisms. Rev. 2 narrowed the first isolate to a
> blocking, single-flight worker substrate and separates it from single-worker
> replacement and live candidate rollout. Rev. 3 fixes worker recovery, public
> failures, spawn inheritance, request ids, handle durability, and bounded
> reap-pending ownership before I1a implementation; the
> [follow-up review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/50a-i1a-plan-hardening-follow-up-review.md) accepts those
> contracts for implementation. Reactor and structured-operation work remains
> exploratory and orthogonal to the shipped blocking isolate path.
>
> Builds on [40 — The Image](40-the-image.md),
> [44 — Toward Self-Hosting](44-toward-self-hosting.md), and
> [47 — CompiledUnit Lifecycle](../../design/47-compiled-unit-lifecycle.md). The existing
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

Copying is semantic, not merely an implementation consequence of processes. Every
message is encoded and decoded even if a future backend hosts several isolated
`Runtime` instances in one process. Symbols cross by spelling, records cross through
explicit schemas, and the backend may never substitute shared Oops, mutable backing
storage, or pointer identity for the wire contract.

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
                      selector: #handle:
                         image: "/tmp/worker.image"];

let reply := [worker request: #[#compile source] timeout: 30000];

[worker stop]
```

The first public surface is deliberately blocking and single-flight:

```text
start             → ready worker or bounded boot/transport error
request:timeout:  → decoded reply or application/timeout/transport/exit error
stop              → acknowledged/reaped child, or bounded terminal `#reapPending`
```

Only one request may be in flight for a worker. One-way sends, unsolicited child
events, promises, `whenComplete:`, multiple outstanding requests, and asynchronous
pool APIs require a reactor or later structured-operation layer and are not I1a.
`request:` may be a shorthand using the worker's configured default deadline; it
is never an unbounded semantic promise.

The entry is a named per-request handler in the worker image:

```birnam
[Reflect defineMethod: #handle: on: BuildWorker as: [:self :request |
  [self compileRequest: request]]];
```

The child substrate, not application code, owns the receive loop. It decodes one
frame, handles reserved control kinds such as stop (and later L2 evolution) only
between application calls, invokes the named selector once for an ordinary
request, encodes its result or application error, and only then reads the next
frame. This ownership is the mailbox safe-point seam; applications never receive
raw envelopes or substrate controls.

I1a handles one worker per opaque handle and one in-flight request per handle. A
coordinator may own several independent handles, which is required for I1b's brief
A/B overlap, but I1a supplies no pool abstraction, concurrent routing, shared
queue, or coordination policy. Supervision, retries, asynchronous request
correlation, load distribution, and unsolicited event delivery remain birnam
modules above the small mechanism.

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

The irreducible adapter needs process and binary-channel capabilities built from
`posix_spawn`, `socketpair` or pipes, `dup2`, bounded blocking `poll`, `waitpid`, and
process termination. The existing shell-based, text-only `Process` module is not
this adapter.

#### Worker boot mode

I1a adds one explicit child mode conceptually equivalent to:

```text
birnam --worker IMAGE RECEIVER SELECTOR --channel-fd N
```

The exact argv spelling is not contractual. Worker parsing precedes baked-image
and ordinary image routing. Image recovery is a shared helper that returns before
entry dispatch; its caller owns a live GC stack anchor for the entire runtime
turn. The boot order is:

1. spawn-and-exec a clean Birnam process; never execute Birnam in a post-`fork`
   copy of the parent runtime;
2. inherit the trusted parent's environment, working directory, stdout, and
   stderr, but only the dedicated protocol descriptor among nonstandard file
   descriptors; use close-on-exec/close-from spawn actions for the rest;
3. load and recover the image while suppressing its ordinary global `main` and
   REPL entry paths;
4. validate its current generation and resolve a named receiver plus an arity-one
   selector inside the child runtime;
5. create the child-local driver and resolve the host-only Codegen phase hooks;
6. enter `#running` exactly once;
7. send zero-id `#ready` only after all preceding steps succeed; and
8. enter the substrate-owned receive loop, invoking the named selector once per
   ordinary request and handling reserved controls between calls.

A boot failure sends a zero-id bounded boot-error frame when possible and exits
nonzero. The parent treats EOF before `#ready` as boot failure and enters the same
bounded terminal cleanup state machine as every other child failure.

#### Wire v1

Every frame uses a fixed 20-byte network-byte-order header:

```text
magic[4] | version:u16 | kind:u8 | flags:u8 | request-id:u64 | payload-length:u32
payload[payload-length]
```

`magic` is `BISO`; version is `1`; flags must be zero. Frame-kind values are fixed:

| Value | Kind | Payload |
| ---: | --- | --- |
| 1 | `ready` | empty |
| 2 | `bootError` | `birnam.boot-error.v1` |
| 3 | `request` | one transferable value |
| 4 | `replyOk` | one transferable value |
| 5 | `replyError` | `birnam.application-error.v1` |
| 6 | `stop` | empty |
| 7 | `stopped` | empty |
| 8 | `protocolError` | `birnam.protocol-error.v1` |

Zero is invalid; 9–31 are reserved for later substrate controls such as L2; 32–255
are reserved and rejected by wire v1. Request ids begin at one and increase
monotonically through `maxInt(i64)` so every diagnostic id remains a Birnam
`Integer`. `request`, `stop`, and later correlated controls use a nonzero id and
the response echoes it. `ready` and `bootError` use zero.
`protocolError` echoes the offending id only when the header was trustworthy,
otherwise zero. Attempting to advance beyond `maxInt(i64)` makes the handle
terminal with `#requestIdExhausted`. One request is in flight, but the id detects a
stale or mismatched reply and leaves room for later multiplexing without changing
the header.

The first implementation fixes conservative resource limits: 16 MiB per frame,
64 levels of nesting, 65,536 elements in one container, and 1,000,000 decoded
nodes total. The decoder rejects unknown versions/tags, oversized lengths,
duplicate decoded map keys, invalid UTF-8 where text is required, cyclic input,
and trailing bytes. Encoders enforce the same limits while building into a bounded
buffer and reject cycles, non-transferable values, or oversized output before any
frame byte is written.

The payload tag table is also fixed:

| Tag | Value | Encoding after tag |
| ---: | --- | --- |
| 0 | nil | empty |
| 1 | false | empty |
| 2 | true | empty |
| 3 | integer | signed two's-complement i64, network byte order |
| 4 | float | bit-preserving IEEE-754 f64 as network-order u64 |
| 5 | string | u32 byte length, then arbitrary bytes |
| 6 | symbol | u32 byte length, then a valid UTF-8 spelling |
| 7 | vector | u32 element count, then recursively encoded values |
| 8 | map | u32 pair count, then recursively encoded key/value pairs |
| 9 | record | u16 UTF-8 schema-name length and bytes, u16 field count, then fields in registered order |

Integers outside i64 are not transferable. All f64 bit patterns are preserved,
including NaN payloads, but floats are not legal map keys. Map keys are restricted
to nil, booleans, integers, strings, and symbols; duplicate keys after decoding are
an error. Collection elements and map values must recursively be transferable.
Shared object identity is not preserved.

Wire v1 fixes three substrate record schemas. Each has exactly two fields in this
order: `kind` (Symbol) and `message` (String):

- `birnam.boot-error.v1`;
- `birnam.application-error.v1`; and
- `birnam.protocol-error.v1`.

Unknown schemas, wrong field counts, and wrong field types are protocol errors.
Later application schemas require explicit registration on both sides; extending
that registry is an open design, but these I1a records are not. Empty-payload
kinds must have a zero payload length.

Reads and writes loop over partial progress and interruption. EOF at a frame
boundary is orderly peer exit; EOF within a header or payload is a transport
error. A malformed or resource-violating frame produces `protocolError` when safe,
then closes the channel. Application errors are encoded as `replyError`; transport
and child-exit errors remain distinct coordinator results.

Birnam errors are values. A handler result answering `isError` true is encoded as
`replyError`; applications return error-shaped data by wrapping ordinary fields in
a transferable vector or map. The coordinator maps every failure to a value-like
`IsolateFailure` answering `isError` true, with read-only `class`, `kind`,
`message`, `exitStatus`, and `requestId` queries. Classes are `#boot`,
`#application`, `#protocol`, `#encode`, `#timeout`, `#transport`, `#exit`, `#busy`,
and `#stopped`. This preserves the remote kind while making the failure source
structurally distinguishable without ambient last-error state.

Successful `start` answers the ready opaque handle, a successful request answers
the decoded reply, and `stop` (including repeated stop after a reaped exit) answers
its handle. A stop that exhausts the reap budget answers an `#exit` failure with
kind `#reapPending`; repeated stop performs one nonblocking drain attempt and
answers the same failure until the reap succeeds.

#### Worker ownership and termination

One opaque Birnam handle stores only a monotonically increasing nonzero `u64`
token for a process-local native registry entry. Tokens are not reused during the
process lifetime; exhaustion refuses another start. The entry exclusively owns:

- the PID and channel descriptor;
- lifecycle state `#starting`, `#ready`, `#busy`, `#stopping`, `#reapPending`, or
  `#exited`;
- the one in-flight request id;
- channel closure and the sole authority to record a successful child reap; and
- owned-byte receiver, selector, image-origin, and exit diagnostics.

PID, descriptor, pending request, and native wait state never live in ordinary
durable Store cells. The registry retains no Oops. An exited tombstone remains
while its handle is reachable, making repeated stop idempotent without token reuse.
Image save rejects a reachable handle in durable Store state with
`#nonDurableIsolateHandle`; it never silently omits the binding or serializes the
token, PID, or descriptor.

Graceful stop is `stop` → `stopped` or EOF → `waitpid`. A bounded blocking timeout
then escalates through terminate and kill as needed. The owner polls
`waitpid(..., WNOHANG)` through the final reap budget. A successful wait is
recorded once as `#exited`; if the process is not yet waitable, the owner becomes
terminal `#reapPending` and the caller returns within its deadline. Repeating stop
after either terminal state is idempotent.

Every blocking start, request, health, and stop operation has a deadline. A
post-write request timeout is terminal for that worker: close the channel,
terminate and then kill on the configured deadlines if needed, transition to
`#exited` or `#reapPending`, and return a timeout result. A pre-write timeout leaves
the worker ready. Later nonblocking host drains retry the sole pending wait until
they record the one successful reap. Code never resumes use of a worker whose
reply may arrive after its caller timed out.

GC finalization does not block on process shutdown. If a Birnam handle becomes
unreachable, its finalizer only marks the native entry orphaned and enqueues it in
a process-local cleanup registry. The native module exports nonallocating `drain`
and bounded `shutdown` hooks. Host runtime frames drain at top-level and around
worker calls, and install shutdown as a defer after loading the isolate module.
The hook performs the same bounded close/terminate/reap-or-pending sequence and
reports a PID still pending after its final budget rather than blocking without a
deadline. Explicit `stop` remains deterministic ownership; a program that never
yields a host safe point cannot demand prompt orphan cleanup.

Ordinary host-boundary `drain` never sleeps or polls with a positive timeout. It
closes newly orphaned channels, sends signals whose monotonic escalation times
have arrived, performs `waitpid(..., WNOHANG)`, and returns. Repeated boundaries
advance cleanup; only explicit operations and the final shutdown hook consume a
bounded positive wait budget.

The parent resolves absolute executable and image paths before spawn. Both channel
endpoints prevent peer-close writes from raising SIGPIPE (`SO_NOSIGPIPE` on Darwin
or `MSG_NOSIGNAL` where available). I1a inherits the trusted parent's environment
and working directory. Environment filtering, per-worker overrides, captured
output, credential changes, and hostile sandboxing are later policy. A request's
service deadline begins before encoding and covers traversal, write, and reply
wait. Expiry before the first frame byte is nonterminal. Expiry after that point
is terminal and starts the separate bounded graceful/TERM/KILL-to-reap cleanup
budget; the caller-visible upper bound is the supplied service timeout plus those
configured cleanup budgets.

Birnam modules may own:

- frame construction and parsing;
- the transferable-value codec;
- inbox and envelope objects;
- worker-facing request/reply policy; and
- later pools, restart policy, and durable job descriptions.

No callback should enter a Birnam runtime from a foreign OS thread. The coordinator
receives child events through its reactor or through an explicitly blocking wait.

Process startup and value copying require jobs coarse enough to amortize their
cost. Stable content-addressed code mappings may later let the OS share read-only
pages between workers, but current image boot uses temporary dylib paths and I1a
does not claim this optimization.

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

I1b proves only single-worker rolling replacement:

1. worker A is the coordinator's selected worker and has no request in flight;
2. build or save a new worker baseline and start worker B;
3. receive B's transport `#ready`;
4. perform one application-defined health `request:timeout:`;
5. between calls, replace the coordinator's selected-worker reference A with B;
6. send stop to idle A and reap it. If A becomes `#reapPending`, keep B selected,
   expose that partial-cleanup outcome, and let the sole native owner finish A's
   reap on later drains.

If B fails to boot, times out, or fails health, A remains selected and B is closed,
terminated if necessary, and reaped. I1b does not route concurrently, drain a busy
worker, replace a pool, retry automatically, or claim that external effects made by
the health request are reversible. Once healthy B is selected and A termination
starts, a delayed A reap never switches selection back to A.

Long-lived stateful isolates may instead receive a compiled-unit candidate and
activate it at a mailbox safe point. Activation is atomic within each isolate,
never globally atomic across a pool. The coordinator records which generation
each worker accepted and may request per-worker rollback while valid. This is the
L2 bridge in [49 — Live Module Evolution](49-live-module-evolution.md), not I1a or
I1b.

### 5.6 Properties

Isolates offer strong ownership, crash isolation, and multicore execution. Their
costs are IPC, copying, worker memory, startup, and explicit code rollout. They are
best for coarse jobs and per-core server workers, not message-per-arithmetic-step
parallelism.

Process isolation is not a security sandbox. A trusted worker retains the user's
filesystem and process authority plus the environment and descriptors deliberately
passed at spawn. Sandboxing, privilege reduction, and hostile-worker containment
are separate designs.

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
let compile := [workers requestOperation: #[#compile source]];

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

## 9. Interleaved implementation sequence

Concurrency work now interleaves with live evolution so neither design invents the
other's boundary in the abstract:

```text
L1a — generation authority and image coherence
→ I1a — blocking single-worker isolate substrate
→ L1b — file-backed revision and image-baseline workflow
→ I1b — single-worker rolling replacement
→ L2  — blocking mailbox-safe live rollout
```

1. Complete L1a from
   [49 — Live Module Evolution](49-live-module-evolution.md): one current
   generation per runtime, sealed source-map/base digests, expected-base commit,
   exact local rollback, and coherent full-image recovery. Commit is synchronous
   at an explicit quiescent top-level boundary; L1a invents no queue or promise.
2. Implement I1a: clean worker boot mode, wire-v1 framing and bounded codec, opaque
   process-local ownership, one blocking request at a time, crash/EOF observation,
   and bounded stop/terminate/reap.
3. Complete L1b: candidate-safe file import/export, divergence, dependency
   metadata/order, staged inspection, and production of saved worker baselines.
4. Prove I1b through the single-worker A→B health-checked handoff in §5.5. Pools,
   busy draining, retries, and candidate delivery remain absent.
5. Implement L2 only after I1b: transfer source/artifact identity plus a value-like
   base digest in a blocking control request, bind preparation to the worker's
   runtime-local generation, activate between requests, and return one value-like
   receipt. Per-worker rollback is another blocking control request while valid.
6. Add the deterministic reactor adapter, nonblocking socket results, and `poll`
   when concurrent I/O is the next product requirement; port the HTTP server to
   receiver/selector turns and measure live replacement under load.
7. Extract only the structured operation, supervision, and scope behavior demanded
   by the first real reactor and isolate programs.
8. Consider compiler sugar, fibers, or in-process isolated runtimes only after
   measurement identifies a concrete limitation.

The reactor remains orthogonal rather than blocked behind L2. If network throughput
is the immediate target, the reactor work in step 6 may run before I1a. If CPU
parallelism or failure isolation is the target, I1a proceeds with explicitly
blocking coordination. Neither path requires shared-memory threads. L2 uses the
mailbox boundary proved by I1a instead of retrofitting asynchronous completion into
the isolate substrate.

Commit-sized implementation steps, initial timeout choices, concrete file owners,
and phase exit gates are specified in
[50 — Five-Phase Implementation Plan](50-five-phase-live-modules-and-isolates-implementation-plan.md).

## 10. Acceptance questions and measurements

During implementation and prototyping, the phase gates should answer:

- How many idle and active connections can the `poll` reactor serve before
  kqueue/epoll materially matters?
- What is the overhead of one receiver/selector turn relative to the measured send
  constant?
- Can compiled-unit activation occur between turns with deterministic subsequent
  behavior and no stale long-lived callback blocks?
- What job size amortizes isolate encoding and IPC?
- Can worker mode recover an image, suppress `main`/REPL, resolve its named entry,
  and report ready before accepting a request?
- Does the substrate retain the receive loop, invoke the application handler once
  per ordinary request, and consume stop or L2 controls only between calls?
- Do wire-v1 limits reject oversized, deeply nested, truncated, unknown, and
  malformed messages without unbounded allocation or coordinator corruption?
- Do all fixed tags, scalar map-key rules, duplicate detection, f64 bit patterns,
  and substrate error records round-trip identically on both ends?
- Does every exit/stop/error path close descriptors within its bound, retain one
  reap owner until `waitpid` succeeds, and record that success exactly once, with
  no durable PID/channel facts or native Oops in the registry?
- Does request timeout make the worker terminal, boundedly terminate/reap it, and
  leave the coordinator usable; does orphan cleanup do the same at the next host
  safe point without blocking GC?
- Can a worker crash or timeout leave the coordinator and its previously selected
  worker healthy?
- Does single-worker replacement leave A selected on every B boot/health failure
  and select B only between blocking calls?
- What are worker startup time, codec overhead, and steady-state RSS before any
  code-page-sharing optimization?
- Does image save clearly reject or exclude runtime-only registrations, channels,
  and pending replies while retaining recovery descriptions?
- Can both the reactor adapter and isolate transport be replaced in tests without
  altering application methods or sharing Oops/backing storage?

## 11. Remaining open decisions

Plan 50 fixes I1a's starting choices: only the three substrate record schemas are
registered, applications use scalar/vector/map payloads; boot/request/stop/TERM/
KILL defaults are 10/30/2/2/2 seconds; and stdout/stderr are inherited while the
protocol uses its dedicated descriptor. A public application-record registry,
captured output, and other defaults require evidence rather than blocking I1a.

1. Whether a simple multi-process blocking HTTP server should precede the reactor
   as the fastest parallel-server experiment.
2. Which failure policy a future task scope applies when one child fails.
3. Whether reactor registrations live entirely in process-local objects or have
   reflected, explicitly non-durable fact records for inspection.
4. How an application declares durable recovery descriptions without making
   ordinary transient tasks look persistent.
