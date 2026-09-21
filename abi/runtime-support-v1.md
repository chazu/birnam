# Native Runtime support factories v1

The existing Method Observation, Storage and Dispatch Cache libraries expose
`birnam_native_module_v1`. Each initializer takes exactly one native Bytes operand
selecting a complete factory, and returns its pinned descriptor address as a Word.
Missing, extra, wrongly typed or unknown requests return `InvalidRequest` without
creating state. These entries need only Id's loader; Runtime and value modules
can be physically absent.

| Library | Bytes request | Returned existing descriptor |
|---|---|---|
| `libbirnam_method_observation.dylib` | `methodObservation` | `method_observation::FactoryV1` |
| `libbirnam_storage.dylib` | `observation` | `storage_observation::FactoryV1` |
| `libbirnam_storage.dylib` | `deferredIdentity` | `deferred_identity::FactoryV1` |
| `libbirnam_storage.dylib` | `deferredIndex` | `deferred_index::FactoryV1` |
| `libbirnam_dispatch_cache.dylib` | `dispatchCache` | `dispatch_cache::FactoryV2` |
| `libbirnam_dispatch_cache.dylib` | `guardVerdict` | `guard_verdict::FactoryV1` |

Every advertised factory operation works on return. Creating a method observer
requires its caller to supply a completed admission callback; the library neither
stores a global Runtime dependency nor awaits later callbacks. The other factories
manage independent native state without callbacks. The caller destroys created
state using the corresponding factory and retains any code/context it supplies.
Factories hold no Id objects and need no collector adoption.

`runtime_support::FactoriesV1` collects copies of these six existing descriptors
for [Runtime construction v15](runtime-construction-v15.md). Its `repr(C)` header
is `u32 version` (1), padding, then native `usize byte_size` (352); fields follow
in the table's order at offsets 16, 56, 120, 168, 248 and 304. Runtime validates the
header and all six factories before adopting any predecessor, then gives each
consumer its own copy. There is no setter or delayed attachment in this path.
The six legacy setter exports have been removed.

The independent [C declaration](fixtures/runtime-support-v1/runtime_support_v1.h)
and `scripts/fixtures/runtime-support-native.c` exercise every operation, independent
state and direct/context-bearing cache results through actual native loading.
`runtime-support-host.rs` runs with only Id, Bootstrap and these three libraries
available. The full native-loading suite also rejects malformed factory inputs
with otherwise valid Runtime predecessors, then successfully adopts those same
predecessors. The obsolete empty `module_init` entries and their `SCRIPT_MODULES`
rows are removed; no later load is required to make these factories usable.


The installed native suite runs the consolidated support admission matrix in
`scripts/fixtures/runtime-support-host.py`. Its 76 cases use actual Id and
Bootstrap with Runtime absent: missing libraries/entries, incompatible native
headers, missing initializers, failed initialization, wrongly typed or null
results, and each factory's invalid header or missing operation. A C proxy
changes only the selected result of the real native entry; other support requests
continue to use the installed implementation. Each descriptor fault records that
the intended request was reached. A final unmodified prefix exercises every
factory operation and repeated selection.

This replaces the five old per-factory host matrices that constructed fake Id
images and tested dependency setters. Missing/rejected setter cases are obsolete;
constructor rejection and the installed export-absence gate cover the current
boundary. The separate historical behavioral C fixtures still need migration.
