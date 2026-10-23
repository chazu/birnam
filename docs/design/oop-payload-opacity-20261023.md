# Opaque payloads without Storage classification

> **Dispatch correction:** the unapproved `apply` protocol and its forwarding
> helpers have been removed. Ordinary sends perform lookup and enter the selected
> Method directly. Earlier descriptions of application dispatch below are historical
> implementation mistakes, not architectural authority. See the
> [correction and current evidence](id-send-correction-20261023.md).

2026-10-23. Implemented in the isolated `codex/oop-only-id-slice` branch.
This is a correctness/design checkpoint, not a performance improvement or a
completed language migration. [Evidence](../measurements/oop-payload-opacity-20261023.json)
contains raw trials, source/binary hashes and qualification receipts.

The later [invariant migration](oop-invariant-migration-20261023.md) also removes
per-allocation failure-reference bits: a failure marker now belongs only to the
native return convention. Earlier measurements below remain historical evidence.

## Change

Removed `Storage`, its allocation-record bits, constructor tagging and payload-kind
switches in collection. The allocator records address, extent, identity and canonical
Oop reference bits. It does not classify Names, Methods, Faults or reference arrays.
The allocation record remains 24 bytes. Reference bits preserve the existing Oop
calling convention; they do not authorize interpreting an object's payload.

Initial payload implementations live in `id/bootstrap_values.rs`, logically above
the generic allocator and dispatcher. They remain in the same native library for
cold bootstrap. Each implementation validates its own format. Ordinary allocated
bytes can become a Name through vtable assignment; construction history is not a
hidden type. Reference arrays have an owner-defined length prefix and validated Oop
fields. Native Methods hold a bounded code declaration index, context and owner;
arbitrary payload bytes never become an unchecked function pointer. The index
selects from immutable declarations in the resident code image, not a new table
classifying individual objects.

The original implementation added an `apply` lookup after binding. That violated
the intended dispatch contract and has been removed, along with the test that
endorsed it. Current `send` enters the selected Method directly. The measurements
below retain their historical scope and are not acceptance evidence for that design.

Id reports the object's vtable to GC. Payload references come from owners registered
with Collect through ordinary objects and `referencesOf:do:`. Initial registrations
are per behavior, not per allocation. New owners do not require allocator edits.
Unknown nonempty payloads without an owner are rejected before sweeping. Resident
native-library contents remain roots for their existing lifetime; reclaimable code
images are not introduced here. Malformed reference emissions, allocation during
tracing and failed finalization retain their existing refusal behavior.

## Validation

`scripts/oop-id-slice-test.sh` passes, including a fresh target directory. The final
unit run has 17 passing Id tests. New checks cover interpreting raw bytes as a Name,
invalid native code indices, malformed reference arrays, and detached payload
contents. Earlier tests for contents after table cleanup/detachment and refusal to
dispatch through a released vtable remain present.

The native gate covers cold bootstrap, independent C and TinyCC callers, live
replacement, custom lookup/DNU, GC during entered calls, all 64 borrowed arguments,
entered Method/context survival, source-field mutation, malformed trace callbacks,
ordered finalization, partial module publication and owner-thread enforcement.
The additional callable-layout check passes with GC inside its implementation.

## Performance

Release, macOS arm64, thread CPU time. Repeated ABBA compares saved libraries from
`4a17ee7c` with this implementation, four fresh processes per variant. Each process
performs 40,000 allocations (retaining every eighth), 300,000 existing-Name intern
calls, 8,192 fresh-name operations and one collection. Existing Name construction
is outside its timer. Raw observations and heap counts are in the evidence.

| Workload | Before | After | Increase |
| --- | ---: | ---: | ---: |
| Allocation/rooting | 52.49 ns/object | 89.17 ns/object | 69.9% |
| Interning an existing 64-byte Name | 46.09 ns/call | 76.37 ns/call | 65.7% |
| Producing/interning a new 64-byte Name | 291.90 ns/name | 356.87 ns/name | 22.3% |
| GC workload | 13.32 ms | 21.78 ms | 63.5% |

This is a substantial regression. The implementation adds current `apply` lookup,
native Method validation and owner callbacks during tracing. These are concrete
additional operations, but this comparison does not isolate each one's contribution.
There is no confidence interval or Logue/application measurement. Follow-up work
must measure those paths and simplify their repeated work while preserving live
lookup and owner-defined payloads; restoring a payload catalog is not an acceptable
shortcut. This commit records the design correction without accepting the regression
as a finished optimization.

Stackless execution, source compilation, images and workers remain outside this
native slice. Synchronous native call roots do not establish full stackless or
concurrency qualification. No merge or installation of this slice was performed.
