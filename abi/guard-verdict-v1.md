# Guard Verdict v1

Design [81BV](../docs/design/81bv-production-guard-verdict-caches.md) moves the
two Runtime guard-verdict cells into the existing `libbirnam_dispatch_cache`.
Each Runtime owns two independent cells, one for canonical Machine-slot access
and one for canonical lookup. The inherited dictionary table is a third,
separate owner. Id retains every eligibility expression and its evaluation
order, including current receiver behavior, mutation epochs and registration.

`birnam_guard_verdict_factory_v1` returns an immutable 48-byte descriptor:
`version: u32`, `byte_size: size_t`, then four function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one cell initialized to epoch `UINT64_MAX` and false |
| `destroy(context)` | Free that cell exactly once |
| `read(context, out)` | Copy the epoch and normalized boolean to the caller's state record |
| `write(context, epoch, verified)` | Assign both fields unconditionally; any nonzero integer means true |

The 16-byte C state record contains `epoch: uint64_t` at offset zero and
`verified: uint32_t` at offset eight, followed by ordinary padding. Only the
fields carry meaning; never inspect padding bytes. All epochs and repeated or
reordered assignments are supported. There is no validity predicate, counter,
transition graph or invalidation policy inside this module.

Callers supply live contexts and serialize access through Runtime's existing
owner-thread rules. Only create/destroy allocate/free memory. Read/write invoke
no callbacks and allocate nothing. No Heap, Runtime, language object, Method,
Frame, executable lease, borrowed descriptor or output pointer is retained.
Moving a Runtime preserves both owners; destroying one owner or clearing the
inherited dictionary table cannot affect either other cell.

Id reuses a verdict only at a matching, unsaturated current epoch. A stored
`UINT64_MAX` is never a cache hit, even after a saturated write. Both true and
false verdicts can be reused. Receiver and registration checks remain outside
the memoized facts at their original sites. GC clears the inherited table at
its existing site; it does not clear either guard cell. New Runtime instances
start with fresh owners, including when restoring an image.

Before Id initialization, the host pins and attaches the factory through
`birnam_guard_verdict_dependency_v1`, after preparing the inherited-table
factory from the same library. Validation checks version, exact size and all
four operation pointers without calling them. Repeating the same descriptor
succeeds; retargeting fails. The ordinary twelve-argument Siphon initializer
position, installed library inventory, IdApi v20, Native Loader v12 and image
v8 are unchanged. No guard cell or native context is serialized.

The independent C declaration is
[guard_verdict_v1.h](fixtures/guard-verdict-v1/guard_verdict_v1.h).
Run `mise run guard-verdict` for module, ABI, Runtime, loaded-owner and host
admission checks. `scripts/guard-verdict-test.sh PREFIX` selects installed
libraries. `scripts/guard-verdict-image-smoke.sh PREFIX` exercises dynamic
startup, source-free compiled callers, mapped/rebuilt native Code images and
baked execution with canonical/overridden lookup, active Frame reads/writes,
GC, aliases and module reload.
