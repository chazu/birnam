# Concurrency

Birnam has structured tasks within a domain and process workers for parallel
execution across CPU cores. Tasks in one domain share objects and take automatic
turns. Different worker processes have independent heaps; requests and results
cross the boundary as copied values. No GIL serializes these processes.

Scheduling and lifecycle policy are ordinary Birnam methods. Currently, worker
hosting uses processes. Thread-hosted domains, channels, and general asynchronous
timer/I/O integration remain future work.

## A complete program

Use a current release on supported arm64 macOS. Save this as `jobs.birnam`:

```birnam
[Module named: #concurrencyDemo requires: #[]];
[Globals at: #DoubleWorker put: [Object clone]];
[Globals at: #Demo put: [Object clone]];

[Reflect defineMethod: #handle: on: DoubleWorker as: [:self :number |
  [number * 2]]];

[Reflect defineMethod: #run: on: Demo as: [:self :program |
  let a := [program worker: #DoubleWorker];
  let b := [program worker: #DoubleWorker];
  [TaskControl start: [:scope |
    [[a isError] ifTrue: [| [scope fail: a]]];
    [[b isError] ifTrue: [| [scope fail: b]]];
    let left := [a request: 21];
    let right := [b request: 50];
    #[[left await] [right await]]]]]];
```

Save this as `main.birnam` in the same directory:

```birnam
[Module load: #os];
let program := [Concurrency load: #["jobs.birnam"]];
[[program isError] ifTrue: [| [System println: program]; [OS exit: 1]]];
let task := [Demo run: program];
[System println: [task pollOutcome]];
[program close];
```

Run it from that directory:

```sh
birnam main.birnam
```

The result is `#[#success #[42 100]]`. Both request tasks are created before
either is awaited. With substantial CPU work, their independent processes can
use different cores. For a measured workload, run
`birnam examples/multicore/main.birnam` from the repository root.

`Concurrency load:` loads the named modules' initializers, selects resumable
methods and blocks, and captures a worker image. Put task bodies in those modules;
the entry script performs setup and invokes the prepared application methods.
Supply the complete batch before the initial dispatch generation exists. An
already initialized generation gives `concurrencyAlreadyPrepared`. Compilation
still follows the existing resumable compiler restrictions: keep initializer
callbacks inside named methods, rather than arbitrary top-level callback blocks.

Setup, compilation, image capture, and worker startup are synchronous. The helper
owns temporary compiler files and the worker image until `program close`. It
does not automatically select an entire project's dependency graph or undo
initializer effects after a preparation failure.

## Tasks and waiting

These selectors apply to tasks returned by `TaskControl start:`, `scope fork:`,
and worker requests. Bodies receive their own task as an argument.

| Send | Behavior |
| --- | --- |
| `[TaskControl start: [:task | ...]]` | Create a root task at the host boundary, or an owned child inside a task. At the host boundary, drive the executor before returning. |
| `[scope fork: [:child | ...]]` | Schedule an owned child. A scope that has settled or begun joining rejects new children with `taskClosed`. |
| `[task pollOutcome]` | Return `nil` until settlement, then the immutable terminal outcome. Does not wait. |
| `[task outcome]` | Wait from another task and return the full outcome. |
| `[task await]` | Wait and return the success value; propagate failure or cancellation into the caller. |
| `[task cancel: reason]` | Record cancellation and propagate it to ordinary children. Returns the task. |
| `[task fail: evidence]` | Fail the current task explicitly; evidence may be an ordinary value. |
| `[task ensure: [| ...] cleanup: [| ...]]` | Run cleanup after normal return, failure, or cancellation once this protected operation is entered. Cleanup is shielded and may wait. |
| `[task yield]` | Give other local tasks a turn. Automatic timeslicing also bounds ordinary resumable execution. |
| `[TaskExecutor current]` | Return the current task, or `nil` at the host boundary. |

A task does not publish its terminal outcome until its owned children and
cleanup settle. An unhandled child failure fails its parent and cancels siblings.
Inspecting a failed child's `outcome` does not detach it or suppress that ownership
rule. A task service creates a fresh independent scope for each request, so a
failed request does not fail the long-lived service.

`outcome` and `await` require a task context; at the host boundary they return
`taskContextRequired`. Use `pollOutcome` there after the synchronous drive.
`TaskControl start:` and worker `request:` outside a task drive the executor;
issuing two host-level requests in sequence therefore waits sequentially. Inside
a task, requests return children promptly, allowing submission before waiting.

Native calls are not forcibly preempted. A blocking native call can block all
tasks in that domain until it returns. Resumable execution is required for waits;
a native barrier cannot become suspendable merely because the caller is a task.
There are no built-in asynchronous sleep or channel operations yet.

## Outcomes, failure, and cancellation

| Outcome | Meaning |
| --- | --- |
| `#[#success value]` | Body, children, and cleanup completed successfully. |
| `#[#cancelled reason]` | Cancellation was recorded before terminal publication, with no failure evidence. |
| `#[#failure failures cancelReason childFailures]` | Failure evidence, any cancellation reason, and retained failed child outcomes. |

Failure takes precedence over cancellation, and cancellation takes precedence
over success before publication. Cancelling an already settled task does not
rewrite its outcome. Cancellation requests cooperation; it is not proof that
cleanup has finished. Join with `outcome` to observe settlement.

Inside a prepared application method, a caller can cancel a worker request:

```birnam
[TaskControl start: [:scope |
  let request := [worker request: job];
  [scope yield];
  [request cancel: #noLongerNeeded];
  [request outcome]]];
```

That request may finish before cancellation reaches it. Remote control is
separate from application admission and carries the request ID; stale control
cannot cancel a later request. Local cancellation admission is not a remote
acknowledgement. Once a remote request was admitted, its local task retains
ownership through reply/cleanup collection or transport failure.

A worker handler gets its task with `[TaskExecutor current]`. Use
`[task fail: evidence]` for an explicit failure. A returned Error is an ordinary
success value locally; it is not automatically transferable across a worker
boundary. Unsupported result values produce a `resultTransfer` failure.

Remote task failures become Error-like `RemoteFailure` objects in the local
task's failure evidence. They answer `isError`, `kind` (`remoteFailure`), `message`,
and `outcome`. The remote outcome retains nested failure evidence; remote Error
evidence is copied as `#[#error kind message]`. Cleanup failures still count as
failures when the caller has already requested cancellation.

## Programs and workers

| Send | Behavior |
| --- | --- |
| `[Concurrency load: paths]` | Prepare a complete batch of named modules; return a program or Error. Paths are relative to the current working directory. |
| `[program worker: #Receiver]` | Start an independent process from the prepared image, using the named global receiver's `handle:` method; return a Worker or Error. |
| `[worker request: value]` | Return a task for a request with a 60,000 ms process deadline. |
| `[worker request: value timeout: milliseconds]` | Specify the process deadline explicitly. |
| `[worker shutdown]` | Close admission, drain accepted work, and stop the process. Return a cleanup task. |
| `[worker cancel]` | Close admission, request cancellation of active work, and join shutdown. Return a cleanup task. |
| `[worker terminate]` | Explicitly terminate the process. Does not promise Birnam cleanup. |
| `[worker state]` | Inspect native lifecycle state, including `ready`, `busy`, or `exited`; after graceful handle release, answer `exited`. |
| `[program close]` | Close program admission, join its workers' shutdown tasks, and remove owned temporary artifacts. Return a cleanup task. |

Each service admits one request at a time. There is no hidden queue: overlapping
requests on the same Worker fail with `workerBusy`. Create multiple workers for
parallel work. Admission occurs when the request task submits; creating a task
alone does not reserve a service slot. After shutdown begins, requests fail with
`workerClosing`; a closed program rejects new workers with `concurrencyClosed`.

Inside a task, join lifecycle operations explicitly when subsequent code depends
on release: `[[worker shutdown] outcome]` or `[[program close] outcome]`. They are
also owned children and are joined at scope exit. At the host boundary they use
the synchronous drive convenience. Shutdown cleanup is shielded from task
creation, so cancellation before its first turn cannot skip resource release.
Ordinary `ensure:cleanup:` protects cleanup only after that operation is entered.

The request deadline is a process containment limit. Expiry terminates the worker
and reports transport-level failure, even during cleanup. It is not a graceful
cleanup deadline. Native code that never returns cannot be cooperatively
cancelled. Detected owner connection loss requests cancellation in the worker,
but cleanup may block indefinitely; connection loss cannot prove remote cleanup.

## Values, live editing, and lower-level APIs

Requests and results support `nil`, booleans, signed 64-bit integers, floats,
strings, symbols, vectors, and maps made from those values. Map keys support
`nil`, booleans, integers, strings, and symbols. Objects, Cells, closures, and
native handles are not implicitly shared. Cycles, excessive nesting, oversized
containers, and frames over 16 MiB are rejected. Service envelopes consume part
of these existing limits.

Each worker starts from a snapshot of the prepared code and data. Root-domain
edits are not automatically broadcast. To adopt updated code, shut down old
workers and start replacements from a newly prepared image. The batch helper
currently prepares only an initial generation; existing CompiledUnit/reflection
APIs remain available for explicit live evolution. Application and native-library
global state remain the programmer's responsibility. Reachable live worker
handles retain the normal image rejection rules; process capabilities are not
made durable by this helper.

The lower-level [blocking isolate API](06-isolates.md) remains available. Its
default request timeout is 5,000 ms and it maps returned Errors to application
failures. Legacy asynchronous handles also support `submit:timeout:` and
`pollRequest:`; `TaskControl request:from:timeout:` adapts those to owned tasks,
but only the new service protocol supports cooperative remote cancellation.
`Isolate startService:image:` selects that protocol, and `Worker fromHandle:`
wraps the resulting handle. A raw service handle requires asynchronous
submit/poll; blocking `request:timeout:` rejects with `asyncRequired`.

Use `birnam doc concurrency`, `birnam doc worker`, `birnam doc taskControl`, and
`birnam doc taskExecutor` for selector help. `birnam docgen` includes this guide
and the same reference entries.
