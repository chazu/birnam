# Frozen Image value codecs

This local Rust library contains the retained image-v8 algorithms, ABI/hash
constants, field-value codecs, behavior-link records, retained-fact records and
native-code record codec. It has no native descriptor, live object capabilities,
mutable restoration state or filesystem publication effects.

[ImageFormat](../birnam-image-format/README.md) owns production full-format
composition and admission over Id. Image uses field/record helpers to project live
objects and interpret validated payloads, and retains its public Rust codec surface
for offline tooling. The v8 frozen fixtures and bounds/checksum tests moved with
the implementation. Atomic file publication remains in Image.
