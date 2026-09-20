# Native Loader v10

Version 10 preserves [v9](native-loader-v9.md) except for the explicit
[Payload Layout v1](payload-layout-v1.md) capability. Its installer receives this capability after GC Settings and before the behavior and selector. It retains and forwards the first execution selection as the tenth native initializer argument.

See [81BL](../docs/design/81bl-production-payload-layout.md) for the migration.
The [independent C header](fixtures/native-loader-v10/native_loader_v10.h)
records the current descriptor. Consumers must rebuild; image encoding is unchanged.
