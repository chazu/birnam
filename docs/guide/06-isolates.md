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

## Trust and durability

I1a workers inherit the coordinator's environment, working directory, stdout,
and stderr. Only the binary protocol descriptor is added; this is a trusted-worker
mechanism, not a hostile-code sandbox.

Handles are process-local capabilities. Saving an image whose durable Store can
reach a handle is rejected with `nonDurableIsolateHandle`; PID, descriptor, and
registry-token state are never serialized.
