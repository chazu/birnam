# ABI evidence

`idapi-v3.json` describes the current arm64 macOS IdApi: 49 fields, 392 bytes.
Design 81J removes `ensurePrivateVtable`; 81K removes owner/parent getters and
setters. Consumers use ordinary Vtable messages. Existing v1/v2 native consumers
must be rebuilt.
Backward compatibility is not a requirement for this refactor.

The Rust mirror has compile-time offsets and exports a diagnostic descriptor.
The descriptor's format version remains 1, while its IdApi schema version is 3.
Its fingerprint matches the independent v3 manifest. The v3 fingerprint algorithm
is SHA-256 over compact, key-sorted UTF-8 JSON, excluding the `fingerprint` member.
The workspace tests verify that fingerprint and every compiled field offset.

`fixtures/idapi-v3/idapi_v3.h` is the current independent C header. The routine
native gate compiles the v3 client/module and checks the table and separately
loaded object-model implementation. Other active foreign fixtures use this header.

`idapi-v1.json` and `fixtures/idapi-v1` preserve the historical 54-field ABI and
its frozen binary evidence. They are not changed to match v3. Their old opt-in
compatibility script applies only to a v1 floor; running that client against v3
is unsupported. The archived v4 image remains historical evidence as before.

The Cargo workspace and `scripts/rust-install.sh` remain the build authority.
ABI manifests and fixtures are build/test evidence, not runtime dependencies.
The installed native inventory now includes the separate object-model dylib.

`idapi-v2.json` and `fixtures/idapi-v2` preserve the 81J checkpoint.
The current optional Block-construction declaration is [v2](native-block-construction-v2.md).
