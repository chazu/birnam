# ABI evidence

Production now enters through [the typed Id protocol](fixtures/id-protocol-v1/README.md),
then constructs Bootstrap, roots, Lifetime, [Nil/Boolean base values](fixtures/base-values-v1/README.md)
the [native Namespace owner](fixtures/namespace-owner-v1/README.md), and
[Object Model ownership/enumeration](fixtures/object-model-owner-v1/README.md).
The remaining aggregate Runtime and module interfaces below are migration
boundaries. Earlier descriptions of factory preparation before `id_init` apply
to the legacy path; current startup loads the real Id first. See
[Design 83](../docs/design/83-production-id-construction-and-lifetimes.md#implementation-evidence-and-remaining-boundary)
for the current construction and remaining package separation.

The current boundary is **IdApi v20**, with 18 fields / 144 bytes. Design
[81BO](../docs/design/81bo-production-native-stack-anchor.md) moves native
stack-anchor ownership into [Native Stack](native-stack-v1.md), exposing its
setter directly to the host. Design [81BN](../docs/design/81bn-production-finalizers.md)
moves native finalizer registration and inline callback construction into
[Finalizers](finalizers-v1.md).
Initializers receive `(api, state, presence, imports, exports, vtables,
observation, roots, settings, layouts, hooks, finalizers)`. [Native Loader v12](native-loader-v12.md)
forwards the matching finalizer selection. Early String preparation is v4;
Float/Word preparation and Range/Vector/Map installation are v6. Native
consumers require rebuilding; image encoding is unchanged.
[IdApi v20 manifest](idapi-v20.json) and the
[C fixture](fixtures/idapi-v20/README.md) describe the current boundary.

`idapi-v20.json` describes the current arm64 macOS IdApi.
Design 81BK removes `gcSetAuto` and `gcSetPrecise` into [GC Settings](gc-settings-v1.md).
Design 81BI removes `setMethodOverwriteHook` into [Method Observation](method-observation-v1.md).
Design 81BH removes `vtableSelectorAt` into the same explicit enumeration service.
Design 81BG removes `vtableCount` and `vtableAt` into [Object Model enumeration](vtable-enumeration-v2.md).
Design 81BF removes `exportObj` into the explicit [Namespace export capability](namespace-export-v1.md).
Design 81BE removes `import` into the explicit [Namespace import capability](namespace-import-v1.md),
following 81BB's [presence capability](namespace-presence-v1.md).
Native initialization takes `(IdApi *, void *prior_state, const NamespacePresenceV1 *, const NamespaceImportV1 *, const NamespaceExportV1 *, const VtableEnumerationV2 *, const MethodObservationV1 *, const RootRegistryV1 *, const GcSettingsV1 *, const PayloadLayoutV1 *, const TraceHooksV1 *, const FinalizersV1 *)`.
All native modules must be rebuilt for this signature and IdApi v20.
After `id_init`, the selected floor exposes `birnam_namespace_presence_provider_v1`,
`birnam_namespace_import_provider_v1` and `birnam_namespace_export_provider_v1`. The host and Native Loader pass these
selections explicitly. The floor also supplies `birnam_vtable_enumeration_provider_v2`
from the Object Model factory prepared before Id initialization. The Method Observation
factory is also prepared before Id; `birnam_method_observation_provider_v1` supplies
its matching registration capability as the seventh initialization argument. The
Root Registry factory is prepared before Id and exposes
`birnam_root_registry_provider_v1` as the eighth argument. GC Settings is also prepared before Id;
`birnam_gc_settings_provider_v1` supplies the ninth argument. Payload Layout is
prepared before Id and exposes `birnam_payload_layout_provider_v1` as the tenth
argument. Trace Hooks is prepared before Id and exposes
`birnam_trace_hooks_provider_v1` as the eleventh argument. Finalizers is also prepared before Id and exposes
`birnam_finalizers_provider_v1` as the twelfth argument. Native Stack is prepared
before Id and exposes `birnam_native_stack_provider_v1` directly to the host;
initialization stays at twelve arguments and Native Loader stays v12. Modules capture matching live capabilities when they save
their execution API; incoming registration/adoption uses the incoming selection
at its original call points. There is no API-address registry or production
callback-slot loan. Code, contexts and descriptors must remain valid for every
retained API use. Image encoding and language behavior are unchanged.

Design 81AX moves `exportedNameOf` to the explicit [Namespace names capability](namespace-names-v1.md).
Design 81X removes three String fields into an explicit [String dependency](string-v1.md).
Design 81T removes the four Word fields into an explicit [Word dependency](word-v1.md).
Design 81R removes the four Float fields into an explicit [Float dependency](float-v1.md).
Design 81J removes `ensurePrivateVtable`; 81K removes owner/parent getters and
setters. 81L removes the four Integer fields into an explicit
[Integer dependency](integer-v1.md). Object-model consumers use ordinary Vtable
messages. Existing v1-v19 native consumers
must be rebuilt.
Backward compatibility is not a requirement for this refactor.

The Rust mirror has compile-time offsets and exports a diagnostic descriptor.
The descriptor's format version remains 1, while its IdApi schema version is 20.
Its fingerprint matches the independent v20 manifest. The v20 fingerprint algorithm
is SHA-256 over compact, key-sorted UTF-8 JSON, excluding the `fingerprint` member.
The workspace tests verify that fingerprint and every compiled field offset.

`fixtures/idapi-v20/idapi_v20.h` is the current independent C header. The routine
native gate compiles the v20 client/module and checks the table and separately
loaded String, Integer, Float, Word and object-model implementations. Other active foreign fixtures use this header.

`idapi-v1.json` and `fixtures/idapi-v1` preserve the historical 54-field ABI and
its frozen binary evidence. They are not changed to match v20. Their old opt-in
compatibility script applies only to a v1 floor; running that client against v20
is unsupported. The archived v4 image remains historical evidence as before.

The Cargo workspace and `scripts/rust-install.sh` remain the build authority.
ABI manifests and fixtures are build/test evidence, not runtime dependencies.
The installed native inventory now includes the separate Integer and object-model dylibs.

`idapi-v2.json` and `fixtures/idapi-v2` preserve the 81J checkpoint.
The current optional Block-construction declaration is [v5](native-block-construction-v5.md).

`idapi-v3.json` and `fixtures/idapi-v3` preserve the 81K checkpoint.
The current optional numeric declaration is [v4](native-numeric-semantics-v4.md).

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

`idapi-v5.json` and `fixtures/idapi-v5` preserve the 81R checkpoint.
[Word representation](../docs/design/81t-production-word-storage.md) now belongs
to the loaded Word module as well.

The [Boolean method module](../docs/design/81u-production-boolean-methods.md)
uses existing Method/module-init interfaces and object-model messages.
Its [export inventory](rust-boolean-dylib-v1.json) adds no IdApi or image fields;
canonical singleton representation remains in Id at this checkpoint.

The [nil method module](../docs/design/81v-production-nil-methods.md) similarly
moves existing Object/nil methods from Collection, using current interfaces.
Its [export inventory](rust-nil-dylib-v1.json) adds no IdApi or image fields.

The [String method module](../docs/design/81w-production-string-methods.md) uses
current Method/IdApi interfaces and Integer/Float dependencies. Its
[export inventory](rust-string-dylib-v1.json) adds no IdApi or image fields.
[String representation and byte ownership](string-v1.md) now live in that module;
native Array splitting stays in Collection.

`idapi-v6.json` and `fixtures/idapi-v6` preserve the 81T checkpoint.

The [StringBuilder module](../docs/design/81y-production-string-builder.md) owns
its three-word native buffer and seven existing selectors, with explicit Integer
and String dependencies. Its [export inventory](rust-string-builder-dylib-v1.json)
adds no IdApi or image fields. Run `mise run string-builder` for the focused debug
C oracle; `scripts/string-builder-image-smoke.sh PREFIX` checks installed startup
and compiled images. Captured live buffers retain their existing nonportable RAW
image handling; compiled entries can construct builders and capture their Strings.

The [Range module](../docs/design/81ac-production-range-vector-methods.md) owns
its representation, construction and all eight Range methods plus Integer `to:`.
Its [current construction dependency](range-v4.md) preserves installation order
and a direct native Vector comma callback. Vector storage is mounted separately
through [Vector v4](vector-v4.md). The [export inventory](rust-range-dylib-v1.json)
adds no IdApi or image fields. Earlier Range interfaces remain historical evidence.

[Collection value policy](collection-value-v1.md) supplies Vector comparison and
Map key hashing through one immutable native dependency. It owns no objects and
uses the caller's selected IdApi and live Integer/Float/String callbacks. Its
[export inventory](rust-collection-value-dylib-v1.json) adds no IdApi fields.

[Vector v4](vector-v4.md) also owns all native Vector methods and sorting roots.
Map iteration uses its native bodies, and Range retains its native comma Method.
The [Vector export inventory](rust-vector-dylib-v1.json) records its explicit
Integer, String and collection-value dependencies.

[Map v4](map-v4.md) owns canonical objects, HAMT storage, methods, traversal
and printing. Collection installs it through the ordered construction boundary. The [Map export inventory](rust-map-dylib-v1.json) adds no IdApi fields.

[String collection v1](string-collection-v1.md) installs String split: through a
late adapter that depends on String and Vector. Its [export inventory](rust-string-collection-dylib-v1.json)
adds no IdApi fields, objects or roots; Collection preserves ordered installation.

[Printing v1](printing-v1.md) owns the three Object/Symbol printing defaults.
Prelude supplies separate registration and original execution tables at its
existing installation point. The [export inventory](rust-printing-dylib-v1.json)
adds no IdApi fields, objects or roots.

[System GC v2](system-gc-v2.md) owns System's four collector control methods.
Prelude supplies its selected behavior, selectors and original execution table at
the final four installation slots. The [export inventory](rust-system-gc-dylib-v1.json)
adds no IdApi fields, collector state, objects or roots.

[Text views v1](text-view-v1.md) supplies shared exact-String and printable byte
borrowing to Prelude consumers. The [export inventory](rust-text-view-dylib-v1.json)
adds no IdApi fields, objects, roots or owned buffers.

[System output v1](system-output-v1.md) owns print:/println: and stdout writing.
Prelude supplies selected behavior/selectors and its original execution table at
the first two registration slots. The [export inventory](rust-system-output-dylib-v1.json)
adds no IdApi fields, objects, roots or owned byte buffers.

[Error value v3](error-value-v3.md) supplies lazy Error factory dispatch and exact
recognition to Integer, Float, Word, Environment, System Input, File IO, Block Loop,
Native Loader and Source Loader. Each call receives explicit presence and
[import capabilities](namespace-import-v1.md). The
[export inventory](rust-error-value-dylib-v1.json) adds no IdApi fields, objects,
roots or retained runtime selection.

[System input v3](system-input-v3.md) owns readLine and stdin buffering. Prelude
supplies its original execution table at the existing registration slot. The
[export inventory](rust-system-input-dylib-v1.json) adds no IdApi fields, objects,
roots or representation changes.

[Environment v3](environment-v3.md) owns System getEnv: and live environment
lookup. Prelude supplies its first execution table at the original registration
slot. The [export inventory](rust-environment-dylib-v1.json) adds no IdApi fields,
objects, roots or representation changes.

[File IO v3](file-io-v3.md) owns System file reads and writes through a 24-byte
installer at their original Prelude binding positions. The
[export inventory](rust-file-io-dylib-v1.json) adds no IdApi fields, object
representation or image encoding.

[Block Loop v3](block-loop-v3.md) owns the native whileTrue:/whileFalse: methods
through a 24-byte installer. Prelude retains Block identity/layout and authored
stackless control still replaces the methods later. The
[export inventory](rust-block-loop-dylib-v1.json) adds no IdApi fields or image
encoding.

[Native Loader v12](native-loader-v12.md) owns System loadModule: through a 24-byte
installer, preserving the existing direct path without host dependency admission.
The [export inventory](rust-native-loader-dylib-v1.json) adds no IdApi fields,
objects or image encoding.

[Source Loader v3](source-loader-v3.md) owns System load: through a 24-byte
installer. Integer, String, Text View and Error Value attachments move with their
last Prelude consumers. The [export inventory](rust-source-loader-dylib-v1.json)
adds no IdApi fields, objects or image encoding.

[Namespace v8](namespace-v8.md) supplies a 176-byte immutable storage/service factory,
prepared explicitly before Id initialization. It owns ordered bindings and
independent contexts; Id consumes a borrowed binding view for roots. The
[export inventory](rust-namespace-dylib-v1.json) adds no IdApi fields or image
encoding. Current native hosts must attach this prerequisite before id_init.

`idapi-v7.json` and `fixtures/idapi-v7` preserve the 81X checkpoint. Current
Namespace [storage v8](namespace-v8.md) creates the per-runtime names service;
its four consumers attach the [names capability](namespace-names-v1.md) explicitly.

[Namespace enumeration v1](namespace-enumeration-v1.md) replaces the private Id
count/name-at functions with Namespace-owned descriptors published by the selected
floor. Reflect retains live discovery and its original operation order.
Image encoding remains unchanged.

`idapi-v8.json` and `fixtures/idapi-v8` preserve the 81AX-81BA checkpoint.
Namespace [factory v8](namespace-v8.md) owns canonical interned lookup, publication and the
live presence/import/export services. The restricted interner remains in Id; missing
names are still interned before the selected raw find. All three operations use explicit capabilities. `idapi-v9.json` and `fixtures/idapi-v9` preserve 81BB-81BD; v10 preserves 81BE.

[Isolate OOP bridges v2](isolate-oop-v2.md) receive the incoming API and import
capability explicitly; wire encoding and worker lifecycle are unchanged.

`idapi-v19.json` and `fixtures/idapi-v19` preserve the 81BN checkpoint.
[Native Stack v1](native-stack-v1.md) owns the native root anchor and removes
`gcSetStackBottom` from IdApi v20 without moving register capture or scanning.

Native Mark v1 (81BP) owns native mark scopes and emission policy while Id keeps
per-floor/thread selection and Rust unwinding. See [the contract](native-mark-v1.md)
and [implementation manifest](rust-native-mark-dylib-v1.json). IdApi v20,
Native Loader v12, twelve-argument initialization and image v8 remain unchanged.

Native Stack factory v2 (81BQ) adds incremental native word scanning to the
existing module while preserving the v1 setter capability. See
[the v2 contract](native-stack-v2.md). Register capture and interval validation
remain in Id; IdApi v20 and native initialization are unchanged.
