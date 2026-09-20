# Mutation Epoch v1

Design [81BW](../docs/design/81bw-production-vtable-mutation-epoch.md) moves each
Vtables registry's epoch storage into `libbirnam_object_model`. Id retains the
saturating increment, every mutation call site, and all lookup/native-tier
readers. The epoch is independent of the inherited table and guard-verdict cells.

`birnam_mutation_epoch_factory_v1` returns an immutable 48-byte descriptor:
`version: u32`, `byte_size: size_t`, then four function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one cell initialized to zero |
| `destroy(context)` | Free the cell exactly once |
| `read(context) -> uint64_t` | Return the stored epoch without changing it |
| `write(context, uint64_t)` | Assign the supplied epoch unconditionally |

The cell supports all bit patterns, repeated writes and arbitrary assignment
order. It owns storage only; it does not count, saturate, decide validity or
invalidate caches. Id's production writer still computes `saturating_add(1)`.
Its existing exhaustion helper stays private to tests and uses the same store.

Callers supply live contexts and retain the existing owner-thread and registry
serialization. Read/write allocate nothing and invoke no callbacks. Only
create/destroy allocate/free memory. No Heap, Vtable, Method, Runtime, callback,
output pointer or executable lease is retained. A moved registry retains its
owner; destruction can occur after Heap teardown and cannot affect another
registry's cell.

The host pins and attaches the descriptor through
`birnam_mutation_epoch_dependency_v1`, immediately after Vtable Enumeration
from the same library and before Id initialization. Validation checks version,
exact size and all operations without calling them. Repeated attachment of the
same descriptor succeeds; retargeting fails. Cell creation works before Object
Model's ordinary initializer and does not initialize its behavior helpers.

The existing Siphon initializer order and 81-library inventory are unchanged.
Object Model retains its Cargo output-path install name; this extraction does
not change it to an `@rpath` identity. Its export manifest records that policy.
IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8 are
unchanged. The epoch cell is not serialized; construction and dictionary replay
retain their existing behavior during image recovery.

The independent C declaration is
[mutation_epoch_v1.h](fixtures/mutation-epoch-v1/mutation_epoch_v1.h).
Run `mise run mutation-epoch` for module, ABI, exact Runtime/Vtable witnesses,
loaded-owner and host admission checks. `scripts/mutation-epoch-test.sh PREFIX`
selects installed libraries. `scripts/mutation-epoch-image-smoke.sh PREFIX`
checks startup, source-free compiled callers, mapped/rebuilt native Code images
and baked execution through live dictionary, parent and behavior changes,
active Frame slot guards, custom lookup, aliases, GC and reload.
