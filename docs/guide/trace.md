# Trace diagnostics

`modules/trace.birnam` publishes `Trace` during compiled bootstrap. There is no
Rust Trace implementation or native Trace library. The installer compiles the
authored artifact using the existing cold Lang/compiler seed.

The native message `report: visitor acceptingGaps: mask` (Object, Word) invokes
the current `Storage observe:` method and traverses the validated graph. It sends
`reached:identity:` or `notReached:identity:` (Object, Word) once per allocation,
ordered by unsigned allocation identity, then `complete:` with the unchanged
Word coverage gaps. The completion's native result propagates unchanged.

A zero mask is strict. Any unaccepted gap, including an unknown future bit,
fails with native INVALID_REQUEST detail `3` before report callbacks run. Storage
reports unobserved native stack roots (bit `1`); callers explicitly accept that
limitation. Missing reference owners add bit `2`.

All endpoints and allocation identities are validated, including edges from
unreached objects. Duplicate roots/edges, cycles and empty graphs are valid.
Object indexing uses Id allocation identity, not language value equality.
`notReached` means only absent from this declared graph; it never authorizes
collection or finalization.

Reporting runs within Storage's rooted observation. Each report has an ordinary
reference-slot session and a fresh callback identity. Retained callbacks fail
closed after return; nested reports fail with detail `2`. The first callback
failure is latched even when a replacement Storage ignores it. Cleanup clears
the session's inventory and visitor on success or failure. Public Module reload
preserves installed method replacements.

## Native execution boundary

Execution supplies a generic deferred-send calling convention and explicit failure emission:

- Incoming signature result `m` executes the returned request object. Its
  authored `value` method returns itself with this signature. The request supplies `nativeReceiver`, `nativeSelector`, `nativeArguments`,
  `nativeSignature`, and `nativeCleanup` through ordinary messages. Arguments use
  the existing Object/Word/Bytes signature vocabulary. Result `v` forwards every
  native carrier; `x` discards success to language nil. After acquiring cleanup,
  argument-conversion and target failures also run cleanup. Original failures
  take precedence over cleanup failures. Object results are protected and byte
  results copied before cleanup callbacks.
- `failNative:detail:` emits an explicit nonzero native status and detail from
  two Word operands. It contains no diagnostic policy.

An incoming signature prefixed with `!` additionally notifies
`nativeFailed:detail:` on its receiver if argument adaptation or invocation fails. The original failure remains authoritative.
Trace uses `!u` for its fact callbacks and `!v` for completion, including malformed
native calls.

All method bindings remain portable; requests need no native Method alias or opaque
result object. Requests, sessions, graph indexes, validation, traversal and reporting remain
Birnam-owned ordinary objects; Execution contains no graph or Trace state.

The source inventory and migration priorities before this port are preserved in
[the Rust bootstrap audit](../archive/2026-10-runtime-refactors/rust-bootstrap-migration-audit.md).

## Acceptance

The `trace_graphs` integration test compares all 4,096 three-node graph/root
combinations with independent transitive closure, then checks malformed and
ignored callbacks, unsigned identities, duplicate facts, result carriers and
collecting/failing cleanup. `storage_trace` exercises the real Storage owner
through a fixture subgraph; `trace_heap` reports its unfiltered physical inventory
with automatic GC enabled. `scripts/trace-image-smoke.py PREFIX` verifies native
carriers and collecting completion after source-free mapped and rebuilt recovery.

Trace allocates ordinary scratch objects. It does not change GC settings;
repeated observations with automatic collection disabled retain that garbage
until the caller collects. Physical allocation counts and runtime cost differ
from the former Rust implementation.

## Allocation and collection cost

Trace reuses rejection and enqueue handlers. Reachability marks each allocation
when enqueued, so there can be at most one worklist pop per allocation. A bounded
Vector fold drains that worklist; exhausted steps do nothing. It avoids the
bootstrap `whileTrue:` continuation cost without moving graph policy into Rust.
The independent 4,096-case oracle also covers empty worklists, cycles and duplicate
edges. Unsigned report ordering and validation of unreachable endpoints remain
unchanged.

GC starts with a fixed 16 MiB allocation budget, increased from 1 MiB. This is
allocation charge since the last successful collection, not a live-heap target.
Explicit `GC budget:` values remain authoritative, including values below 16 MiB.
There is no adaptive scaling. Trace itself neither sets the budget nor disables
collection.

The remaining transport work is a separate interface change: Storage already
indexes its protected inventory, but scalar callbacks make Trace rebuild indexes
and repeatedly box endpoint identities. An indexed batch should extend the
existing `Storage observe:` visitor protocol with an explicit opt-in. Trace must
still accept the scalar callbacks from replaced `observe:` methods; it must not
bypass replacements by caching a native entry. Batches need bounded, validated
indexes, a defined integer encoding, ordinary rooted Object references, unchanged
coverage bits and failure propagation, and inventory protection through completion
and cleanup. Snapshot indexes must never be confused with allocation identities.
No batch protocol is implemented in this change.
