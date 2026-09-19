# Described-storage tracing v1

`libbirnam_trace.dylib` loads after the storage-description validator and before
Integer. It depends only on `birnam-abi`; it retains no IdApi, heap or runtime
state. Its graph operation is:

```c
int birnam_trace_described_v1(
    const StorageDescribedViewV1 *described,
    const TraceRequestV1 *request);
```

The [independent C header](fixtures/trace-v1/trace_v1.h) mirrors the
[Rust carriers](../rust/birnam-abi/src/trace.rs). Request and report have version
1 and exact `byte_size`; their arm64 sizes are 40 and 80 bytes. Allocation
references are 16 bytes: an Oop and its runtime-local allocation identity.
IdApi v4, its fingerprint, Method, Frame and image formats are unchanged.

## Composition and lifetime

The caller resolves and pins native entries and provider data before observation:

```text
birnam_id_observe_storage_v1(request)
  ordinary Vtable withStorageView:
    birnam_storage_describe_v1(storage, prepared providers)
      birnam_trace_described_v1(described, trace request)
        report callback
```

Pass the exact live description to the tracer inside the description visitor.
The original observation must remain active throughout tracing and reporting.
Input graph arrays stay readable and unchanged; callbacks and their data remain
live. Callbacks obey the
[storage observation contract](storage-v1.md). There is no provider lookup,
ordinary Id call, legacy callback invocation or payload read inside the tracer.
Native work buffers are independent of the observed heap. No unwinding is
permitted across the ABI.

The report carries the original description pointer, runtime identity,
observation identity and two borrowed arrays sorted by allocation identity:
`reached` and `not_reached`. Every physical allocation occurs exactly once across
them. Arrays expire when the report callback returns. The original description
and its arrays keep their existing, enclosing callback lifetime. Copies of
identities or Oops neither root storage nor extend object/module lifetime; a
copied result describes only its recorded observation and is not current after
ordinary execution resumes.

## Meaning and coverage

Roots come from the description validator's combined core/provider roots. Edges
are the union of the Id core edges and validated payload edges. Duplicate roots
and edges, shared children, cycles and self-edges are valid. Null roots mean
absence; edge endpoints must identify physical allocations exactly. All edges
are validated, including those whose sources are not reached. The tracer checks
allocation address/identity uniqueness and graph carrier shape. Representation
ownership and slot validation remain the storage validator's responsibility;
arbitrary forged or expired descriptions are outside the native contract.

`accept_gaps = 0` is strict: any coverage gap returns `INCOMPLETE` without
reporting. A diagnostic caller explicitly names the gaps it accepts. Unaccepted
bits, including future unknown bits, still reject the request. Accepted gaps
remain visible in the report. The tracer preserves the union of storage and
description gaps and adds the missing-payload bit when the missing-description
count is nonzero. No root provider or caller-supplied root silently clears a gap.

Production observations currently omit native roots and may omit legacy roots,
hooks and upper payload descriptions. They therefore cannot produce a strict
complete report yet. Diagnostics report reachability relative to supplied facts.
`not_reached` does **not** mean dead or reclaimable. Even complete graph coverage
would not establish finalizer, weak-index, native-code or retirement obligations.

Permanence is an allocation fact, not an implicit graph root. Permanent objects
are partitioned by declared reachability like other physical allocations; their
flags remain available through the original view. This is not the production
collector's permanent-payload scanning policy. Deferred logical allocations have
no Oop and do not enter either list; their count, charged bytes and gap remain
available through the original storage view. Tracing never settles them.

Statuses reuse storage v1: `OK=0`, `INVALID=1`, `INCOMPLETE=3`; callback errors
propagate exactly, including negative values. There is no persistent state to
reset. Nested calculations over the same live description use independent
buffers; starting a nested Id observation remains `BUSY`.

Run `mise run storage-tracing` for the actual production dylib C fixture, and
`mise exec -- cargo test -p birnam-trace --locked --offline` for graph and carrier
tests. These are focused debug checks and require no installation or release
validation.
