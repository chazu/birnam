# Namespace export capability v1

> **Construction update (Design 83):** The pre-Id preparation, aggregate
> initialization and private provider exports described below are historical.
> Production constructs this service above Id and passes completed dependencies
> explicitly. Host selection uses [Runtime service requests](runtime-construction-v15.md#completed-service-selection),
> through Bootstrap and Id; the old provider export is removed.

A 32-byte live descriptor: version 1, byte size, opaque context, and
`export(context, const char *name, Oop value) -> Oop`. The independent
[C mirror](fixtures/namespace-export-v1/namespace_export_v1.h) fixes its layout.
A caller copies function and context at its original export-slot read. Earlier
callbacks may change either; an active invocation retains the selected pair.
Integer snapshots before construction and uses the same pair for both exports.
Float, Word and String read separately after owner callbacks and for each export.
Descriptor, selected context and code remain valid for every retained use.

[Namespace factory v8](namespace-v8.md) creates the canonical service. Each
Runtime owns its allocation; Id publishes the live descriptor through
`birnam_namespace_export_provider_v1` after `id_init`. Creation copies the
immutable 48-byte primitive floor descriptor, whose access operation performs
Id owner/observation admission and returns a transient primitive context.

The service rejects null names, scans through the first NUL, then performs
floor admission. It calls the selected intern/bind/mirror publication operation:
mirror receives the original Symbol and input value, while the export result
is the selected bind result. Empty/non-UTF8 names and null input/results retain
their meaning. Existing diagnostics and their precedence are unchanged.
Symbol interning and weak-dictionary representation remain Id primitives;
Globals remains a separate registry. No RuntimeState reference spans callbacks.

IdApi v11 removed `exportObj`; current v13 has 27 fields, 216 bytes. Native initialization
receives `(api, prior_state, presence, imports, exports, vtables)`. A saved execution API
retains exports at the same selection point, including early preparation.
Incoming registration and adoption use the incoming capability where the old
caller used its incoming API. Direct Native Loader forwards its saved selection.
There is no aggregate facade, callback-slot loan or API-address registry.

Float/Word prepare v4 append exports after imports. String prepare v2 receives
`(api, exports, allocation)`. Range/Vector/Map construction v4 change only
`publish(api, exports)`. Native Loader installation v6 receives execution exports
after imports, followed by vtable enumeration. The other five System/Block installers stay v3;
numeric and warm Block declarations stay v4 because their admitted paths do
not export. Native modules require rebuilding; image encoding is unchanged.

See [81BF](../docs/design/81bf-production-namespace-export.md) for caller audit
and validation evidence. `mise run namespace` includes the native publication
boundary and constructor-selection probes.
