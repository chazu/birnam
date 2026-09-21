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
visitor-failure propagation, index retirement and detached allocation release,
admission rejection, and image pinning
after initializer failure. Next it exposes only the root-registry library and
constructs the actual native root owner through Id. A second independent C module
retains an object before Runtime exists, checks repeated retain/release and token
nonreuse, and publishes methods that later query/release it. Only then does the
fixture expose the remaining libraries. Normal host startup adopts the same Id
graph, carrier and root registry; precise collection preserves the early object
until release and reclaims it afterward. It then invokes typed
methods through ordinary sends and verifies native failures and unwrapped scalar
results become language Errors. For construction after adoption, the fixture
passes the already-created root registry and collector as explicit initializer
operands, registers its temporary roots before allocating, and verifies an actual
collection preserves the new object's identity and payload. This does not establish
the full staged construction graph or remove upper code statically linked in Id.
