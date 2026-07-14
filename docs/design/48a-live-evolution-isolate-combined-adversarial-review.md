# 48a — Combined Adversarial Review: Live Evolution and Isolates

> **Status:** Accepted after corrections and final spot-check, 2026-07-14.
>
> Scope: [45 — Compiled Units Are Primary](45-compiled-units.md),
> [47 — CompiledUnit Lifecycle](47-compiled-unit-lifecycle.md),
> [48 — Concurrency](48-concurrency-reactors-and-isolates.md),
> [49 — Live Module Evolution](49-live-module-evolution.md), and their canonical
> roadmap surfaces.

## Verdict on the first integrated draft

**Reject until blockers are corrected.** The reviewer found the ordering

```text
L1a → I1a → L1b → I1b → L2
```

sound and compatible with a later reactor/structured-operation layer, but found
five contracts that did not yet compose.

## Findings and resolutions

### 1. The application owned the worker receive loop

**Finding:** I1a passed an inbox to a long-running application entry method while
L2 required the substrate to intercept evolution controls between ordinary
requests. An application could consume the control as ordinary data or never
return control to the substrate.

**Resolution:** The child substrate now owns the receive loop. The configured
selector is a per-request handler. Stop and later L2 controls are reserved frame
kinds processed only between handler calls; applications never receive raw
envelopes or substrate controls.

### 2. Safe-point authority was only a convention

**Finding:** The design promised that a running turn remains on its starting
generation, but the implemented `CompiledUnit activate` surface is an ordinary
call and the enforcement mechanism was left open. A handler could activate in its
own turn and observe two generations.

**Resolution:** L1a now requires a native, process-local runtime phase guard.
Only the shell/tool host after top-level return, the isolate driver between
requests, or a future reactor between turns can enter `#quiescentEvolution`.
Commit and rollback require the guard; ordinary code cannot mint or persist it.

### 3. Blocking health and cleanup had no deadline-bearing primitive

**Finding:** I1b required bounded health and stop behavior, but I1a exposed only a
blocking `request:`. A hung handler could trap the coordinator. The draft also
relied on unspecified GC-finalizer scheduling to reap a dropped worker.

**Resolution:** I1a now exposes `request:timeout:` and makes timeout terminal for
the worker: close, terminate/kill on bounded deadlines, and reap exactly once.
Native registry ownership is explicit. GC only marks and enqueues an orphan;
host safe points drain cleanup without blocking the collector.

### 4. The legacy-authority cutover was mandatory but left open

**Finding:** L1a claimed the sealed generation source map superseded
`activeCompiledUnit` and `__modsrc__`, while current compile/replay paths still use
those legacy facts and their migration was an open decision. This allowed two
competing authorities or a partial first generation.

**Resolution:** L1a now owns an all-or-nothing cutover: reconcile dispatch,
validate and topologically order legacy sources, derive the sealed map and portable
provenance, construct the composed generation, then publish the one authority.
Legacy facts become derived compatibility mirrors. Failed cutover publishes
nothing and refuses new-contract activation/save.

### 5. Wire v1 framing did not define an interoperable codec

**Finding:** Header framing and broad value categories were fixed, but tag values,
numeric encodings, map-key rules, duplicate semantics, and substrate error records
were not. Two implementations could decode different values or fail to copy an
application error.

**Resolution:** Design 48 now fixes frame-kind and value-tag tables, i64 and
bit-preserving f64 encodings, scalar map keys, duplicate-key rejection, positional
record encoding, and exact boot/application/protocol error schemas. L2 consumes
reserved kinds 9 and 10 and fixes its activate, rollback, and receipt records.

## Additional consistency correction

The future asynchronous wrapper example now uses `requestOperation:` rather than
overloading I1a's blocking `request:` meaning.

## Follow-up verification

The reviewer confirmed that all five blockers were substantively closed without
introducing an asynchronous or reactor prerequisite. The follow-up found two
remaining high-priority clarifications:

1. the cross-runtime base digest needed a versioned canonical byte encoding and
   hash algorithm; and
2. “one worker” needed to mean one worker per handle so I1b can briefly own A and B
   without implying a pool abstraction.

Design 49 now specifies domain-separated SHA-256 encodings for source revisions,
source maps, and portable operations, including deterministic ordering and
explicit exclusions. Design 48 now permits several independent handles while
retaining one request per handle and no pool/coordinator policy.

The final spot-check accepted both clarifications and found no remaining blocker or
hidden asynchronous dependency.

**Final verdict: accept.** The sequence remains:

```text
L1a → I1a → L1b → I1b → L2
```

The remaining open decisions are representation choices or later policy, not
missing prerequisites for these slices.

The representation choices and executable work breakdown were subsequently fixed
in [50 — Five-Phase Implementation Plan](50-five-phase-live-modules-and-isolates-implementation-plan.md).
