# Vector v6

Version 6 preserves [v5](vector-v5.md) except for the explicit
[Payload Layout v1](payload-layout-v1.md) capability. Its layout stage now receives `(IdApi *, const PayloadLayoutV1 *)` and uses the supplied capability at the existing layout assignment points. All other stages retain their ordering and signatures.

See [81BL](../docs/design/81bl-production-payload-layout.md) for the migration.
The [independent C header](fixtures/vector-v6/vector_v6.h)
records the current descriptor. Consumers must rebuild; image encoding is unchanged.
