# Birnam ABI evidence

This directory contains repository-side compatibility evidence. Nothing here is
loaded by Birnam at runtime, installed, used for artifact selection, or included
in the release runtime layout.

`idapi-v1.json` is the independent expected description of the current arm64
macOS `IdApi`: field order and layout, C-callable signatures, pointer
nullability, callback types, slot constants, target assumptions, and a
deterministic fingerprint. It is review evidence, not a runtime module.

The Rust workspace exercises this boundary through its own unit and FFI tests.

The `IdApi` contract is append-only. Adding a trailing field requires an explicit
compatibility review and a deliberate manifest/fingerprint update. Insertion,
reordering, removal, signature changes, or nullability changes must fail the
existing v1 check; do not regenerate the manifest merely because both language
implementations changed together. A genuinely incompatible ABI requires a new
versioned manifest and migration decision.

The fingerprint is SHA-256 over the verifier's deterministic sequence of
length-prefixed target, type, field, and constant descriptors. It is evidence
against unnoticed drift, not a runtime ABI-version field.

The Cargo workspace and `scripts/rust-install.sh` are the sole build authority.
The installed prefix contains the Rust host, the fifteen Rust native dylibs,
and the authored Birnam bootstrap modules; no alternate implementation or
compatibility build graph is required.
