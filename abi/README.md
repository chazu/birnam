# ABI evidence

`idapi-v4.json` describes the current arm64 macOS IdApi: 45 fields, 360 bytes.
Design 81J removes `ensurePrivateVtable`; 81K removes owner/parent getters and
setters. 81L removes the four Integer fields into an explicit
[Integer dependency](integer-v1.md). Object-model consumers use ordinary Vtable
messages. Existing v1-v3 native consumers
must be rebuilt.
Backward compatibility is not a requirement for this refactor.

The Rust mirror has compile-time offsets and exports a diagnostic descriptor.
The descriptor's format version remains 1, while its IdApi schema version is 4.
Its fingerprint matches the independent v4 manifest. The v4 fingerprint algorithm
is SHA-256 over compact, key-sorted UTF-8 JSON, excluding the `fingerprint` member.
The workspace tests verify that fingerprint and every compiled field offset.

`fixtures/idapi-v4/idapi_v4.h` is the current independent C header. The routine
native gate compiles the v4 client/module and checks the table and separately
loaded Integer and object-model implementations. Other active foreign fixtures use this header.

`idapi-v1.json` and `fixtures/idapi-v1` preserve the historical 54-field ABI and
its frozen binary evidence. They are not changed to match v4. Their old opt-in
compatibility script applies only to a v1 floor; running that client against v4
is unsupported. The archived v4 image remains historical evidence as before.

The Cargo workspace and `scripts/rust-install.sh` remain the build authority.
ABI manifests and fixtures are build/test evidence, not runtime dependencies.
The installed native inventory now includes the separate Integer and object-model dylibs.

`idapi-v2.json` and `fixtures/idapi-v2` preserve the 81J checkpoint.
The current optional Block-construction declaration is [v2](native-block-construction-v2.md).

`idapi-v3.json` and `fixtures/idapi-v3` preserve the 81K checkpoint.
The current optional numeric declaration is [v2](native-numeric-semantics-v2.md).

[Storage observation v1](storage-v1.md) adds a protected host entry and a separate
stateless description validator. It leaves IdApi v4 and its fingerprint unchanged.
The independent C fixture lives in `fixtures/storage-v1`; run the focused debug
check with `scripts/storage-observation-test.sh`.

[Storage tracing v1](trace-v1.md) adds the stateless loaded graph tracer over
described observations. Its report is strict by default and never authorizes
reclamation. Run `scripts/storage-tracing-test.sh` for the independent C oracle
against actual production dylibs.

[Live GC traversal v1](trace-live-v1.md) moves the production collector's worklist
to that same loaded module. The host and custom native embedders mount its explicit
capability before collection. Run `scripts/live-tracing-test.sh` for focused
native GC and current IdApi checks.
