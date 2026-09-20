# Payload Layout v1

This native module owns the existing exact-vtable payload descriptions. See
[81BL](../docs/design/81bl-production-payload-layout.md) for the ownership audit,
caller migration and validation status.

`birnam_payload_layout_factory_v1` supplies an immutable 72-byte factory:
version, extent, capability creation/destruction, prefix copying/release, slot
selection, layout presence and all-Oop extent checking. The host pins its image
and attaches it through Id's `birnam_payload_layout_dependency_v1` before
`id_init`. Attachment is inert, accepts the same pointer again and rejects
retargeting. Ordinary module initialization publishes no methods or objects.

The 48-byte floor descriptor has version, extent, context, ordinary admission,
exact-record replacement and exact-record borrowed view callbacks. Creation
copies this descriptor without invoking it. The 40-byte public capability has
version, extent, context, `set` and `slot_kind`. Id publishes it through
`birnam_payload_layout_provider_v1`; native initialization receives it as its
tenth argument, paired with the selected execution API.

A 24-byte view contains the byte pointer, count, repeated tail and presence bit.
Its backing bytes remain borrowed until replacement or destruction of the exact
record. A distinct 24-byte owned carrier transfers those bytes exactly once into
a floor attachment, or is released once through the creating factory. No Rust
container representation crosses the boundary. Unset and explicitly empty
layouts have null prefix pointers and allocate no byte buffer.

The public setter admits before validating/copying input and copies before
exact membership lookup. Zero length never reads the input pointer; all byte
codes remain accepted. Slot lookup admits before exact membership lookup, uses
`SLOT_UNKNOWN` for unset layouts and never inherits a parent's layout. Internal
presence and all-Oop queries remain callback-free, including during storage
observation and collection. Canonical operations and destruction do not consult
the public descriptor, so its instrumentation cannot redirect internal behavior.

All descriptors, contexts and code outlive retaining consumers and owned
carriers. The runtime controls record retirement in separate per-behavior metadata.
Design 83 removes the raw payload mirror. This contract adds no roots or
mutation-epoch change,
collection policy or image format. Native consumers must rebuild for IdApi v17.

The [independent C header](fixtures/payload-layout-v1/payload_layout_v1.h) and
[Rust declaration](../rust/birnam-abi/src/payload_layout.rs) assert the carrier
sizes and offsets. `mise run payload-layout` runs the focused debug gate.
