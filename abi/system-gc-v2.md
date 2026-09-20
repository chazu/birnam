# System GC installation v2

Design [81BK](../docs/design/81bk-production-gc-settings.md) adds an explicit
[GC Settings capability](gc-settings-v1.md) to the existing System GC installer.
The immutable descriptor stays 24 bytes, with version 2, extent at offset 8 and
install at offset 16. Obtain `birnam_system_gc_provider_v2` and attach through
`birnam_system_gc_dependency_v2`. The
[independent C mirror](fixtures/system-gc-v2/system_gc_v2.h) fixes its layout.
Validation remains inert; malformed descriptors and retargeting reject, while
the same pointer can be attached again.

The operation is `install(registration, execution, settings, behavior, gc,
liveObjects, autoGc, preciseGc)`. Prelude passes settings with its original
execution API. The installer retains that pair at its existing first-install
point, before registration callbacks. Reentry/reinstallation uses current
registration inputs without replacing the execution pair. All borrowed code,
descriptors and contexts remain pinned.

Methods retain the [v1 contract](system-gc-v1.md): argument checks, live nil and
Boolean identity, true precedence, query/Integer construction order, receiver
returns and registration order are unchanged. Valid setters call the selected
settings descriptor's live operation/context; query methods still use the
saved IdApi collection/live-count callbacks. There are no new language methods
or image-format changes.
