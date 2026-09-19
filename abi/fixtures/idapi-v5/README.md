# Current IdApi v5 source fixtures

Design 81R removes Float's four fields after Integer's extraction in 81L.
The table has 41 fields and occupies 328 bytes on arm64 macOS.

`idapi_v5.h` is the independent C declaration. `client.c` and `module.c`
retain native API coverage and check the separately loaded numeric families,
object-model boundary, ordinary owner/parent replacement and reparenting.
The numeric helper prepares Float, mounts Integer with that dependency, then
attaches Integer to Float. Fixtures that need Float methods activate its
`module_init` afterward. No committed binary is regenerated.

The v1-v4 directories remain historical evidence for their respective floors.
