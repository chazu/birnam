# Current IdApi v7 source fixtures

Design 81X removes String's three fields after Word's extraction in 81T.
The table has 34 fields and occupies 272 bytes on arm64 macOS.

`idapi_v7.h` is the independent C declaration. `client.c` and `module.c`
retain native API coverage and check the separately loaded String and numeric families,
object-model boundary, ordinary owner/parent replacement and reparenting.
The helper prepares String, Word and Float, mounts Integer with Float attached,
then attaches Integer to both modules. Fixtures needing Float/Word methods
activate their `module_init` afterward. No committed binary is regenerated.

The v1-v6 directories remain historical evidence for their respective floors.
