# Allocation Budget v1

Design [81BR](../docs/design/81br-production-allocation-budget.md) moves accumulated
allocation charges into `libbirnam_allocation_budget`. Each Heap owns one opaque
native budget, including heaps created during bootstrap. This scalar counts
allocation work since the last reset; it is not a live-storage measurement.

`birnam_allocation_budget_factory_v1` returns an immutable 64-byte descriptor:
`version: u32`, `byte_size: size_t`, followed by six function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a distinct native owner with a zero budget |
| `destroy(context)` | Free that owner exactly once |
| `read(context) -> size_t` | Read the current charge |
| `replace(context, bytes)` | Assign all scalar bits exactly, including zero and `SIZE_MAX` |
| `charge(context, bytes) -> int` | Checked addition; return 1 and commit on success, or 0 without mutation on overflow |
| `preview(context, bytes, out) -> int` | Checked addition; write the projected total and return 1 on success, or leave output unchanged and return 0 on overflow; never mutate the budget |

Operations require a live context created by this factory, serialized by its
owner. Preview requires writable caller-owned output; no pointer is retained.
Only create/destroy allocate or free native memory. No operation invokes Id or
foreign callbacks, retains language objects or charges its native storage to the
Heap. Heap may therefore use the budget while its existing state borrow is live.
Moving a Heap retains its owner; separate Heaps have independent budgets.

The host pins this descriptor and implementation, then supplies it to Id's
`birnam_allocation_budget_dependency_v1` before `id_init`. Admission checks
version, exact size and all six operations without invoking them. Repeated
attachment of the same descriptor succeeds; retargeting fails. Heap owners must
be destroyed before code can be unloaded. The Siphon mounts this module through
the existing twelve-argument initializer without publishing methods or objects.

Id retains charge placement and diagnostic decisions. Ordinary charge overflow
aborts with `birnam-id invariant failed: GC allocation byte counter overflow`.
Counted-index overflow refuses the plan before mutation. Materialization saves,
zeros and restores the prior charge, avoiding double charging. Collect resets
the charge at its original completion step. Public IdApi stays v20, Native
Loader stays v12 and image encoding stays v8.

The independent C declaration is
[allocation_budget_v1.h](fixtures/allocation-budget-v1/allocation_budget_v1.h).
Run `mise run allocation-budget` for module, ABI, actual Heap and host-admission
checks. `scripts/allocation-budget-test.sh PREFIX` selects an installed library
stack; `scripts/allocation-budget-image-smoke.sh PREFIX` checks Siphon startup,
source-free compiled callers, mapped/rebuilt native Code images and baked apps.

Current production constructs and owns these controls in the early
[collection-state owner](collection-state-v2.md), then Runtime adopts the same
context. Normal startup no longer prepares this dependency inside Id. The
existing factory ABI remains available for legacy clients.
