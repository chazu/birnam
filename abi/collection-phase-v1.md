# Collection Phase v1

Design [81BS](../docs/design/81bs-production-collection-phase.md) moves each
Runtime's transient collection phase into the existing `libbirnam_collect`.
Id keeps its admission decisions and the guard that resets the phase on return
or Rust unwind. No language method, public IdApi field or image record is added.

`birnam_collection_phase_factory_v1` returns an immutable 48-byte descriptor:
`version: u32`, `byte_size: size_t`, then four function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a distinct native owner initialized to Idle |
| `destroy(context)` | Free that owner exactly once |
| `read(context) -> u32` | Read its current phase code |
| `set(context, phase) -> int` | Assign 0 (Idle), 1 (Marking) or 2 (Finalizing), returning 1; reject every other value with 0 and no state change |

Every valid assignment is allowed, including repetition and any ordering of
the three values. This does not impose a transition graph. Operations require
a live context created by this factory and access serialized by its owner.
Read/set allocate nothing. No operation calls Id or foreign code, retains
language objects, or retains Runtime, Heap or collection-source pointers.
Creation/destruction only manage native phase storage. Moving a Runtime retains
its owner; separate runtimes and floors never share a phase context.

The host pins the descriptor and code, then passes the descriptor to
`birnam_collection_phase_dependency_v1` before Id initialization. Admission
checks version, exact size and all four operations without invoking them. The
same descriptor may be attached repeatedly; retargeting fails. Code remains
loaded for every created context's lifetime.

Early factory preparation maps Collect's library but does not attach its
coordinator. The existing Siphon initializer and `birnam_collect_dependency_v1`
attachment retain their original positions. Collect's driver API and source
ABI remain v1, IdApi remains v20, native initialization remains twelve arguments,
Native Loader remains v12 and images remain v8.

The driver's thread-local `ACTIVE` guard is separate from a Runtime's phase.
For example, Runtime becomes Idle before pruning, while the driver remains
active through pruning. Do not merge these states or infer one from the other.
All admission decisions stay at their previous Id call sites. Invalid source
phase assignment keeps the exact `invalid collection phase` invariant failure.
Marking/finalizing allocation retains `id allocation attempted during collection`.

The independent C declaration is
[collection_phase_v1.h](fixtures/collection-phase-v1/collection_phase_v1.h).
Run `mise run collection-phase` for focused module, ABI, Runtime, loaded-owner
and host-admission checks. `scripts/collection-phase-test.sh PREFIX` selects an
installed library stack. `scripts/collection-phase-image-smoke.sh PREFIX`
checks startup, source-free compiled callers, mapped/rebuilt native Code images
and baked apps.
