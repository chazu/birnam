# Dispatch Cache v2

Design 83 changes cached dictionary answers from a single legacy Method pointer
to the complete [native entry](fixtures/id-protocol-v1/README.md): function and
context. The cache remains a bounded, non-owning optimization above Id. It does
not invoke entries, retain objects or acquire executable ownership.

`birnam_dispatch_cache_factory_v2` returns a 56-byte descriptor with version 2,
`size_t byte_size`, and five function pointers. Its independent C declaration is
[dispatch_cache_v2.h](fixtures/dispatch-cache-v2/dispatch_cache_v2.h).

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one empty 64-slot native table |
| `destroy(context)` | Free that table |
| `lookup(context, epoch, shape, selector) -> NativeEntry` | Copy the complete entry on an exact key match; return null function/context on a miss |
| `insert(context, epoch, shape, selector, entry)` | Replace the selected slot with the supplied keys and complete entry |
| `clear(context)` | Empty all slots |

Only create/destroy allocate or free. Callers serialize access; no cache borrow
survives a callback. Shape and selector remain inert address keys. The slot is
`((selector >> 4) ^ (selector >> 10)) & 63`. Each slot grows from 32 to 40 bytes.
The cache retains neither roots nor leases, and its contents are not image data.

Runtime probes after a direct dictionary miss, with canonical lookup proven,
an unsaturated mutation epoch and no active collection. It copies the entry,
then classifies and acquires any registered implementation on every send. A
context-only replacement invalidates the old answer just like a function change.
The existing collection clear point and callback bypass are unchanged.

The current host attaches the factory through
`birnam_dispatch_cache_dependency_v2` after real Id initialization and before
the aggregate Runtime constructor. This remains a migration boundary; the final
upper construction path must remove Id's dependency forwarding export.
Version 1 is rejected. IdApi v20 and image runtime ABI 7 are unchanged.

`scripts/dispatch-cache-test.sh PREFIX` covers the table, ABI, Runtime, loaded
factory and host admission. `scripts/dispatch-cache-image-smoke.sh PREFIX`
covers source, compiled, mapped/rebuilt and baked execution. The production Id
loading fixture also invokes typed C methods through ordinary upper sends;
Runtime tests cover cached contexts, effectful lookup replacement and failure,
and Machine calls and tail calls.
