# Current IdApi v2 source fixtures

Design 81J removes `ensurePrivateVtable` from IdApi. The table has 53 fields and
is 424 bytes on arm64 macOS. Private-behavior creation is supplied by the loaded
object-model module through `ensurePrivateVtableFor:`.

`idapi_v2.h` is the independent C declaration. `client.c` and `module.c` retain
the native API coverage of the historical v1 source fixture and add the loaded
object-model boundary. `scripts/rust-driver-native-smoke.sh` builds and runs them
against its explicit prefix. No committed binary is overwritten or regenerated.

The v1 directory remains historical compatibility evidence for v1 floors only.
