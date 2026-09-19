# Namespace reverse names v1

Design [81AX](../docs/design/81ax-production-namespace-names.md) moves
`exportedNameOf` out of IdApi. IdApi v8 contains 33 fields / 264 bytes; rebuild all
native consumers. No aggregate forwarding entry remains.

The [names descriptor](fixtures/namespace-names-v1/namespace_names_v1.h) is 32
bytes on arm64: version 1 at 0, extent at 8, nonnull mutable context at 16 and
required `name_of(context, value)` operation at 24. Descriptor/code/context stay
live on the owner thread. Each ordinary use snapshots the current operation and
context together; replacement has the same borrowed-return obligations. No
concurrent native mutation is permitted.

The [storage factory](namespace-v4.md) creates this service with a 40-byte
immutable floor descriptor: version 1, extent, optional opaque context, required
`preflight(context)` and `symbol_name(context, symbol)` primitives. A default call
performs preflight, invokes the selected storage reverse operation, then borrows
the Symbol bytes, including when the value or selected name is null. This keeps
owner/storage-observation checks, first-current-alias selection and null results
at their original points. Neither floor primitive allocates, collects, mutates,
sends, or invokes user callbacks. The borrowed name remains valid while its
Symbol is live; the binding roots both name and value, including null-valued names.

The floor exports `birnam_namespace_names_provider_v1` after initialization; it
returns the per-runtime service, not a global side namespace. The host validates
it once and attaches it to the declared Image, Alien, Portable Code and
Codegen/Reflect consumers through `birnam_namespace_names_dependency_v1` before
module_init. Null pointers, wrong version/extent, null context/operation and
retargeting fail; repeated attachment of the same descriptor succeeds. A native
embedder may attach its own pinned copy to select operations independently.

The [v3 Block declaration](native-block-construction-v3.md) compares both current
operands against the originals retained by the floor before any name call. A
wrapped function or changed context declines admission without being invoked.
Ordinary reflection, foreign-wrapper source recording and image lookup continue
to observe the live result, including alternate names and null. Nothing caches
an admission result or manufactures authority from a later alias.

Run `mise run namespace`, `mise run namespace-names`,
`scripts/namespace-names-image-smoke.sh PREFIX` and
`scripts/namespace-image-smoke.sh PREFIX` for focused debug state, admission,
live-consumer and startup/compiled/image/baked checks.
