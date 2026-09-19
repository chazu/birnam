# Range protocol construction v1

> Historical 81Z checkpoint. Production now uses [Range v1](range-v1.md);
> the method-only provider/dependency and Rust mirror have been removed.

Design [81Z](../docs/design/81z-production-range-methods.md) extracts six Range
methods while preserving Collection's original construction/publication schedule.
Range loads before Collection. The host discovers `birnam_range_methods_provider_v1`
and attaches its immutable descriptor to Collection through
`birnam_range_methods_dependency_v1` before invoking Collection's `module_init`.
Both libraries and the descriptor stay pinned for the process. Other native
embedders must perform the same attachment; a missing dependency is an error.

The arm64 descriptor is 24 bytes: version 1 at offset 0, `size_t byte_size` at 8,
and the installer function pointer at 16. The independent C definition is in
[`fixtures/range-methods-v1`](fixtures/range-methods-v1/range_methods_v1.h).
Null, wrong-version/size, missing-operation and different-descriptor reattachments
are rejected; attaching the same descriptor is idempotent.

`install(IdApi *, behavior, value1, value2, printString, reversed, comma)` runs
synchronously on the owner thread at Collection's original Range-binding point.
The Oops are its retained behavior and cached selectors. The last two arguments
are direct Method implementations, retained in the behavior just as before;
there is no forwarded send. Registration order remains `size`, `includes:`,
`do:`, `inject:into:`, `at:`, `reversed`, `printString`, `,`.

The first installation's Id table and selectors are retained for method calls
and remain live for the process; Range registers its selector root provider.
Subsequent installs retain that state and use their supplied table for method
registration. Reinitializing Collection still reinstalls all eight methods and
preserves its cached family identities. Preparing the Range library again does
not itself register methods or consult namespace aliases.

The six extracted implementations still read the existing 16-byte Range payload
(two RAW i64 bounds). Collection owns its allocation, layout, canonical objects
and Integer `to:` at this checkpoint. Image's generic RAW encoding is unchanged.
This descriptor adds no IdApi fields or ordinary Range operations.

Run `mise run range` for the independent native oracle and
`scripts/range-image-smoke.sh PREFIX` for installed debug startup/compiled images.
