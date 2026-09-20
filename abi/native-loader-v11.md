# Native Loader v11

Version 11 preserves [v10](native-loader-v10.md) except for the explicit
[Trace Hooks v1](trace-hooks-v1.md) capability. Its installer receives this
capability after Payload Layout and before the behavior and selector. It retains
and forwards the first execution selection as the eleventh initializer argument.

See [81BM](../docs/design/81bm-production-trace-hooks.md) for the migration.
The [independent C header](fixtures/native-loader-v11/native_loader_v11.h)
records the descriptor. Consumers must rebuild; image encoding is unchanged.
