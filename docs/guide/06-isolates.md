# Blocking process isolates

I1a isolates run Birnam in clean child processes. Each worker boots one explicit
image, validates its current dispatch generation, enters the running phase, and
then receives copied values through a substrate-owned loop. Workers do not run
the image's `main` or start a REPL.

## Starting and calling a worker

The receiver must be a named global in the saved image and the selector must
take one argument:

```text
let worker := [Isolate
  start: #ThumbnailWorker
  selector: #render:
  image: "/srv/app/thumbnail.image"];

let result := [worker request: job timeout: 5000];
[worker stop]
```

`start:selector:image:` blocks until the child reports ready or the boot budget
expires. `request:timeout:` takes a timeout in milliseconds. `request:` uses the
default five-second service budget. A handle permits one request at a time;
several independent handles may coexist.

Runnable examples live in `examples/isolates/`. From the repository root, run
`bash examples/isolates/run.sh` to build a worker image and exercise copied
values, concurrent handles, application failures, deadline behavior, and a
health-checked v1-to-v2 replacement built from full image baselines.

Transferable values are `nil`, booleans, signed 64-bit integers, floats, strings,
symbols, vectors, and maps composed from those values. Map keys are limited to
`nil`, booleans, integers, strings, and symbols. The wire codec rejects cycles,
unsupported objects, excessive nesting, oversized containers, and frames larger
than 16 MiB before they become shared state.

## Failures and deadlines

Every public failure answers `isError` true and supports:

```text
failure class
failure kind
failure message
failure exitStatus
failure requestId
```

Classes distinguish `#boot`, `#application`, `#protocol`, `#encode`, `#timeout`,
`#transport`, `#exit`, `#busy`, and `#stopped`. A handler that answers a Birnam
error becomes an `#application` failure. Encode failure before transmission
leaves the worker ready. Once request transmission begins, a timeout or uncertain
transport result is terminal: the coordinator closes the channel and performs
bounded TERM/KILL/reap cleanup.

`stop` is idempotent after a worker has been reaped. A rare child that has not
become waitable inside the cleanup bound reports `#reapPending`; later runtime
boundaries retry the nonblocking reap.

## Replacing one selected worker

I1b adds a blocking health-checked handoff. The caller retains the selected
handle; Birnam does not create hidden pool or routing state. Supply the next
worker description and explicit ready, health, and stop acknowledgement budgets:

```text
let spec := {
  #receiver -> #ThumbnailWorker,
  #selector -> #render:,
  #image -> "/srv/app/thumbnail-v2.image",
  #healthRequest -> healthProbe,
  #readyTimeout -> 10000,
  #healthTimeout -> 2000,
  #stopTimeout -> 2000};

let replacement := [Isolate replace: worker using: spec];
let selected := [replacement currentWorker]
```

The selected worker must be `#ready`. Birnam starts B while A remains selected,
waits for transport ready, and sends exactly one health request. The health reply
must be the Symbol `#healthy`. A boot failure, application error, timeout, crash,
or any other reply stops B and returns an `IsolateReplacement` with status
`#failed` and A in `currentWorker`; A remains usable.

After a healthy reply, selection changes to B between calls and A is stopped.
Ordinary success has status `#replaced`. If A cannot yet be reaped, status is
`#cleanupPending`, `currentWorker` is B, and `pendingCleanup` is A. Selection
never moves back after A termination begins. Every outcome supports `status`,
`currentWorker`, `priorWorker`, `pendingCleanup`, `failure`, `kind`, `message`,
and `isError`.

The policy performs no retry, concurrent routing, busy draining, pool update, or
rollback of external effects caused by the health request. Native cleanup retains
one process-local reap owner and later host boundaries continue nonblocking reap.

## Rolling out code inside a worker

L2 adds one blocking mailbox control on an idle handle. Prepare a normal L1b
candidate in the coordinator, publish its checksum-addressed artifact, and send
the copied request:

```text
let candidate := [LiveModules import: "/work/pricing-v2.birnam"];
let request := [LiveModules evolutionRequestFor: candidate];
let receipt := [worker evolve: request timeout: 10000]
```

The parent sends only module/revision identity, optional source, artifact checksum
and location, and the expected base digest. The worker compares that digest with
its own current generation, reopens and verifies the artifact locally, prepares
against its own bindings, and fulfills the resulting intent under the host-owned
quiescence guard. The substrate handles the control only between ordinary
application requests; no prepared token, Oop, generation object, handle, or
rollback capability crosses the wire.

The runnable L2 example is the final section of
`examples/live-modules/run.sh`. It prints the receipt, verifies that the next
request observes v2, rolls back by accepted digest, and verifies that the next
request observes v1 again.

An `EvolutionReceipt` supports:

```text
receipt operation
receipt module
receipt acceptedBaseDigest
receipt priorDigest
receipt currentDigest
receipt outcome
receipt errorKind
receipt message
receipt isError
```

Successful outcomes are `#activated` and `#rolledBack`; a definitive policy or
validation rejection is `#rejected` and leaves the handle ready. Timeout,
transport, protocol, or process-exit uncertainty returns an `IsolateFailure` and
makes the handle terminal under the same rules as an ordinary transmitted
request. The coordinator never infers commit merely from transport readiness.

Rollback is another digest-bound control:

```text
let rollback := [LiveModules rollbackRequestForDigest: [receipt currentDigest]];
let rolled := [worker evolve: rollback timeout: 10000]
```

Only that worker's latest process-local capability can roll back, and only while
its current digest still matches. A saved and rebooted successor retains its
generation and behavior but not the old process's rollback capability. Different
workers may accept, reject, or roll back independently; L2 provides no pool-wide
atomicity, fan-out, retry, promise, or concurrent rollout.

## Trust and durability

Workers inherit the coordinator's environment, working directory, stdout,
and stderr. Only the binary protocol descriptor is added; this is a trusted-worker
mechanism, not a hostile-code sandbox.

Handles are process-local capabilities. Saving an image whose durable Store can
reach a handle is rejected with `nonDurableIsolateHandle`; PID, descriptor, and
registry-token state are never serialized.
