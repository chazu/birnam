# 80X — Asynchronous process domains

**Follow-up:** [80Y](80y-task-services-and-program-setup.md) adds opt-in task
services with cooperative remote cancellation and program preparation helpers.

The process backend now supports multicore work through asynchronous requests,
with completions integrated into the ordinary Birnam task executor. Each worker
has its own process, Runtime, heap, and execution owner. No GIL is introduced.
This implements the first multicore increment in Design 80; thread-hosted domains
remain separate work.

## Usable path

Workers start from an explicit image and named one-argument method, as before.
The following task body runs in code selected for resumable execution, using the
existing CompiledUnit selection API (as in `scripts/multicore-smoke.sh`):

```birnam
let left := [Isolate start: #Worker selector: #compute: image: "workers.image"];
let right := [Isolate start: #Worker selector: #compute: image: "workers.image"];
let work := [TaskControl start: [:scope |
  let a := [TaskControl request: 100 from: left timeout: 5000];
  let b := [TaskControl request: 200 from: right timeout: 5000];
  #[[a await] [b await]]]];
[work pollOutcome];
[left stop];
[right stop];
```

Inside a task, each request returns an owned child task promptly. Both workers
can execute before either result is awaited. Other local tasks continue running
while replies are pending. The host `TaskControl start:` convenience still drives
until its ready/ingress work drains; this does not by itself provide an interactive
REPL event loop or integrate unrelated host I/O.

Worker startup and the old `request:timeout:` remain blocking. Start the workers
before entering latency-sensitive task work. Code updates use stop/recreate from
a new image; there is no automatic cross-domain propagation.

## Transport and admission

The additive, ordinary handle protocol is:

| Send | Result |
| --- | --- |
| `submit: value timeout: milliseconds` | Copy and admit a request, returning its positive request ID or `IsolateFailure`. |
| `pollRequest: id` | `nil` while pending, `#[value]` on success (including `#[nil]`), or `IsolateFailure`. Collection consumes the request. |
| `terminate` | Explicitly terminate/reap the worker process, including while busy. |

A worker admits one outstanding request, including a completed but uncollected
reply. A second submission rejects with `#busy / #notReady`; it is not silently
queued. IDs belong to the specific worker handle. Wrong or consumed IDs reject
without consuming the actual outstanding request. Admission means copied bytes
were accepted locally, not that the worker executed the operation.

The transport uses nonblocking Unix sockets and owned byte buffers. One poll
moves at most 64 KiB of wire data; object encoding/decoding remains synchronous
and subject to the existing wire limits. An absolute deadline starts at admission
and is observed when polled. The deadline and transport-failure paths terminate
the process, preserving the existing backend's containment behavior. A missing
reply reports uncertainty about effects; nothing retries or replays implicitly.

The registry now holds independently locked entries. Its global lock is released
before worker I/O, reaping, and Birnam handle allocation. Async decoding also releases
the entry lock before ordinary object sends. No background thread carries Oops,
uses IdApi, or retains an unaccounted native activation.

## Task ownership and cancellation

`TaskControl request:from:timeout:` is ordinary selected Birnam code. Admission,
request-field installation, and ingress registration share a protected transition.
Waiting checks readiness and actually parks under another protected transition,
so completion cannot be lost between checking and registration.

The executor polls ingress between task drives and while the ready queue is
empty. Its replaceable `idle` method sleeps one millisecond when only pending
external work remains, through the platform `Isolate idleFor:` operation. A full
readiness reactor can replace this polling policy later. Pending tasks retain
their worker handles and copied results through traced, image-transient fields.

Cancelling a request task does **not** claim that remote execution stopped. Its
shielded cleanup retains ownership and waits for collection, deadline, or explicit
process termination before settling. Successful late replies do not override the
recorded cancellation. Application failures during an ordinary request become
structured task failures; transport loss never becomes a successful nil result.

`terminate` bypasses busy application admission, but it is process termination,
not graceful remote Birnam cleanup. Cooperative remote task cancellation needs
worker-side control ingress and is not implemented here. Nor may this backend's
kill behavior be reused to forcibly unwind a future worker thread.

## Invariants and evidence

No IdApi/Method/StepMethod ABI, wire-v1 encoding, descriptor grammar, or image
version changes are made. Heap objects stay local. Scheduler and ownership policy
remain replaceable Birnam. Native code supplies copied transport and process
operations. Existing blocking request failure classes remain intact.
Kind-only language Errors now encode their absent message as an empty string;
previously that legal Error shape caused the worker to exit instead of reporting
an application failure. The wire record schema is unchanged.

Focused qualification covers the changed boundaries:

- Isolate crate tests, including fragmented replies, pending versus nil success,
  bounded polling, absolute deadlines, and disconnects without replay.
- The existing blocking-isolate smoke.
- Multicore source programs in compiled startup and source recovery: bounded
  admission, wrong/duplicate IDs, CPU overlap, root-task progress, GC with requests
  pending, cancellation ownership, application failure, deadline, and termination.
- The existing task smoke, covering timeslicing, wait identity, cancellation,
  cleanup, joins, live dispatch, and mapped/rebuilt image recovery.

The multicore fixture records sequential and parallel elapsed time plus worker
process CPU time. Aggregate CPU time exceeding elapsed time establishes actual
multicore service for that run. Timing is recorded rather than used as a flaky
load-dependent pass threshold; this is not a general performance qualification.
The smoke joins the existing native phase, adding no validation-plan phase.

The isolate crate's 12 tests, blocking-isolate smoke, and 24 multicore assertions
in each startup mode pass. The frozen IdApi client/module pass with their original
checksums; the obsolete v4 image is still rejected. The eight validation-support
tests also pass after adding the fixture to the native phase's input manifest.

| Startup | Sequential elapsed | Parallel elapsed | Worker CPU / parallel elapsed |
| --- | ---: | ---: | ---: |
| Compiled | 1.042 s | 0.609 s | 1.52 |
| Source recovery | 1.151 s | 0.641 s | 1.62 |

The existing 171 task assertions pass in each startup mode, including timeslicing
at quanta of 128 and one driver turn. Live task images reject as intended;
persisted outcomes and fresh tasks work in both mapped and rebuilt images.
Evidence is retained in `.cargo-target/multicore-evidence/`; `candidate.json`
records hashes of the changed source, tested release artifacts, and check logs.
The exhaustive optimization matrices are not rerun for this increment: validation
is scoped to the transport, task integration, and compatibility boundaries.

## Next boundary: thread hosting

The same domain model can use OS threads, but the current native module
`RUNTIME`/root-registration singletons and generated module bindings need
per-domain ownership. Audit IdApi routing, built-in module Oops, callback entry,
compiler/loader state, and code pins together before admitting two live Runtimes
into one process. Merely spawning threads around the current process worker API
would not establish those invariants. Application and third-party native globals
remain the user's responsibility; Birnam's own runtime globals are ours to fix.
