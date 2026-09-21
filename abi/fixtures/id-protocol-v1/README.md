# Id native protocol v1

`id_protocol.h` is the independent C declaration of the typed native boundary
specified by [Design 83](../../../docs/design/83-production-id-construction-and-lifetimes.md#3-the-common-native-loading-protocol).
It is frozen separately from the upper, legacy IdApi table.

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
method/context publication and invocation, admission rejection, and image pinning
after initializer failure. It then makes the upper libraries available and proves
normal host startup adopts the same Id graph and carrier. This does not establish
the full staged construction graph or remove upper code statically linked in Id.
