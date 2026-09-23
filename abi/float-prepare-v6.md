> Historical descriptor-era record. This boundary is retired under Design 90.
> See the [current Float message protocol](../rust/birnam-float/README.md).
> Do not restore the tables or preparation path described below.

# Float preparation v6

`birnam_float_prepare_v6` preserves the existing v5 preparation order
and appends the explicit `const PayloadLayoutV1 *` capability. It must match the
selected IdApi and outlive retained state. The layout setter/context is copied at the original callback snapshot point before behavior construction.

Live Float operations and storage codecs remain v1. IdApi v17 consumers must
rebuild. See [Payload Layout](payload-layout-v1.md),
[81BL](../docs/design/81bl-production-payload-layout.md) and the
[independent C declaration](fixtures/float-prepare-v6/float_prepare_v6.h).
