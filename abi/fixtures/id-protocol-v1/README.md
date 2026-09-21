# Id native protocol v1

`id_protocol.h` is the independent C declaration of the typed native boundary
specified by [Design 83](../../../docs/design/83-production-id-construction-and-lifetimes.md#3-the-common-native-loading-protocol).
It is frozen separately from the upper, legacy IdApi table. The
[initial object operations](../../../docs/design/83-production-id-construction-and-lifetimes.md#initial-id-object-operations)
create behaviors/objects and query Id-owned facts through messages without
requiring language value modules.

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
and invocation, admission rejection, and image pinning
after initializer failure. It then makes the upper libraries available and proves
normal host startup adopts the same Id graph and carrier. It then invokes typed
methods through ordinary sends and verifies native failures and unwrapped scalar
results become language Errors. For construction after adoption, the fixture
passes the already-created root registry and collector as explicit initializer
operands, registers its temporary roots before allocating, and verifies an actual
collection preserves the new object's identity and payload. This does not establish
the full staged construction graph or remove upper code statically linked in Id.
