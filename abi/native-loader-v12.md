# Native Loader v12

Version 12 preserves [v11](native-loader-v11.md) except for the explicit
[Finalizers v1](finalizers-v1.md) capability. The installer receives this
capability after Trace Hooks and before the behavior and selector. It retains
and forwards the first execution selection as the twelfth initializer argument.

See [81BN](../docs/design/81bn-production-finalizers.md) for the migration.
The [independent C header](fixtures/native-loader-v12/native_loader_v12.h)
records the descriptor. Consumers must rebuild; image encoding is unchanged.
