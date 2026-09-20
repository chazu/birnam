# Dispatch Cache v1

Design [81BU](../docs/design/81bu-production-inherited-lookup-cache.md) moves the
64-slot inherited dictionary-answer table into `libbirnam_dispatch_cache`.
Each Runtime owns a separate table. Id keeps canonical lookup eligibility,
direct dictionary lookup, mutation epochs, collection admission and target
classification/acquisition. This cache owns neither language roots nor code
leases; it never invokes a cached Method.

`birnam_dispatch_cache_factory_v1` returns an immutable 56-byte descriptor:
`version: u32`, `byte_size: size_t`, then five function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one empty native table |
| `destroy(context)` | Free the table exactly once |
| `lookup(context, epoch, shape, selector) -> method` | Return the slot's Method pointer on an exact epoch/shape/selector match, or null on a miss |
| `insert(context, epoch, shape, selector, method)` | Replace the selected slot with the supplied keys and non-null Method pointer |
| `clear(context)` | Reset all 64 slots to the original empty entries |

Shape and selector are inert `uintptr_t` keys. The selected slot is
`((selector >> 4) ^ (selector >> 10)) & 63`. Lookup does not change the table.
Methods remain pointers throughout storage and copying; the module does not
dereference keys, invoke pointers, create code references or encode Methods as
integers. Empty entries have epoch `u64::MAX`, zero keys and a null Method.
The native table supports all epoch/key values; Id still bypasses saturated
epochs before calling it.

Callers supply a live context and serialize access through existing Runtime
ownership. Only creation/destruction allocate/free memory. Lookup, insertion
and clearing invoke no callbacks and allocate nothing. No Runtime, Heap,
Vtables, execution registry or borrowed source pointer is retained. A moved
Runtime retains its table, and clearing or destroying one owner does not
affect another.

Id probes only after a direct dictionary miss and only while Idle with an
unsaturated mutation epoch. It inserts present inherited Methods only; missing
answers retain a colliding entry and follow ordinary DNU resolution. A hit is
classified and acquired afresh even if registration changed without a vtable
mutation. GC clears at the original root-emission site, after native/initial
roots and before floor/provider roots. The two [guard-verdict cells](guard-verdict-v1.md) have independent native
owners in the same library; their eligibility evaluation remains in Id.

The host pins and attaches the factory through
`birnam_dispatch_cache_dependency_v1` before Id initialization. Validation
checks version, exact size and all five operations without calling them.
Repeating the same descriptor succeeds; retargeting fails. The ordinary
twelve-argument Siphon initializer runs after Allocation Budget and adds no
language methods or exports. Existing initializer order is preserved. IdApi
v20, Native Loader v12 and image v8 are unchanged; saved images contain no
table entries or native owner pointers.

The independent C declaration is
[dispatch_cache_v1.h](fixtures/dispatch-cache-v1/dispatch_cache_v1.h).
Run `mise run dispatch-cache` for module, ABI, Runtime, loaded-factory and host
admission checks. `scripts/dispatch-cache-test.sh PREFIX` selects installed
libraries. `scripts/dispatch-cache-image-smoke.sh PREFIX` checks dynamic startup,
source-free compiled callers, mapped/rebuilt native Code images and baked
execution with inherited dispatch, warm mutation, custom lookup effects, GC,
aliases and module reload.
