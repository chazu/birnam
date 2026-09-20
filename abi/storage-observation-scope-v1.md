# Storage Observation Scope v1

Design [81BT](../docs/design/81bt-production-storage-observation-scopes.md)
moves each Runtime's observation epoch, active flag and one-use permission into
the existing `libbirnam_storage`. Id retains runtime identity numbering,
admission order, live `withStorageView:` dispatch, physical fact construction
and the two reset guards. The description-provider scopes are separate.

`birnam_storage_observation_factory_v1` returns an immutable 64-byte factory:
`version: u32`, `byte_size: size_t`, then six function pointers. Its 16-byte
state record contains `epoch: u64`, `active: u32` and `permit: u32`.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a distinct native scope initialized to zero epoch and false flags |
| `destroy(context)` | Free that scope exactly once |
| `read(context, out)` | Copy the state, encoding each flag as zero or one |
| `set_active(context, enabled)` | Assign the active flag; any nonzero integer means true |
| `set_permit(context, enabled)` | Assign the permission flag with the same boolean convention |
| `advance_epoch(context, out_epoch) -> int` | Checked increment; return 1 and write the new epoch on success, or return 0 on overflow without changing state or output |

Flag assignment is unconditional and epoch advancement leaves both flags
unchanged. Each operation requires a live scope from this factory, owner-
serialized access, and valid caller-owned output storage where applicable.
No operation calls Id or foreign code. Read, assignment and advancement allocate
nothing. Creation/destruction manage only native scope storage. No Runtime,
Heap, object, visitor, request or borrowed view is retained. Moving a Runtime
retains its scope; separate runtimes do not share one.

The host pins the descriptor and code and attaches it through
`birnam_storage_observation_dependency_v1` before Id initialization. Admission
validates the version, exact size and all operations without invoking them.
Repeating the same descriptor succeeds; retargeting fails. This maps Storage
early without running its normal Siphon initializer early or invoking its
description validator.

Id consumes permission before attempting epoch advancement. Overflow returns
INVALID without visiting. Visitor failure consumes the accepted epoch; the
existing guards clear active and permit at their respective return/unwind
boundaries. Nested host observation and ordinary unpermitted invocation retain
BUSY. Owner-thread checks still precede active-observation rejection.

Storage's description `ACTIVE` builder pointer, provider sink tokens and
`NEXT_TOKEN` counter are unchanged. Public storage ABI v1, IdApi v20,
twelve-argument native initialization, Native Loader v12 and image v8 are
unchanged. This factory adds no language methods or image records.

The independent C declaration is
[storage_observation_v1.h](fixtures/storage-observation-scope-v1/storage_observation_v1.h).
Run `mise run storage-observation-scope` for focused module, ABI, Runtime,
loaded-factory and host-admission checks. The underlying script accepts an
installed prefix. `scripts/storage-observation-scope-image-smoke.sh PREFIX`
checks dynamic startup, source-free compiled callers, mapped/rebuilt native
Code images and baked execution. Its host consumer verifies successful
protected observations after execution, separately from nested rejection
during native methods and collection callbacks.
