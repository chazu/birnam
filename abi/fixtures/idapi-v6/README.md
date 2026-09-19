# Current IdApi v6 source fixtures

Design 81T removes Word's four fields after Float's extraction in 81R.
The table has 37 fields and occupies 296 bytes on arm64 macOS.

`idapi_v6.h` is the independent C declaration. `client.c` and `module.c`
retain native API coverage and check the separately loaded numeric families,
object-model boundary, ordinary owner/parent replacement and reparenting.
The numeric helper prepares Word and Float, mounts Integer with Float attached,
then attaches Integer to both modules. Fixtures needing Float/Word methods
activate their `module_init` afterward. No committed binary is regenerated.

The v1-v5 directories remain historical evidence for their respective floors.
