# 80Y — Task services and program setup

This follows [80X](80x-asynchronous-process-domains.md). The next increment adds
an opt-in task service to the process backend and packages program preparation
in ordinary Birnam. Thread hosting, channels, and general timer/I/O integration
remain separate work.

## Program API

```birnam
let program := [Concurrency load: #["worker.birnam" "application.birnam"]];
// Application's methods and nested blocks are now resumable.
[Application run: program];
[program close];
```

`Concurrency load:` reads named modules, loads their ordinary initializers,
registers their sources, selects resumable implementations with CompiledUnit,
and captures a worker image. It owns the image and compiler artifacts until
`close`. Supply the complete source batch before starting the initial dispatch
generation. The helper currently reports an error if that generation already
exists; live editing through the existing reflection and CompiledUnit APIs is
unchanged. This is a setup convenience, not automatic whole-program selection
or a replacement for the project compiler.

The existing resumable compiler restrictions still apply. Keep initializer
callbacks inside named methods; arbitrary top-level callback blocks cannot
currently be selected as resumable implementations.

Preparation runs before tasks and worker handles exist. Image capture happens
after conditional setup callbacks return: checked Machine activations retain
their existing image rejection behavior. No exemption is added to image capture.
Temporary program bookkeeping uses the existing image-transient attribute
mechanism; worker processes do not inherit preparation objects or handle lists.

An application creates independent serial services with:

```birnam
let worker := [program worker: #MyWorker];
let scope := [TaskControl start: [:scope |
  let request := [worker request: #[#compute 42] timeout: 5000];
  [request await]]];
[worker shutdown];
```

The named receiver implements ordinary `handle:`, selected for resumable
execution by program preparation. Each invocation runs in a fresh independent
request scope; `[TaskExecutor current]` supplies that request's task. The service
collects its outcome after children and cleanup settle before accepting the next
request. Request failure does not fail the long-lived service. Worker policy and
the receiver protocol remain ordinary, replaceable Birnam methods.

`request:` returns an owned child task inside a task. At the host boundary it
uses the existing synchronous drain convenience. Its default process request
deadline is 60 seconds; `request:timeout:` specifies milliseconds explicitly.
Startup and image preparation remain synchronous. One request is admitted at a
time; a second request rejects with `workerBusy`. There is no hidden service queue.

## Cancellation and shutdown

- Cancelling a local request queues at most one small control frame for that
  request ID, independently of application admission. The owner progresses its
  write through the existing bounded transport poll.
- The worker polls control between task turns, including when its request is
  parked. A cancellation reaches the ordinary task cancellation/cleanup path.
  It does not forcibly unwind native calls.
- A fragmented control frame remains intact across request publication. Old
  request IDs are ignored and cannot cancel subsequent requests.
- Local control admission is not remote acknowledgement. If publication wins
  at the worker, a late cancellation has no effect there. The local task still
  observes its own recorded cancellation and retains ownership until collection.
- Cleanup failures received while the local task is cancelled remain failures;
  they cannot be discarded in favor of a cancelled outcome.
- `shutdown` immediately closes local admission, then drains accepted work and
  stops the process. Its drain/stop cleanup is shielded from cancellation of the
  shutdown observer from task creation, including cancellation before its first
  turn. It releases the Birnam wrapper's native handle after stopping. It returns
  a task so callers inside a scope can join it.
- `cancel` closes admission, requests cancellation of active work, and joins
  shutdown. `terminate` remains explicit process termination, without a claim
  that Birnam cleanup ran.
- Detected connection loss requests cancellation in the worker. Cleanup may
  still block indefinitely; no remote cleanup acknowledgement can be invented
  after a connection is lost.

The process request deadline remains a containment limit: expiry terminates the
process and reports a transport-level failure, even if cleanup was in progress.
It is not a separate graceful cleanup budget and is not a future thread-kill rule.

## Outcomes and compatibility

The new service sends an explicit task outcome inside a successful transport
reply. Success, failure, and cancellation are separate from transport status.
Failure evidence and nested child outcomes are retained; language Error failure
evidence uses a copied `#[#error kind message]` record. An ordinary returned Error
is a successful task value, but is not automatically transferable; unsupported
result data produces `resultTransfer`, rather than declaring a remote task failure.
`RemoteFailure outcome` exposes the copied remote failure outcome.

The existing blocking and asynchronous isolate APIs keep their legacy behavior.
`Isolate startService:image:` explicitly selects the new service protocol. It
uses tagged copied values in existing wire-v1 Request frames, with cancellation
identified by the frame's request ID. Framing and the value codec are unchanged;
the service request envelope uses one of the existing nesting levels. No IdApi,
Method/StepMethod, descriptor, or image version changes are introduced.

No background thread carries Oops or invokes IdApi. Control and task execution
stay on each domain's owner. Application and third-party native globals remain
the user's responsibility; automatic cross-domain live editing is not added.

The worker cleanup fixture also exposed a pre-existing Cell image defect: its
three object-reference words were declared RAW, so image recovery preserved
stale store/entity/attribute addresses. Cell now declares those slots OOP. The
payload layout and ABI are unchanged; newly saved images relocate references
using the existing image codec. This does not repair older images that already
contain raw Cell addresses; recreate those images from source. A standalone
save/load program reproduced the crash on the previous installed release.

## Focused evidence

The focused checks passed:

- Isolate crate: 15 tests; store crate: 11 tests, with 2 existing ignored tests.
- Task service: 19 checks in each of compiled and source startup. These cover
  CPU and parked cancellation, cleanup-before-settlement, cleanup failure
  precedence, service reuse after failure, explicit result transfer failure,
  draining shutdown, cancellation before shutdown starts, and service cancellation.
- Cell image recovery: read, write, collect, and read again through mapped and
  rebuilt recovery.
- Existing task executor: 171 checks in each startup mode, plus task image recovery.
- Existing asynchronous multicore: 24 checks in each startup mode; measured
  worker CPU/elapsed ratios of 1.52 and 1.61 demonstrate concurrent CPU execution.
- Existing blocking isolate smoke and frozen IdApi compatibility fixture.
- Direct Birnam multicore example: both runs found 1,229 primes; sequential
  34.38 seconds, parallel 26.37 seconds, average worker CPU usage 1.78 cores.
  These are single-run observations, not a speedup guarantee.

Logs are under `.cargo-target/multicore-evidence/`. Broad optimization matrices
are not part of this increment.
