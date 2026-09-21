# Native Object Model owner v1

The independent C header freezes a 64-byte completed owner descriptor, mirrored
by [`object_model.rs`](../../../rust/birnam-abi/src/object_model.rs). It owns the
private-behavior associations and a complete existing Enumeration v2 interface.
Canonical owner query, replacement and weak-record retirement invoke no Id
lookup or policy callback. Native code and the descriptor outlive all users.
Destroy the owner once, after the collector and enumeration consumers release it.

`libbirnam_object_model:birnam_native_module_v1` takes Words pointing to completed
Root Registry, Namespace and Base Values descriptors. It acquires Id's dictionary
facts through native lookup, initializes the canonical associations from actual
objects, and installs working private-behavior operations on the Id receiver.
The returned Word points to the owner. Duplicate construction is rejected,
including after partial publication; entered implementation code stays pinned.
No String, Integer, Error, collector or Runtime is required.

| Selector on Id | Native operands | Initial result |
|---|---|---|
| `ownerOf:` | Object candidate | Object owner, or Unit if no association |
| `ownerOf:put:` | Object registered behavior; Object owner or Unit to clear | The behavior |
| `prepareBehaviorMutationOf:` | Object | Its current behavior, or InvalidRequest |
| `ensurePrivateVtableFor:` | Object | Its existing or newly created private behavior |

Private construction sends the current preparation, ownership, delegation and
header-reassignment methods. It does not hold a map borrow across a send.
A newly created behavior is rooted until its owner is recorded and it is attached
to the caller-rooted object, including after GC becomes available. Registered
behavior objects themselves are already their own private mutation targets.
Malformed operands and non-behavior ownership targets have native failures.

Object Model's enumeration interface invokes exact Id fact callables selected
during construction: `behaviorCount`, `behaviorAt:` and `selectorAt:in:`. It owns
no copied behavior/selector registry and requires no Runtime view callback.
Index misses return null in the existing C enumeration interface; selector
queries still require a registered behavior before their bounds are checked.
Its public enumeration descriptor remains replaceable as before.

Associations are weak by behavior key. They do not globally root their owners:
a live behavior contributes its owner edge when the collector traces it. Upper
Runtime adopts the same owner for behavior tracing and enumeration. Retiring a
dead behavior removes its association before Id releases its allocation.
The ordinary `ensurePrivateVtableFor:` entry remains the actual early native
implementation after adoption and later module mounting.

The temporary Runtime v5 constructor adopts roots, Lifetime, base identities,
Namespace and Object Model; it replaces v4. Runtime adds complete preparation,
reassignment and owner/parent adapters with explicit Runtime contexts, so calls
through bare Id do not require a preexisting active-runtime scope. The query
adapters temporarily preserve the legacy all-Oop null sentinel (an Object-null
operand) until the remaining native consumers migrate. That compatibility rule
is not the final typed query contract. Legacy `id_init` harnesses retain local
owner maps and the old enumeration factory until those consumers migrate.

The installed native-loading check constructs a private behavior with successors
absent, then roots only that behavior. Its owner and method survive adoption and
precise collection; releasing the behavior permits both to be collected and
removes the weak association. The same construction/release check runs again
with Runtime already active. The test also checks enumeration, native argument
failures, delegation changes and preservation of the native ensure entry.
