# 50a — I1a Plan Hardening Follow-up Review

> **Status:** Accepted after corrections, 2026-07-14.
>
> Scope: [48 rev. 3 — Concurrency](48-concurrency-reactors-and-isolates.md) and
> [50 — Five-Phase Implementation Plan](50-five-phase-live-modules-and-isolates-implementation-plan.md),
> reviewed against the implemented L1a host, image, GC, generation, and module
> boot seams.

## Verdict

**Accept I1a for implementation.** The phase order remains sound:

```text
L1a → I1a → L1b → I1b → L2
```

No reactor, promise, callback, pool, post-G0 module-introduction escape, or
cross-worker authority has entered I1a. The revised I1a.0–I1a.7 sequence now has
an implementable dependency order and explicit failure boundaries.

## Original findings and resolutions

| Finding | Resolution | Status |
| --- | --- | --- |
| The plan assumed a recovery-only image seam that did not exist. | I1a.0 extracts recovery from entry dispatch, retains the GC stack anchor in the outer host frame, and makes worker ready require a validated generation plus `#running`. | Closed |
| The isolate facade could have been introduced after G0. | The native mechanism enters the normal stack and the embedded/AOT facade enters the dependency-ordered source set before G0; source-fallback parity is a narrow gate. | Closed |
| “Caught application error” had no meaning in a value-error language. | Handler results answering `isError` become `replyError`; every local/remote failure maps to a structured `IsolateFailure`; error-shaped data must be wrapped. | Closed |
| A bounded call and mandatory synchronous reap could not both be guaranteed. | Terminal cleanup records one reap owner and may return bounded `#reapPending`; nonblocking drains finish the sole eventual `waitpid` and record it once. | Closed |
| Orderly orphan cleanup had no host hook. | The native module exposes nonallocating `drain` and bounded `shutdown`; host frames drain at safe points and install shutdown after module load. | Closed |
| Spawn inheritance and peer-close behavior were underspecified. | Absolute paths, descriptor filtering, trusted environment inheritance, inherited stdout/stderr, and no-SIGPIPE writes on both endpoints are fixed and tested through an adapter. | Closed |
| Handle identity, durability, and request-id edges were open. | Handles use never-reused process tokens and are rejected in durable Store paths; correlated ids occupy `1...maxInt(i64)`, zero is reserved for boot frames, and ids never wrap. | Closed |

## Findings from the follow-up pass

### 1. The preflight initially depended on files created by the next increment

The first hardening draft put fake-process and `test-isolate` work before the
native test target and facade existed. I1a.0 now creates the isolated mechanism,
test target, facade source, and test boot set; I1a.1 publishes them into the
production stack and G0.

### 2. The worker fixture initially violated L1a module-introduction policy

A fixture helper cannot file in a new handler after G0. The plan now puts a
test-only handler module in the fixture boot manifest before sealing its G0 and
forbids a fixture-only escape hatch.

### 3. Request and cleanup deadlines initially shared an exhausted budget

The request timeout is now the service budget beginning before encoding. Expiry
before the first frame byte is nonterminal; expiry afterward is terminal and may
consume the separately configured cleanup budget. The documented caller bound is
their sum.

### 4. Header ids initially exceeded Birnam's diagnostic integer range

The wire field remains `u64`, but issued ids stop at `maxInt(i64)`, so every
`IsolateFailure requestId` is representable without truncation.

### 5. Reap-pending ownership changed I1b's post-selection failure model

Once healthy B is selected and A termination starts, an A `#reapPending` outcome
cannot honestly restore “A unchanged.” The plan now keeps B selected, reports
partial cleanup, and completes only when a later drain records A's reap.

## Residual implementation risks

These are test targets, not missing contracts:

- Zig/platform API differences for close-from, `posix_spawn`, no-SIGPIPE writes,
  and monotonic polling;
- proving image preflight rejects a durable handle without rejecting an
  unreachable or stack-local one;
- keeping the native registry Oop-free across encode, request, and diagnostics;
- making malformed-frame response best-effort without extending the service
  deadline; and
- calibrating the default timeouts from the non-gating startup/latency/RSS data.

None requires changing the public I1a boundary. If implementation evidence forces
one to do so, revise design 48 and plan 50 before widening the API.
