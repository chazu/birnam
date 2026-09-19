# ABI evidence

`idapi-v5.json` describes the current arm64 macOS IdApi: 41 fields, 328 bytes.
Design 81R removes the four Float fields into an explicit [Float dependency](float-v1.md).
Design 81J removes `ensurePrivateVtable`; 81K removes owner/parent getters and
setters. 81L removes the four Integer fields into an explicit
[Integer dependency](integer-v1.md). Object-model consumers use ordinary Vtable
messages. Existing v1-v4 native consumers
must be rebuilt.
Backward compatibility is not a requirement for this refactor.

The Rust mirror has compile-time offsets and exports a diagnostic descriptor.
The descriptor's format version remains 1, while its IdApi schema version is 5.
Its fingerprint matches the independent v5 manifest. The v5 fingerprint algorithm
is SHA-256 over compact, key-sorted UTF-8 JSON, excluding the `fingerprint` member.
The workspace tests verify that fingerprint and every compiled field offset.

`fixtures/idapi-v5/idapi_v5.h` is the current independent C header. The routine
native gate compiles the v5 client/module and checks the table and separately
loaded Integer, Float and object-model implementations. Other active foreign fixtures use this header.

`idapi-v1.json` and `fixtures/idapi-v1` preserve the historical 54-field ABI and
its frozen binary evidence. They are not changed to match v5. Their old opt-in
compatibility script applies only to a v1 floor; running that client against v5
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

[Collection coordination v1](collect-v1.md) moves collection phase order,
conditional settlement and the finalizer/reclamation loop into a loaded module.
Its per-invocation source preserves storage and metadata ownership; IdApi stays v4.

The [Float method module](../docs/design/81q-production-float-methods.md) uses
existing Method/module-init contracts and the explicit Integer dependency.
Its [export inventory](rust-float-dylib-v1.json) adds no IdApi or image fields.

`idapi-v4.json` and `fixtures/idapi-v4` preserve the 81L checkpoint.
[Float representation](../docs/design/81r-production-float-storage.md) now moves
the canonical objects and primitive operations into the Float module as well.

The [Word method module](../docs/design/81s-production-word-methods.md) uses
existing Method/module-init contracts and the explicit Integer dependency.
Its [export inventory](rust-word-dylib-v1.json) adds no IdApi or image fields.
