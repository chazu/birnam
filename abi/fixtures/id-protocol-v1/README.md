# Id native protocol v1

`id_protocol.h` is the independent C declaration of the typed native boundary
specified by [Design 83](../../../docs/design/83-production-id-construction-and-lifetimes.md#3-the-common-native-loading-protocol).
It is frozen separately from the upper, legacy IdApi table. The
[initial object operations](../../../docs/design/83-production-id-construction-and-lifetimes.md#initial-id-object-operations)
create behaviors/objects and query Id-owned facts through messages without
requiring language value modules. [Storage traversal](../../../docs/design/83-production-id-construction-and-lifetimes.md#id-storage-traversal)
provides roots, allocation facts and Id references using native visitors, plus
index retirement and detached allocation release for upper collectors.

`entry.c` checks the actual C field layout and call/return convention. The Id
unit test loads this entry and installs/invokes it through production Id's own
initial graph and dispatch, before Runtime exists. It checks all six operand
kinds and scalar failure detail. It supplies no alternate Id implementation.

For the installed production handoff, run:

```sh
python3 scripts/id-native-loading-smoke.py /path/to/debug-prefix
```

That fixture uses the actual host Siphon and installed Id, with upper libraries
absent at first. The independent C module checks explicit initializer operands,
object/behavior construction, Symbol byte access, method/context publication
and invocation, nested storage queries, allocation during snapshot visitation,
visitor-failure propagation, exact address membership, allocation counts, index
retirement and detached allocation release,
admission rejection, and image pinning
after initializer failure. Next it exposes Bootstrap alone. Independent C uses
its loader recursively to construct an otherwise unrooted Object result, and
exercises Protocol retention with the real Id. Bootstrap owns that result before
the root-registry library is available. The fixture then constructs the actual
root registry through Bootstrap and transfers the early root set. A second independent C module
retains an object before Runtime exists, checks repeated retain/release and token
nonreuse, and publishes methods that later query/release it. Only then does the
fixture expose the remaining libraries. Normal host startup adopts the same Id
graph, carrier and root registry; precise collection preserves the early object
until release and reclaims it afterward. A further prefix exposes only the three
primitive description libraries and Lifetime. Independent C registers payload
layouts, native trace edges and finalizers before Runtime, including a finalizer
on a behavior. Actual precise collection after adoption follows those edges;
release permits reclamation, invokes the declared finalizers and removes the
weak descriptions. Bootstrap's returned object also survives transfer, adoption
and GC until explicit result release. The fixture repeats that loading/retention
check after Runtime exists. It then invokes typed
methods through ordinary sends and verifies native failures and unwrapped scalar
results become language Errors. For construction after adoption, the fixture
passes the already-created root registry and collector as explicit initializer
operands, registers its temporary roots before allocating, and verifies an actual
collection preserves the new object's identity and payload. This does not establish
the full staged construction graph or remove upper code statically linked in Id.

The later prefix now constructs the [native Trace graph](../../../abi/trace-graph-v2.md)
with only eighteen predecessor libraries available. Independent C tests cover
actual Id roots and edges, description coverage, conservative/precise payloads,
inherited hooks and behavior-owner references before Runtime, then reuse that
same graph after adoption and precise collection. Id's object-allocation contract
provides native-word alignment and zero-initialized padding to a native-word
boundary; `allocationSizeOf:` continues to return only the requested extent.

Runtime retains completed [Id allocation primitives](../../primitive-allocation-v1.md)
for its internal construction and deferred materialization; production Heap no
longer owns or borrows Id allocation records.

[Registry operations](../../../docs/design/83-production-id-construction-and-lifetimes.md#removing-upper-access-to-id-registries)
keep Symbol and dictionary ownership in Id. The independent C consumer checks
non-interning name lookup and counts, ordered Symbol/behavior/key views,
opaque shadowing, exact method/context results, mutation epochs and dictionary
snapshot visitation with nested mutation and failure propagation, before and
after Runtime. Raw registry mutations reject all-Object arguments before changing
state; their Word addresses distinguish them from ordinary language sends. The
carrier ABI remains version 1.
