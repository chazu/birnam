# Id native protocol v1

`id_protocol.h` is the independent C declaration of the typed native boundary
specified by [Design 83](../../../docs/design/83-production-id-construction-and-lifetimes.md#3-the-common-native-loading-protocol).
It is frozen separately from the upper, legacy IdApi table.

`entry.c` checks the actual C field layout and call/return convention. The Id
unit test loads this entry and installs/invokes it through production Id's own
initial graph and dispatch, before Runtime exists. It checks all six operand
kinds and scalar failure detail. It supplies no alternate Id implementation.

This fixture does not establish Siphon or module-construction acceptance. The
common descriptor loader and actual protocol handoff are still being migrated.
