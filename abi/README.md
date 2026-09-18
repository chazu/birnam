# ABI evidence

`idapi-v2.json` describes the current arm64 macOS IdApi: 53 fields, 424 bytes.
Design 81J deliberately removes `ensurePrivateVtable`; consumers call the loaded
object-model service instead. Existing v1 native consumers must be rebuilt.
Backward compatibility is not a requirement for this refactor.

The Rust mirror has compile-time offsets and exports a diagnostic descriptor.
The descriptor's format version remains 1, while its IdApi schema version is 2.
Its fingerprint matches the independent v2 manifest. The v2 fingerprint algorithm
is SHA-256 over compact, key-sorted UTF-8 JSON, excluding the `fingerprint` member.
The workspace tests verify that fingerprint and every compiled field offset.

`fixtures/idapi-v2/idapi_v2.h` is the current independent C header. The routine
native gate compiles the v2 client/module and checks the table and separately
loaded object-model implementation. Other active foreign fixtures use this header.

`idapi-v1.json` and `fixtures/idapi-v1` preserve the historical 54-field ABI and
its frozen binary evidence. They are not changed to match v2. Their old opt-in
compatibility script applies only to a v1 floor; running that client against v2
is unsupported. The archived v4 image remains historical evidence as before.

The Cargo workspace and `scripts/rust-install.sh` remain the build authority.
ABI manifests and fixtures are build/test evidence, not runtime dependencies.
The installed native inventory now includes the separate object-model dylib.
