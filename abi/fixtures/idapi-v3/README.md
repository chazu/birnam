# Current IdApi v3 source fixtures

Design 81K removes owner/parent getters and setters after 81J removed
`ensurePrivateVtable` from IdApi. The table has 49 fields and
is 392 bytes on arm64 macOS. Private-behavior creation is supplied by the loaded
object-model module through `ensurePrivateVtableFor:`.

`idapi_v3.h` is the independent C declaration. `client.c` and `module.c` retain
the native API coverage of the historical v1 source fixture and check the loaded
object-model boundary, ordinary owner/parent replacement and reparenting. `scripts/rust-driver-native-smoke.sh` builds and runs them
against its explicit prefix. No committed binary is overwritten or regenerated.

The v1 and v2 directories remain historical evidence for their respective floors.
