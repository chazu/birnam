# Current IdApi v9 source fixtures

Design 81AX removes reverse export-name lookup into Namespace's explicit names
capability. The table has 33 fields and occupies 264 bytes on arm64 macOS.

`idapi_v9.h` is the independent C declaration. `client.c` and `module.c`
retain native API coverage and check the separately loaded String and numeric families,
object-model boundary, ordinary owner/parent replacement and reparenting.
The helper prepares Namespace before Id initialization and queries reverse names
through the initialized Names capability. It prepares String, Word and Float, mounts Integer with Float attached,
then attaches Integer to both modules. Fixtures needing Float/Word methods
activate their `module_init` afterward. No committed binary is regenerated.

The v1-v7 directories remain historical evidence for their respective floors.
