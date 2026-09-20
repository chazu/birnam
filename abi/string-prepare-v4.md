# String preparation v4

`birnam_string_prepare_v4` preserves the existing v3 preparation order
and appends the explicit `const PayloadLayoutV1 *` capability. It must match the
selected IdApi and outlive retained state. The layout setter/context is read at the existing assignment point after behavior construction.

Live String operations and storage codecs remain v1. IdApi v17 consumers must
rebuild. See [Payload Layout](payload-layout-v1.md),
[81BL](../docs/design/81bl-production-payload-layout.md) and the
[independent C declaration](fixtures/string-prepare-v4/string_prepare_v4.h).
